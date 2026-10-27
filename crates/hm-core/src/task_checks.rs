//! Real EL0 task creation, owned completion and cancellation; design 0029.
#![forbid(unsafe_code)]
use super::frames::{Block, Pool};
use super::process::Executor;
use super::process_group::{
    self, End, Error, Event, Group, RootEvidence, TaskCompletion, TaskId, TaskState, TaskTurn, Turn,
};
use super::task_inputs as inputs;
use super::user_space::Factory;
use core::fmt::Write;
use hm_kernel::boot_image;
use hm_kernel::elf::Image;

type Check<T = ()> = Result<T, ()>;
const NAMES: [&str; 25] = [
    "supervisor",
    "copy_inputs",
    "wait_poll_ready",
    "wait_budget_child",
    "rights_console",
    "foreign_authority",
    "stale_reuse",
    "malformed_outer",
    "malformed_descriptor",
    "copy_faults",
    "child_fault",
    "held_capacity",
    "child_frame_cap",
    "group_frame_cap",
    "allocation_rollback",
    "parent_exit",
    "parent_fault",
    "parent_budget",
    "parent_call_limit",
    "parent_cancel_ready",
    "parent_cancel_waiting",
    "limit_spawn",
    "limit_wait",
    "limit_cancel",
    "drop_waiting",
];
const PATHS: [&str; 25] = [
    "bin/supervisor",
    "bin/copy_inputs",
    "bin/wait_poll_ready",
    "bin/wait_budget_child",
    "bin/rights_console_policy",
    "bin/foreign_authority",
    "bin/stale_reuse",
    "bin/malformed_outer",
    "bin/malformed_descriptor",
    "bin/copy_faults",
    "bin/child_fault",
    "bin/held_capacity",
    "bin/child_frame_cap",
    "bin/group_frame_cap",
    "bin/allocation_rollback",
    "bin/parent_exit",
    "bin/parent_fault",
    "bin/parent_budget",
    "bin/parent_call_limit",
    "bin/parent_cancel_ready",
    "bin/parent_cancel_waiting",
    "bin/limit_spawn",
    "bin/limit_wait",
    "bin/limit_cancel",
    "bin/drop_waiting",
];
const CALLS: [usize; 25] = [
    9, 11, 8, 3, 4, 6, 7, 12, 20, 9, 5, 11, 4, 6, 6, 3, 2, 2, 64, 2, 2, 64, 64, 64, 2,
];
fn demand(value: bool) -> Check {
    if value { Ok(()) } else { Err(()) }
}
fn expected(case: usize) -> End {
    match case {
        15 => End::Exited(17),
        16 => End::Fault,
        17 => End::Budget,
        18 | 21..=23 => End::CallLimit,
        19 | 20 => End::Cancelled,
        _ => End::Exited(0),
    }
}
fn completion_word(end: End) -> u64 {
    use hm_kernel::task_wire::Completion as C;
    match end {
        End::Exited(c) => C::Exited(c),
        End::Fault => C::Fault,
        End::Budget => C::Budget,
        End::CallLimit => C::CallLimit,
        End::TurnLimit => C::TurnLimit,
        End::Cancelled => C::Cancelled,
        End::Unavailable => C::Unavailable,
    }
    .encode()
}

