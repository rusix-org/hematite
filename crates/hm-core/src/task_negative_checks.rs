//! Actual fixed task failure boundaries; no injected state is manufactured here.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::process::{Executor, NegativeCase, Process, Reason};
use super::process_group::{
    self, End, Error, Group, HookStatus, RootEvidence, SeedKind, TaskId, TaskState, TaskTurn, Turn,
};
use super::user_space::{Factory, Space};
use core::fmt::Write;
use hm_kernel::boot_image;
mod layout {
    include!("../fixtures/task-negative-cases/layout.rs");
}
type Check<T = ()> = Result<T, ()>;
fn demand(ok: bool) -> Check {
    if ok { Ok(()) } else { Err(()) }
}
fn expected(case: NegativeCase) -> End {
    match case {
        NegativeCase::FinalBudgetSpawn | NegativeCase::FinalBudgetPark => End::Budget,
        NegativeCase::CapabilityExhaustion | NegativeCase::SlotExhaustion => End::Exited(0),
        _ => End::Unavailable,
    }
}
fn administrative(case: NegativeCase) -> bool {
    matches!(
        case,
        NegativeCase::WakeInstallFailure
            | NegativeCase::ChildRetireReady
            | NegativeCase::ChildRetireWaiting
    )
}
fn word(end: End) -> u64 {
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
fn path(case: NegativeCase) -> &'static str {
    match case {
        NegativeCase::FinalBudgetSpawn => "bin/final_budget_spawn",
        NegativeCase::FinalBudgetPark => "bin/final_budget_park",
        NegativeCase::PlatformReply => "bin/platform_reply",
        NegativeCase::PlatformPark => "bin/platform_park",
        NegativeCase::AbandonDispatch => "bin/abandon_dispatch",
        NegativeCase::WakeInstallFailure => "bin/wake_install_failure",
        NegativeCase::CapabilityExhaustion => "bin/capability_exhaustion",
        NegativeCase::SlotExhaustion => "bin/slot_exhaustion",
        NegativeCase::ChildRetireReady => "bin/child_retire_ready",
        NegativeCase::ChildRetireWaiting => "bin/child_retire_waiting",
    }
}
#[inline(never)]
fn free(pool: &Pool) -> usize {
    pool.stats().free
}
#[inline(never)]
fn entry(case: NegativeCase) -> Check<u64> {
    boot_image::admit(Group::negative_archive(), path(case))
        .map(|i| i.entry())
        .map_err(|_| ())
}
struct Checks<'a> {
    pool: &'a Pool,
    output: &'a mut dyn Write,
    baseline: usize,
    root: u64,
    vectors: u64,
    case: NegativeCase,
    steps: usize,
}
impl Checks<'_> {
    fn balance(&self, group: &Group<'_, '_>) -> Check {
        demand(self.baseline.checked_sub(free(self.pool)) == Some(group.charged_frames()))
    }
    #[inline(never)]
    fn header(&mut self, executor: &Executor) -> Check {
        demand(
            layout::PROCESS_BYTES == process_group::PROCESS_BYTES
                && layout::PROCESS_PAGES == process_group::BOX_FRAMES
                && layout::METADATA_PAGES == 2 * layout::IMAGE_PAGES + layout::AUTHORITY_PAGES
                && layout::LOOP_START < layout::LOOP_END,
        )?;
        writeln!(self.output, "HMTASK BEGIN 1").map_err(|_| ())?;
        writeln!(self.output,"HMTASK INFO wire=1 cpus=1 frequency={} total_ticks={} quantum_ticks={} process_bytes={} process_pages={} image_bytes={} image_pages={} authority_bytes={} authority_pages={}", executor.frequency(),executor.ticks(),executor.frequency()/1000,layout::PROCESS_BYTES,layout::PROCESS_PAGES,layout::IMAGE_BYTES,layout::IMAGE_PAGES,layout::AUTHORITY_BYTES,layout::AUTHORITY_PAGES).map_err(|_| ())?;
        writeln!(
            self.output,
            "HMTASK CASE index=0 pass=0 name={} before={}",
            self.case.name(),
            self.baseline
        )
        .map_err(|_| ())
    }
    #[inline(never)]
    fn admitted(&mut self, group: &Group<'_, '_>) -> Check<TaskId> {
        let root = group.task_root().map_err(|_| ())?;
        let info = group.info(root).map_err(|_| ())?;
        demand(info.state() == TaskState::Ready && info.box_frames() == layout::PROCESS_PAGES)?;
        demand(
            group.charged_frames()
                == layout::METADATA_PAGES + info.process_frames() + info.box_frames(),
        )?;
        self.balance(group)?;
        let after = free(self.pool);
        writeln!(
            self.output,
            "HMTASK GROUP group={} limit=512 metadata={} before={} after={after}",
            group.namespace(),
            layout::METADATA_PAGES,
            self.baseline
        )
        .map_err(|_| ())?;
        let entry = entry(self.case)?;
        writeln!(self.output,"HMTASK ROOT group={} slot={} generation={} launch_generation={} console_generation={} fixture={} entry={entry} root={} process_frames={} box_frames={} before={} after={after}",group.namespace(),root.slot(),root.generation(),group.task_launch_generation().map_err(|_| ())?,info.console_generation().ok_or(())?,self.case.name(),info.root(),info.process_frames(),info.box_frames(),self.baseline).map_err(|_| ())?;
        Ok(root)
    }
    #[inline(never)]
    fn step(&mut self, group: &mut Group<'_, '_>) -> Check<bool> {
        let owned = group.turn();
        let turn = match &owned {
            Ok(Turn::Task(t)) => t,
            _ => return Err(()),
        };
        self.balance(group)?;
        self.inspect(group, turn)
    }
    #[inline(never)]
    fn inspect(&mut self, group: &Group<'_, '_>, turn: &TaskTurn) -> Check<bool> {
        match turn {
            TaskTurn::Step {
                report, management, ..
            } => {
                self.steps = self.steps.checked_add(1).ok_or(())?;
                demand(
                    self.steps <= 1024
                        && report.controls()
                        && report.boot_root() == self.root
                        && report.vbar() == self.vectors,
                )?;
                demand(
                    report.live_frames() == group.charged_frames()
                        && management.live_frames() == group.charged_frames(),
                )?;
                Ok(false)
            }
            TaskTurn::Administrative { management } => {
                demand(management.live_frames() == group.charged_frames())?;
                Ok(false)
            }
            TaskTurn::RootCompleted {
                completion,
                management,
            } => {
                demand(
                    management.children_remaining() == 0
                        && completion.live_frames() == layout::METADATA_PAGES,
                )?;
                demand(completion.closure().end() == expected(self.case))?;
                let root = group.task_root().map_err(|_| ())?;
                match completion.closure().evidence() {
                    RootEvidence::Executed(report) => {
                        demand(!administrative(self.case) && report.task().id() == root)?
                    }
                    RootEvidence::Administrative { trigger } => {
                        let child = trigger.ok_or(())?;
                        demand(
                            administrative(self.case)
                                && child.group() == root.group()
                                && child != root,
                        )?;
                    }
                }
                let held = group.task_completion(root);
                demand(matches!(&held, Ok(Some(value)) if value == completion))?;
                Ok(true)
            }
        }
    }
    #[inline(never)]
    fn reap(&mut self, group: &mut Group<'_, '_>, root: TaskId) -> Check {
        let observed = group.negative_observation().map_err(|_| ())?;
        let seed = match self.case {
            NegativeCase::CapabilityExhaustion => SeedKind::TaskCapabilityMax,
            NegativeCase::SlotExhaustion => SeedKind::ChildSlotsMax,
            _ => SeedKind::None,
        };
        demand(
            observed.case() == self.case
                && observed.succeeded()
                && observed.state() == HookStatus::Fired
                && observed.fired() == 1
                && observed.seed() == seed,
        )?;
        let held = group.task_completion(root).map_err(|_| ())?.ok_or(())?;
        demand(
            held.closure().end() == expected(self.case)
                && group.charged_frames() == layout::METADATA_PAGES,
        )?;
        let before = free(self.pool);
        let reaped = group.task_reap(root).map_err(|_| ())?;
        demand(reaped == held && group.charged_frames() == 0 && free(self.pool) == self.baseline)?;
        demand(matches!(group.negative_observation(), Err(Error::Mode)))?;
        writeln!(
            self.output,
            "HMTASK REAP_ROOT group={} outcome={} before={before} after={}",
            group.namespace(),
            word(reaped.closure().end()),
            free(self.pool)
        )
        .map_err(|_| ())
    }
}
#[inline(never)]
fn execute(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let namespace = {
        let mut owned = Group::new_negative_task(executor, factory.reborrow());
        let group = owned.as_mut().map_err(|_| ())?;
        let root = checks.admitted(group)?;
        let mut completed = false;
        // Steps have their own exact 1024 cap; the total also bounds management.
        for _ in 0..1028 {
            if checks.step(group)? {
                completed = true;
                break;
            }
        }
        demand(completed)?;
        checks.reap(group, root)?;
        group.namespace()
    };
    demand(free(checks.pool) == checks.baseline)?;
    writeln!(
        checks.output,
        "HMTASK DROP group={namespace} before={} after={}",
        checks.baseline,
        free(checks.pool)
    )
    .map_err(|_| ())
}
#[inline(never)]
fn retry(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let image = boot_image::admit(Group::negative_archive(), path(checks.case)).map_err(|_| ())?;
    {
        let result = Group::new_task(
            executor,
            factory.reborrow(),
            &image,
            Group::negative_archive(),
            true,
        );
        demand(matches!(&result, Err(Error::Resources)))?;
    }
    demand(free(checks.pool) == checks.baseline && !executor.retired())?;
    writeln!(
        checks.output,
        "HMTASK RETRY_SETUP result=resources before={} after={} published=0",
        checks.baseline,
        free(checks.pool)
    )
    .map_err(|_| ())
}
#[inline(never)]
fn legacy(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let image = boot_image::admit(Group::negative_archive(), "bin/legacy_hello").map_err(|_| ())?;
    let space = factory.load(&image, 284).map_err(|_| ())?;
    legacy_prepare(checks, executor, space)
}
#[inline(never)]
fn legacy_prepare(checks: &mut Checks<'_>, executor: &mut Executor, space: Space<'_>) -> Check {
    let prepared = executor.prepare(space, true, 1);
    let process = match prepared {
        Ok(p) => p,
        Err(_) => return Err(()),
    };
    legacy_run(checks, executor, process)
}
#[inline(never)]
fn legacy_run(checks: &mut Checks<'_>, executor: &mut Executor, process: Process<'_>) -> Check {
    let frames = process.frame_count();
    demand(checks.baseline.checked_sub(free(checks.pool)) == Some(frames))?;
    writeln!(
        checks.output,
        "HMTASK LEGACY_BEGIN entry={} root={} before={} during={} frames={frames}",
        process.entry(),
        process.root_address(),
        checks.baseline,
        free(checks.pool)
    )
    .map_err(|_| ())?;
    let report = executor.run(process).map_err(|_| ())?;
    demand(
        report.reason == Reason::Exit
            && report.code == 0
            && report.calls == 2
            && report.written == 15
            && report.controls
            && report.root == checks.root
            && report.vbar == checks.vectors
            && free(checks.pool) == checks.baseline,
    )?;
    writeln!(checks.output,"HMTASK LEGACY_END reason=exit code=0 calls=2 written=15 root={} vbar={} controls=1 before={} after={}",report.root,report.vbar,checks.baseline,free(checks.pool)).map_err(|_| ())
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
    let case = executor.negative_case().ok_or(())?;
    let mut checks = Checks {
        pool,
        output,
        baseline: free(pool),
        root,
        vectors,
        case,
        steps: 0,
    };
    checks.header(executor)?;
    execute(&mut checks, factory, executor)?;
    if case == NegativeCase::CapabilityExhaustion {
        retry(&mut checks, factory, executor)?;
        legacy(&mut checks, factory, executor)?;
    }
    demand(free(pool) == checks.baseline)?;
    writeln!(
        checks.output,
        "HMTASK PASS index=0 pass=0 name={} before={} after={}",
        case.name(),
        checks.baseline,
        checks.baseline
    )
    .map_err(|_| ())?;
    writeln!(checks.output, "HMTASK END 1").map_err(|_| ())
}
#[inline(never)]
pub(super) fn run(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> bool {
    checks(pool, factory, executor, root, vectors, output).is_ok()
}