#[inline(never)]
fn image_pages(archive: &'static [u8], path: &str) -> Check<usize> {
    let image = boot_image::admit(archive, path).map_err(|_| ())?;
    Ok(image.page_count())
}
#[inline(never)]
fn construct<'e, 'p>(
    factory: &Factory<'p>,
    executor: &'e mut Executor,
    archive: &'static [u8],
    path: &str,
    console: bool,
) -> Result<Group<'e, 'p>, Error> {
    let image = boot_image::admit(archive, path).map_err(Error::Catalog)?;
    Group::new_task(executor, factory.reborrow(), &image, archive, console)
}
enum StepFacts {
    Step {
        id: TaskId,
        state: TaskState,
        event: Event,
        calls: usize,
        terminal: Option<End>,
    },
    Completed,
}
#[inline(never)]
fn check_publication(group: &Group<'_, '_>, completion: &TaskCompletion) -> Check {
    demand(completion.live_frames() == inputs::METADATA_PAGES)?;
    demand(matches!(
        completion.closure().evidence(),
        RootEvidence::Executed(_)
    ))?;
    let held = group.task_completion(group.task_root().map_err(|_| ())?);
    demand(matches!(&held, Ok(Some(value)) if value == completion))
}
struct Checks<'a> {
    pool: &'a Pool,
    output: &'a mut dyn Write,
    baseline: usize,
    root: u64,
    vectors: u64,
    case_steps: usize,
    pass_steps: usize,
    pressure: usize,
}
impl Checks<'_> {
    fn balance(&self, group: &Group<'_, '_>) -> Check {
        demand(
            self.baseline
                .checked_sub(self.pressure)
                .and_then(|n| n.checked_sub(free(self.pool)))
                == Some(group.charged_frames()),
        )
    }
    #[inline(never)]
    fn group(&mut self, group: &Group<'_, '_>, fixture: &str) -> Check<TaskId> {
        let root = group.task_root().map_err(|_| ())?;
        let info = group.info(root).map_err(|_| ())?;
        demand(info.state() == TaskState::Ready)?;
        demand(info.box_frames() == inputs::PROCESS_PAGES)?;
        demand(
            group.charged_frames()
                == inputs::METADATA_PAGES + info.process_frames() + info.box_frames(),
        )?;
        self.balance(group)?;
        let after = free(self.pool);
        writeln!(
            self.output,
            "HMTASK GROUP group={} limit=512 metadata={} before={} after={after}",
            group.namespace(),
            inputs::METADATA_PAGES,
            self.baseline
        )
        .map_err(|_| ())?;
        let entry = image_entry(
            if fixture == "group_frame_cap" {
                inputs::LARGE_ARCHIVE
            } else {
                inputs::ARCHIVE
            },
            fixture,
        )?;
        writeln!(self.output,"HMTASK ROOT group={} slot={} generation={} launch_generation={} console_generation={} fixture={fixture} entry={entry} root={} process_frames={} box_frames={} before={} after={after}",
            group.namespace(),root.slot(),root.generation(),group.task_launch_generation().map_err(|_|())?,info.console_generation().ok_or(())?,info.root(),info.process_frames(),info.box_frames(),self.baseline).map_err(|_|())?;
        Ok(root)
    }
    #[inline(never)]
    fn step(&mut self, group: &mut Group<'_, '_>) -> Check<StepFacts> {
        let owned = group.turn();
        let turn = match &owned {
            Ok(Turn::Task(turn)) => turn,
            _ => return Err(()),
        };
        self.balance(group)?;
        self.inspect_turn(group, turn)
    }
    #[inline(never)]
    fn inspect_turn(&mut self, group: &Group<'_, '_>, turn: &TaskTurn) -> Check<StepFacts> {
        match turn {
            TaskTurn::Step {
                report,
                terminal,
                management,
            } => {
                self.case_steps = self.case_steps.checked_add(1).ok_or(())?;
                self.pass_steps = self.pass_steps.checked_add(1).ok_or(())?;
                demand(self.case_steps <= 1024 && self.pass_steps <= 8192)?;
                demand(
                    report.controls()
                        && report.boot_root() == self.root
                        && report.vbar() == self.vectors,
                )?;
                demand(
                    report.live_frames() == group.charged_frames()
                        && management.live_frames() == group.charged_frames(),
                )?;
                demand(management.root() == group.task_root().map_err(|_| ())?)?;
                demand(management.reaped() <= 2 && management.children_remaining() <= 2)?;
                demand(*terminal != Some(End::Unavailable) && *terminal != Some(End::TurnLimit))?;
                if let Some(timing) = report.timing() {
                    demand(
                        timing.accounted()
                            && timing.end() >= timing.start()
                            && timing.comparator() <= timing.hard_end(),
                    )?;
                }
                if let Some(closed) = management.closed_root() {
                    demand(matches!(closed.evidence(), RootEvidence::Executed(_)))?;
                }
                Ok(StepFacts::Step {
                    id: report.task().id(),
                    state: report.task().state(),
                    event: report.event(),
                    calls: report.calls(),
                    terminal: *terminal,
                })
            }
            TaskTurn::RootCompleted {
                completion,
                management,
            } => {
                demand(management.children_remaining() == 0 && management.closing())?;
                demand(
                    management.live_frames() == inputs::METADATA_PAGES
                        && completion.live_frames() == inputs::METADATA_PAGES,
                )?;
                check_publication(group, completion)?;
                Ok(StepFacts::Completed)
            }
            TaskTurn::Administrative { .. } => Err(()),
        }
    }
    #[inline(never)]
    fn cancel(&mut self, group: &mut Group<'_, '_>, root: TaskId, state: &str) -> Check {
        let before = free(self.pool);
        group.request_cancel(root).map_err(|_| ())?;
        demand(free(self.pool) == before)?;
        writeln!(self.output,"HMTASK CONTROL group={} operation=cancel_root state={state} before={before} after={before}",group.namespace()).map_err(|_|())
    }
    #[inline(never)]
    fn reap(&mut self, group: &mut Group<'_, '_>, root: TaskId, case: usize) -> Check {
        let owned = group.task_completion(root);
        let done = match &owned {
            Ok(Some(done)) => done,
            _ => return Err(()),
        };
        let closure = done.closure();
        demand(closure.root().id() == root && closure.end() == expected(case))?;
        let RootEvidence::Executed(report) = closure.evidence() else {
            return Err(());
        };
        demand(report.calls() == CALLS[case])?;
        demand(group.info(root).map_err(|_| ())?.state() == TaskState::Completed)?;
        let before = free(self.pool);
        let reaped = group.task_reap(root);
        demand(matches!(&reaped, Ok(value) if value == done))?;
        demand(free(self.pool) == self.baseline && group.charged_frames() == 0)?;
        writeln!(
            self.output,
            "HMTASK REAP_ROOT group={} outcome={} before={before} after={}",
            group.namespace(),
            completion_word(closure.end()),
            free(self.pool)
        )
        .map_err(|_| ())
    }
    #[inline(never)]
    fn dropped(&mut self, namespace: u64, before: usize) -> Check {
        demand(self.pressure == 0 && free(self.pool) == self.baseline)?;
        writeln!(
            self.output,
            "HMTASK DROP group={namespace} before={before} after={}",
            self.baseline
        )
        .map_err(|_| ())
    }
}
#[inline(never)]
fn image_entry(archive: &'static [u8], fixture: &str) -> Check<u64> {
    let path = if fixture == "rights_console_wait_only" {
        "bin/rights_console_wait_only"
    } else if fixture == "rights_console_policy" {
        "bin/rights_console_policy"
    } else {
        PATHS[NAMES.iter().position(|name| *name == fixture).ok_or(())?]
    };
    let image = boot_image::admit(archive, path).map_err(|_| ())?;
    Ok(image.entry())
}

#[derive(Clone, Copy)]
enum Gate {
    Admit,
    Info,
    Completion,
    Reap,
    Cancel,
    TaskCompletion,
    TaskReap,
    Root,
    Idle,
    NoCompletion,
    PendingReap,
}
impl Gate {
    fn operation(self) -> &'static str {
        match self {
            Self::Admit => "admit",
            Self::Info => "info",
            Self::Completion => "completion",
            Self::Reap => "reap",
            Self::Cancel => "cancel",
            Self::TaskCompletion | Self::NoCompletion => "task_completion",
            Self::TaskReap | Self::PendingReap => "task_reap",
            Self::Root => "task_root",
            Self::Idle => "turn",
        }
    }
    fn result(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::NoCompletion => "none",
            Self::PendingReap => "pending",
            _ => "mode",
        }
    }
}
#[inline(never)]
fn reject_admit(group: &mut Group<'_, '_>) -> Check {
    let owned = boot_image::admit(inputs::ARCHIVE, "bin/worker");
    let image = owned.as_ref().map_err(|_| ())?;
    demand(group.admit(image, true, None) == Err(Error::Mode))
}
#[inline(never)]
fn gate(
    checks: &mut Checks<'_>,
    group: &mut Group<'_, '_>,
    id: TaskId,
    phase: &str,
    op: Gate,
) -> Check {
    let before = free(checks.pool);
    let charged = group.charged_frames();
    let issuers = group.task_issuance_snapshot();
    match op {
        Gate::Admit => reject_admit(group)?,
        Gate::Info => demand(group.info(id) == Err(Error::Mode))?,
        Gate::Completion => demand(group.completion(id) == Err(Error::Mode))?,
        Gate::Reap => demand(group.reap(id) == Err(Error::Mode))?,
        Gate::Cancel => demand(group.request_cancel(id) == Err(Error::Mode))?,
        Gate::TaskCompletion => demand(group.task_completion(id) == Err(Error::Mode))?,
        Gate::TaskReap => demand(group.task_reap(id) == Err(Error::Mode))?,
        Gate::Root => demand(group.task_root() == Err(Error::Mode))?,
        Gate::Idle => demand(group.turn() == Ok(Turn::Idle))?,
        Gate::NoCompletion => demand(group.task_completion(id) == Ok(None))?,
        Gate::PendingReap => demand(group.task_reap(id) == Err(Error::Pending))?,
    }
    demand(
        free(checks.pool) == before
            && group.charged_frames() == charged
            && group.task_issuance_snapshot() == issuers,
    )?;
    writeln!(checks.output,"HMTASK GATE group={} phase={phase} op={} slot={} generation={} result={} before={before} after={before} charged_before={charged} charged_after={charged} task_next_before={} task_next_after={} console_before={} console_after={}",
        group.namespace(),op.operation(),id.slot(),id.generation(),op.result(),issuers.0,issuers.0,issuers.1.unwrap_or(0),issuers.1.unwrap_or(0)).map_err(|_|())
}
fn active_gates(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, root: TaskId) -> Check {
    for op in [Gate::Admit, Gate::Completion, Gate::Reap] {
        gate(checks, group, root, "active", op)?;
    }
    Ok(())
}
fn child_gates(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, child: TaskId) -> Check {
    for op in [
        Gate::Info,
        Gate::Cancel,
        Gate::TaskCompletion,
        Gate::TaskReap,
    ] {
        gate(checks, group, child, "child", op)?;
    }
    Ok(())
}
fn pending_gates(
    checks: &mut Checks<'_>,
    group: &mut Group<'_, '_>,
    root: TaskId,
    phase: &str,
) -> Check {
    for op in [Gate::NoCompletion, Gate::PendingReap] {
        gate(checks, group, root, phase, op)?;
    }
    Ok(())
}
fn closed_gates(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, root: TaskId) -> Check {
    for op in [
        Gate::Admit,
        Gate::Info,
        Gate::Completion,
        Gate::Reap,
        Gate::Cancel,
        Gate::TaskCompletion,
        Gate::TaskReap,
        Gate::Root,
        Gate::Idle,
    ] {
        gate(checks, group, root, "closed", op)?;
    }
    Ok(())
}
#[inline(never)]
fn drive(
    checks: &mut Checks<'_>,
    group: &mut Group<'_, '_>,
    root: TaskId,
    case: usize,
    gates: bool,
) -> Check {
    let mut child_checked = false;
    let mut closing_checked = false;
    let mut waiting_checked = false;
    let mut cancelled = false;
    loop {
        match checks.step(group)? {
            StepFacts::Step {
                id,
                state,
                event,
                calls,
                ..
            } => {
                if gates && id != root && !child_checked {
                    child_gates(checks, group, id)?;
                    child_checked = true;
                }
                if id == root {
                    if gates && state == TaskState::Closing && !closing_checked {
                        pending_gates(checks, group, root, "closing")?;
                        closing_checked = true;
                    }
                    if case == 3 && state == TaskState::Waiting && !waiting_checked {
                        pending_gates(checks, group, root, "waiting")?;
                        waiting_checked = true;
                    }
                    if case == 19 && event == Event::Svc && calls == 2 && !cancelled {
                        demand(state == TaskState::Ready)?;
                        checks.cancel(group, root, "ready")?;
                        cancelled = true;
                    }
                    if case == 20 && state == TaskState::Waiting && !cancelled {
                        demand(calls == 2)?;
                        checks.cancel(group, root, "waiting")?;
                        cancelled = true;
                    }
                }
            }
            StepFacts::Completed => {
                if gates {
                    demand(child_checked && closing_checked)?;
                }
                if case == 3 {
                    demand(waiting_checked)?;
                }
                if case == 19 || case == 20 {
                    demand(cancelled)?;
                }
                return Ok(());
            }
        }
    }
}
#[inline(never)]
fn one(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    case: usize,
    variant: bool,
) -> Check {
    let archive = if case == 13 {
        inputs::LARGE_ARCHIVE
    } else {
        inputs::ARCHIVE
    };
    let path = if case == 4 && variant {
        "bin/rights_console_wait_only"
    } else {
        PATHS[case]
    };
    let fixture = if case == 4 {
        if variant {
            "rights_console_wait_only"
        } else {
            "rights_console_policy"
        }
    } else {
        NAMES[case]
    };
    let (namespace, before) = {
        let mut owned = construct(factory, executor, archive, path, case != 4 || variant);
        let group = owned.as_mut().map_err(|_| ())?;
        let root = checks.group(group, fixture)?;
        let gates = case == 0 && !variant;
        if gates {
            active_gates(checks, group, root)?;
        }
        drive(checks, group, root, case, gates)?;
        checks.reap(group, root, case)?;
        if gates {
            closed_gates(checks, group, root)?;
        }
        (group.namespace(), free(checks.pool))
    };
    checks.dropped(namespace, before)
}

#[inline(never)]
fn wait_until_parked(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, root: TaskId) -> Check {
    loop {
        match checks.step(group)? {
            StepFacts::Step {
                id,
                state,
                calls,
                terminal,
                ..
            } => {
                if id == root {
                    demand(terminal.is_none())?;
                    if state == TaskState::Waiting {
                        return demand(calls == 2);
                    }
                }
            }
            _ => return Err(()),
        }
    }
}
#[inline(never)]
fn drop_waiting(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let (namespace, before) = {
        let mut owned = construct(factory, executor, inputs::ARCHIVE, PATHS[24], true);
        let group = owned.as_mut().map_err(|_| ())?;
        let root = checks.group(group, NAMES[24])?;
        wait_until_parked(checks, group, root)?;
        demand(group.info(root).map_err(|_| ())?.state() == TaskState::Waiting)?;
        (group.namespace(), free(checks.pool))
    };
    writeln!(
        checks.output,
        "HMTASK CONTROL group={namespace} operation=drop state=waiting before={before} after={}",
        free(checks.pool)
    )
    .map_err(|_| ())?;
    checks.dropped(namespace, before)
}

fn hold(pool: &Pool, leave: usize) -> Check<Block<'_>> {
    let pages = free(pool).checked_sub(leave).ok_or(())?;
    demand(pages != 0)?;
    let pressure = pool
        .allocate_heap(pages.checked_mul(4096).ok_or(())?, 4096)
        .map_err(|_| ())?;
    demand(free(pool) == leave)?;
    Ok(pressure)
}
#[inline(never)]
fn reject_setup(factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let image = boot_image::admit(inputs::ARCHIVE, PATHS[0]);
    let image = image.as_ref().map_err(|_| ())?;
    let rejected = Group::new_task(executor, factory.reborrow(), image, inputs::ARCHIVE, true);
    demand(matches!(&rejected, Err(Error::Resources)))?;
    drop(rejected);
    Ok(())
}
#[inline(never)]
fn setup_phase(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    phase: usize,
    leave: usize,
) -> Check {
    demand(free(checks.pool) == checks.baseline)?;
    let pressure = hold(checks.pool, leave)?;
    reject_setup(factory, executor)?;
    demand(free(checks.pool) == leave)?;
    let snapshot = executor.task_issuance_snapshot();
    demand(snapshot.0 == [1, 1, 2, 3, 4, 5][phase])?;
    demand(snapshot.1 == [None, None, None, None, Some(2), Some(3)][phase])?;
    drop(pressure);
    demand(free(checks.pool) == checks.baseline)?;
    let stage = [
        "image1",
        "image2",
        "root_load",
        "root_startup",
        "root_box",
        "authority",
    ][phase];
    writeln!(
        checks.output,
        "HMTASK SETUP stage={stage} free={leave} before={} after={} published=0",
        checks.baseline, checks.baseline
    )
    .map_err(|_| ())
}
#[inline(never)]
fn setup_failures(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check {
    // Fresh issuer: root startup failure burns console generation1; box failure
    // prepares generation2; authority-box failure prepares generation3.
    demand(executor.task_issuance_snapshot() == (1, None))?;
    let n = image_pages(inputs::ARCHIVE, PATHS[0])?
        .checked_add(28)
        .ok_or(())?;
    let images = inputs::IMAGE_PAGES.checked_mul(2).ok_or(())?;
    for (phase, leave) in [
        0,
        inputs::IMAGE_PAGES,
        images,
        images + n,
        images + n + 1,
        images + n + 1 + inputs::PROCESS_PAGES,
    ]
    .into_iter()
    .enumerate()
    {
        setup_phase(checks, factory, executor, phase, leave)?;
    }
    Ok(())
}
#[inline(never)]
fn failed_spawn_boundary(
    checks: &mut Checks<'_>,
    group: &mut Group<'_, '_>,
    root: TaskId,
    call: usize,
) -> Check {
    loop {
        match checks.step(group)? {
            StepFacts::Step {
                id,
                state,
                event,
                calls,
                terminal,
            } => {
                demand(id == root && terminal.is_none())?;
                demand(calls <= call)?;
                if event == Event::Svc && calls == call {
                    return demand(state == TaskState::Ready);
                }
            }
            _ => return Err(()),
        }
    }
}
#[inline(never)]
fn child_pressure_phase(
    checks: &mut Checks<'_>,
    group: &mut Group<'_, '_>,
    root: TaskId,
    phase: usize,
    leave: usize,
    console: u32,
) -> Check {
    let before = free(checks.pool);
    let charged = group.charged_frames();
    let task_next = group.task_issuance_snapshot().0;
    let pressure = hold(checks.pool, leave)?;
    let held = before.checked_sub(leave).ok_or(())?;
    checks.pressure = held;
    let stage = ["load", "startup", "box"][phase];
    writeln!(checks.output,"HMTASK PRESSURE group={} phase=hold stage={stage} before={before} after={leave} held={held}",group.namespace()).map_err(|_|())?;
    failed_spawn_boundary(checks, group, root, phase + 1)?;
    demand(free(checks.pool) == leave && group.charged_frames() == charged)?;
    let snapshot = group.task_issuance_snapshot();
    demand(snapshot.0 == task_next.checked_add(1).ok_or(())?)?;
    demand(
        snapshot.1
            == Some(if phase == 2 {
                console.checked_add(2).ok_or(())?
            } else {
                console
            }),
    )?;
    drop(pressure);
    checks.pressure = 0;
    demand(free(checks.pool) == before)?;
    writeln!(checks.output,"HMTASK PRESSURE group={} phase=release stage={stage} before={leave} after={before} held={held}",group.namespace()).map_err(|_|())
}
#[inline(never)]
fn allocation_rollback(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check {
    let n = image_pages(inputs::ARCHIVE, "bin/worker")?
        .checked_add(28)
        .ok_or(())?;
    let (namespace, before) = {
        let mut owned = construct(factory, executor, inputs::ARCHIVE, PATHS[14], true);
        let group = owned.as_mut().map_err(|_| ())?;
        let root = checks.group(group, NAMES[14])?;
        let console = group
            .info(root)
            .map_err(|_| ())?
            .console_generation()
            .ok_or(())?;
        for (phase, leave) in [0, n, n + 1].into_iter().enumerate() {
            child_pressure_phase(checks, group, root, phase, leave, console)?;
        }
        drive(checks, group, root, 14, false)?;
        demand(group.last_prepared_generation() == Some(console.checked_add(3).ok_or(())?))?;
        checks.reap(group, root, 14)?;
        (group.namespace(), free(checks.pool))
    };
    checks.dropped(namespace, before)
}
#[inline(never)]
fn artifact_bounds() -> Check {
    demand(
        inputs::PROCESS_BYTES == process_group::PROCESS_BYTES
            && inputs::PROCESS_PAGES == process_group::BOX_FRAMES,
    )?;
    demand(
        inputs::IMAGE_BYTES == core::mem::size_of::<Image<'static>>()
            && inputs::IMAGE_PAGES == inputs::IMAGE_BYTES.div_ceil(4096),
    )?;
    demand(inputs::AUTHORITY_PAGES == inputs::AUTHORITY_BYTES.div_ceil(4096))?;
    demand(inputs::METADATA_PAGES == 2 * inputs::IMAGE_PAGES + inputs::AUTHORITY_PAGES)?;
    let root = image_pages(inputs::LARGE_ARCHIVE, PATHS[13])?
        .checked_add(29 + inputs::PROCESS_PAGES)
        .ok_or(())?;
    let child = image_pages(inputs::LARGE_ARCHIVE, "bin/worker")?
        .checked_add(29 + inputs::PROCESS_PAGES)
        .ok_or(())?;
    let total = inputs::METADATA_PAGES.checked_add(root).ok_or(())?;
    demand(child <= 128 && total <= 512 && total + child <= 512 && total + 2 * child > 512)
}
#[inline(never)]
pub fn run(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> bool {
    checks(pool, factory, executor, root, vectors, output).is_ok()
}
#[inline(never)]
fn checks(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Check {
    artifact_bounds()?;
    let baseline = free(pool);
    let mut checks = Checks {
        pool,
        output,
        baseline,
        root,
        vectors,
        case_steps: 0,
        pass_steps: 0,
        pressure: 0,
    };
    setup_failures(&mut checks, factory, executor)?;
    header(&mut checks, executor)?;
    for pass in 0..2 {
        checks.pass_steps = 0;
        for (case, name) in NAMES.iter().enumerate() {
            checks.case_steps = 0;
            demand(free(pool) == baseline && checks.pressure == 0)?;
            let index = pass * 25 + case;
            case_record(&mut checks, index, pass, name, false)?;
            match case {
                4 => {
                    one(&mut checks, factory, executor, case, false)?;
                    one(&mut checks, factory, executor, case, true)?;
                }
                14 => allocation_rollback(&mut checks, factory, executor)?,
                24 => {
                    drop_waiting(&mut checks, factory, executor)?;
                    one(&mut checks, factory, executor, 0, true)?;
                }
                _ => one(&mut checks, factory, executor, case, false)?,
            }
            demand(free(pool) == baseline && checks.pressure == 0)?;
            case_record(&mut checks, index, pass, name, true)?;
        }
    }
    writeln!(checks.output, "HMTASK END 50").map_err(|_| ())
}

#[inline(never)]
fn header(checks: &mut Checks<'_>, executor: &Executor) -> Check {
    writeln!(checks.output, "HMTASK BEGIN 50").map_err(|_| ())?;
    writeln!(checks.output,"HMTASK INFO wire=1 cpus=1 frequency={} total_ticks={} quantum_ticks={} process_bytes={} process_pages={} image_bytes={} image_pages={} authority_bytes={} authority_pages={}",
        executor.frequency(),executor.ticks(),executor.frequency()/1000,inputs::PROCESS_BYTES,inputs::PROCESS_PAGES,inputs::IMAGE_BYTES,inputs::IMAGE_PAGES,inputs::AUTHORITY_BYTES,inputs::AUTHORITY_PAGES).map_err(|_|())?;
    Ok(())
}

#[inline(never)]
fn case_record(
    checks: &mut Checks<'_>,
    index: usize,
    pass: usize,
    name: &str,
    done: bool,
) -> Check {
    let baseline = checks.baseline;
    if done {
        writeln!(
            checks.output,
            "HMTASK PASS index={index} pass={pass} name={name} before={baseline} after={baseline}"
        )
        .map_err(|_| ())
    } else {
        writeln!(
            checks.output,
            "HMTASK CASE index={index} pass={pass} name={name} before={baseline}"
        )
        .map_err(|_| ())
    }
}

// Keep allocator bitmap snapshots out of callers retained across ELF allocation.
#[inline(never)]
fn free(pool: &Pool) -> usize {
    pool.stats().free
}
