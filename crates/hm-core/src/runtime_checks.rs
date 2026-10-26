//! Actual resumable EL0 ownership and isolation cases; design 0028.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::process::Executor;
use super::process_group::{
    self, Completion, ContextProbe, End, Error, Event, Group, TaskId, Turn,
};
use super::runtime_inputs::ARCHIVE;
use super::user_space::Factory;
use core::fmt::Write;
use hm_kernel::boot_image;
use hm_kernel::elf::Image;
use hm_kernel::startup_input::Input;
type Check<T = ()> = Result<T, ()>;
const NAMES: [&str; 12] = [
    "svc_context",
    "irq_context",
    "busy_peer",
    "byte_budget",
    "call_limit",
    "fault_peer",
    "cancel_new",
    "cancel_reply",
    "cancel_quantum",
    "completion_tokens",
    "admission_rollback",
    "drop_live",
];
fn demand(ok: bool) -> Check {
    if ok { Ok(()) } else { Err(()) }
}
fn path(name: &str) -> Check<&'static str> {
    Ok(match name {
        "context_svc_a" => "bin/context_svc_a",
        "context_svc_b" => "bin/context_svc_b",
        "context_irq_a" => "bin/context_irq_a",
        "context_irq_b" => "bin/context_irq_b",
        "loop" => "bin/loop",
        "hello" => "bin/hello",
        "byte_budget" => "bin/byte_budget",
        "call_limit" => "bin/call_limit",
        "fault" => "bin/fault",
        "yield" => "bin/yield",
        "grant" => "bin/grant",
        _ => return Err(()),
    })
}
#[inline(never)]
fn image(name: &str) -> Check<Image<'static>> {
    boot_image::admit(ARCHIVE, path(name)?).map_err(|_| ())
}
struct Checks<'a> {
    pool: &'a Pool,
    output: &'a mut dyn Write,
    baseline: usize,
    root: u64,
    vectors: u64,
    case_steps: usize,
    pass_steps: usize,
}
impl Checks<'_> {
    fn group(&mut self, group: &Group<'_, '_>, limit: usize) -> Check {
        writeln!(
            self.output,
            "HMRT GROUP group={} limit={limit} before={}",
            group.namespace(),
            self.pool.stats().free
        )
        .map_err(|_| ())
    }
    #[inline(never)]
    fn admitted(
        &mut self,
        group: &Group<'_, '_>,
        id: TaskId,
        name: &str,
        entry: u64,
        before: usize,
    ) -> Check {
        let info = group.info(id).map_err(|_| ())?;
        let after = self.pool.stats().free;
        demand(before.checked_sub(info.process_frames() + info.box_frames()) == Some(after))?;
        writeln!(self.output,"HMRT ADMIT group={} slot={} generation={} console_generation={} fixture={name} entry={entry} root={} process_frames={} box_frames={} before={before} after={after}",id.group(),id.slot(),id.generation(),info.console_generation().unwrap_or(0),info.root(),info.process_frames(),info.box_frames()).map_err(|_|())
    }
    #[inline(never)]
    fn admit(
        &mut self,
        group: &mut Group<'_, '_>,
        name: &str,
        console: bool,
        input: Option<&Input>,
    ) -> Check<TaskId> {
        let owned_image = image(name);
        let image = owned_image.as_ref().map_err(|_| ())?;
        let before = self.pool.stats().free;
        let id = group.admit(image, console, input).map_err(|_| ())?;
        self.admitted(group, id, name, image.entry(), before)?;
        Ok(id)
    }
    #[inline(never)]
    fn probe(&mut self, group: &mut Group<'_, '_>, probe: ContextProbe) -> Check<TaskId> {
        let before = self.pool.stats().free;
        let id = group.admit_context_probe(probe).map_err(|_| ())?;
        self.admitted(
            group,
            id,
            match probe {
                ContextProbe::A => "context_irq_a",
                ContextProbe::B => "context_irq_b",
            },
            0x1_0000_0000,
            before,
        )?;
        Ok(id)
    }
    fn step(&mut self, group: &mut Group<'_, '_>) -> Check<Turn> {
        let result = group.turn().map_err(|_| ())?;
        let report = match result {
            Turn::Idle => return Ok(result),
            Turn::Progress(report) => report,
            Turn::Completed(done) => done.report(),
        };
        self.case_steps = self.case_steps.checked_add(1).ok_or(())?;
        self.pass_steps = self.pass_steps.checked_add(1).ok_or(())?;
        demand(self.case_steps <= 1024 && self.pass_steps <= 4096)?;
        demand(
            report.controls() && report.boot_root() == self.root && report.vbar() == self.vectors,
        )?;
        demand(report.live_frames() == group.charged_frames())?;
        demand(self.pool.stats().free.checked_add(group.charged_frames()) == Some(self.baseline))?;
        if let Some(timing) = report.timing() {
            demand(timing.accounted())?;
        }
        if let Turn::Completed(done) = result {
            demand(!matches!(done.end(), End::Unavailable | End::TurnLimit))?;
        }
        demand(report.event() != Event::Spurious)?;
        Ok(result)
    }
    fn action(
        &mut self,
        group: &Group<'_, '_>,
        id: Option<TaskId>,
        op: &str,
        result: &str,
        before: usize,
    ) -> Check {
        let (namespace, slot, generation) = id
            .map(|id| (id.group(), id.slot(), id.generation()))
            .unwrap_or((0, 0, 0));
        let after = self.pool.stats().free;
        demand(before == after)?;
        writeln!(self.output,"HMRT ACTION group={} id_group={namespace} op={op} slot={slot} generation={generation} result={result} before={before} after={after}",group.namespace()).map_err(|_|())
    }
    fn cancel(&mut self, group: &mut Group<'_, '_>, id: TaskId) -> Check {
        let before = self.pool.stats().free;
        group.request_cancel(id).map_err(|_| ())?;
        self.action(group, Some(id), "cancel", "ok", before)
    }
    fn reap(&mut self, group: &mut Group<'_, '_>, id: TaskId) -> Check<Completion> {
        let before = self.pool.stats().free;
        let done = group.reap(id).map_err(|_| ())?;
        self.action(group, Some(id), "reap", "ok", before)?;
        Ok(done)
    }
    fn word(&mut self, group: &Group<'_, '_>, id: TaskId) -> Check<u64> {
        let value = group.test_release_word(id).map_err(|_| ())?;
        writeln!(
            self.output,
            "HMRT WORD group={} id_group={} slot={} generation={} value={value}",
            group.namespace(),
            id.group(),
            id.slot(),
            id.generation()
        )
        .map_err(|_| ())?;
        Ok(value)
    }
    fn refused_release(
        &mut self,
        group: &mut Group<'_, '_>,
        id: TaskId,
        observer: TaskId,
        stale: bool,
    ) -> Check {
        let word = self.word(group, observer)?;
        let before = self.pool.stats().free;
        demand(
            group.release_context_probe(id)
                == Err(if stale { Error::Stale } else { Error::Bounds }),
        )?;
        self.action(
            group,
            Some(id),
            "release",
            if stale { "stale" } else { "release_denied" },
            before,
        )?;
        demand(self.word(group, observer)? == word)
    }
    fn release(&mut self, group: &mut Group<'_, '_>, id: TaskId) -> Check {
        demand(self.word(group, id)? == 0)?;
        let before = self.pool.stats().free;
        group.release_context_probe(id).map_err(|_| ())?;
        self.action(group, Some(id), "release", "ok", before)?;
        demand(self.word(group, id)? == 1)?;
        self.refused_release(group, id, id, false)
    }
    fn idle(&mut self, group: &mut Group<'_, '_>) -> Check {
        let before = self.pool.stats().free;
        demand(matches!(group.turn(), Ok(Turn::Idle)))?;
        demand(before == self.pool.stats().free)?;
        writeln!(
            self.output,
            "HMRT IDLE group={} before={before} after={before}",
            group.namespace()
        )
        .map_err(|_| ())
    }
    fn dropped(&mut self, namespace: u64, before: usize) -> Check {
        let after = self.pool.stats().free;
        writeln!(
            self.output,
            "HMRT DROP group={namespace} before={before} after={after}"
        )
        .map_err(|_| ())?;
        demand(after == self.baseline)
    }
    fn drain(&mut self, group: &mut Group<'_, '_>, ids: &[TaskId]) -> Check {
        while ids
            .iter()
            .any(|id| group.completion(*id).ok().flatten().is_none())
        {
            demand(!matches!(self.step(group)?, Turn::Idle))?;
        }
        self.idle(group)
    }
    fn expect(
        &self,
        group: &Group<'_, '_>,
        id: TaskId,
        end: End,
        calls: Option<usize>,
        written: Option<usize>,
    ) -> Check {
        let done = group.completion(id).map_err(|_| ())?.ok_or(())?;
        demand(done.end() == end)?;
        if let Some(calls) = calls {
            demand(done.report().calls() == calls)?;
        }
        if let Some(written) = written {
            demand(done.report().written() == written)?;
        }
        Ok(())
    }
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
    let baseline = pool.stats().free;
    writeln!(output, "HMRT BEGIN 24").map_err(|_| ())?;
    writeln!(output,"HMRT INFO wire=1 cpus=1 frequency={} total_ticks={} quantum_ticks={} box_bytes={} box_frames={}",executor.frequency(),executor.ticks(),executor.frequency()/1000,process_group::PROCESS_BYTES,process_group::BOX_FRAMES).map_err(|_|())?;
    let mut checks = Checks {
        pool,
        output,
        baseline,
        root,
        vectors,
        case_steps: 0,
        pass_steps: 0,
    };
    for pass in 0..2 {
        checks.pass_steps = 0;
        for (number, name) in NAMES.iter().enumerate() {
            let index = pass * 12 + number;
            checks.case_steps = 0;
            demand(pool.stats().free == baseline)?;
            writeln!(
                checks.output,
                "HMRT CASE index={index} pass={pass} name={name} before={baseline}"
            )
            .map_err(|_| ())?;
            match number {
                0 => pair(
                    &mut checks,
                    factory,
                    executor,
                    "context_svc_a",
                    "context_svc_b",
                    End::Exited(0),
                    Some(6),
                    Some(832),
                )?,
                1 => irq_context(&mut checks, factory, executor)?,
                2 => pair(
                    &mut checks,
                    factory,
                    executor,
                    "loop",
                    "hello",
                    End::Budget,
                    None,
                    Some(0),
                )?,
                3 => byte_budget(&mut checks, factory, executor)?,
                4 => pair(
                    &mut checks,
                    factory,
                    executor,
                    "call_limit",
                    "hello",
                    End::CallLimit,
                    Some(64),
                    Some(0),
                )?,
                5 => pair(
                    &mut checks,
                    factory,
                    executor,
                    "fault",
                    "hello",
                    End::Fault,
                    Some(0),
                    Some(0),
                )?,
                6..=8 => cancel_case(&mut checks, factory, executor, number)?,
                9 => completion_tokens(&mut checks, factory, executor)?,
                10 => admission_rollback(&mut checks, factory, executor)?,
                11 => drop_live(&mut checks, factory, executor)?,
                _ => return Err(()),
            }
            demand(pool.stats().free == baseline)?;
            writeln!(
                checks.output,
                "HMRT PASS index={index} pass={pass} name={name} before={baseline} after={baseline}"
            )
            .map_err(|_| ())?;
        }
    }
    writeln!(checks.output, "HMRT END 24").map_err(|_| ())
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn pair(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    a: &str,
    b: &str,
    end: End,
    calls: Option<usize>,
    written: Option<usize>,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let first = checks.admit(group, a, true, None)?;
    let second = checks.admit(group, b, true, None)?;
    let mut quanta = 0;
    let mut peer_finished = false;
    while group.completion(first).map_err(|_| ())?.is_none()
        || group.completion(second).map_err(|_| ())?.is_none()
    {
        match checks.step(group)? {
            Turn::Progress(report)
                if report.task().id() == first && report.event() == Event::Timer =>
            {
                quanta += 1
            }
            Turn::Completed(done) if done.report().task().id() == second => {
                peer_finished = true;
                if end == End::Budget {
                    demand(group.completion(first).map_err(|_| ())?.is_none())?;
                }
            }
            Turn::Idle => return Err(()),
            _ => {}
        }
    }
    checks.idle(group)?;
    checks.expect(group, first, end, calls, written)?;
    checks.expect(
        group,
        second,
        End::Exited(0),
        Some(if b == "context_svc_b" { 6 } else { 2 }),
        Some(if b == "context_svc_b" { 832 } else { 15 }),
    )?;
    if end == End::Budget {
        demand(quanta >= 2 && peer_finished)?;
    }
    checks.reap(group, first)?;
    checks.reap(group, second)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
fn poll(report: process_group::StepReport) -> bool {
    report.event() == Event::Timer && (0x1_0000_0100..0x1_0000_0108).contains(&report.elr())
}
#[inline(never)]
fn irq_context(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.probe(group, ContextProbe::A)?;
    let b = checks.probe(group, ContextProbe::B)?;
    let ordinary = checks.admit(group, "context_irq_a", true, None)?;
    checks.refused_release(group, ordinary, ordinary, false)?;
    checks.cancel(group, ordinary)?;
    let mut polls = [0usize; 2];
    let mut released = [false; 2];
    let mut completed_refusal = false;
    while [a, b, ordinary]
        .iter()
        .any(|id| group.completion(*id).ok().flatten().is_none())
    {
        match checks.step(group)? {
            Turn::Progress(report) => {
                for (index, id) in [a, b].iter().enumerate() {
                    if report.task().id() == *id && poll(report) {
                        polls[index] += 1;
                        if polls[index] >= 2 && !released[index] {
                            checks.release(group, *id)?;
                            released[index] = true;
                        }
                    }
                }
            }
            Turn::Completed(done)
                if !completed_refusal
                    && (done.report().task().id() == a || done.report().task().id() == b) =>
            {
                let completed = done.report().task().id();
                let observer = if completed == a { b } else { a };
                demand(group.completion(observer).map_err(|_| ())?.is_none())?;
                checks.refused_release(group, completed, observer, false)?;
                completed_refusal = true;
            }
            Turn::Idle => return Err(()),
            _ => {}
        }
    }
    demand(released.iter().all(|b| *b) && completed_refusal)?;
    checks.idle(group)?;
    checks.expect(group, a, End::Exited(0), Some(5), Some(832))?;
    checks.expect(group, b, End::Exited(0), Some(5), Some(832))?;
    checks.expect(group, ordinary, End::Cancelled, Some(0), Some(0))?;
    for id in [a, b, ordinary] {
        checks.reap(group, id)?;
    }
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
#[inline(never)]
fn byte_budget(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.admit(group, "byte_budget", true, None)?;
    let token = (u64::from(
        group
            .info(a)
            .map_err(|_| ())?
            .console_generation()
            .ok_or(())?,
    ) << 32)
        | 1;
    let input = Input::from_bytes(&token.to_le_bytes()).map_err(|_| ())?;
    let b = checks.admit(group, "grant", true, Some(&input))?;
    checks.drain(group, &[a, b])?;
    checks.expect(group, a, End::Exited(0), Some(19), Some(4096))?;
    checks.expect(group, b, End::Exited(0), Some(3), Some(12))?;
    checks.reap(group, a)?;
    checks.reap(group, b)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
#[inline(never)]
fn cancel_case(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    case: usize,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.admit(
        group,
        if case == 7 { "yield" } else { "loop" },
        case != 6,
        None,
    )?;
    let b = checks.admit(group, "hello", true, None)?;
    if case != 6 {
        loop {
            match checks.step(group)? {
                Turn::Progress(report)
                    if report.task().id() == a
                        && report.event() == if case == 7 { Event::Svc } else { Event::Timer } =>
                {
                    break;
                }
                Turn::Completed(done) if done.report().task().id() == a => return Err(()),
                Turn::Idle => return Err(()),
                _ => {}
            }
        }
    }
    checks.cancel(group, a)?;
    checks.cancel(group, a)?;
    checks.drain(group, &[a, b])?;
    checks.expect(
        group,
        a,
        End::Cancelled,
        Some(if case == 7 { 1 } else { 0 }),
        Some(0),
    )?;
    checks.expect(group, b, End::Exited(0), Some(2), Some(15))?;
    checks.reap(group, a)?;
    checks.reap(group, b)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
fn reject_token(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, id: TaskId) -> Check {
    let before = checks.pool.stats().free;
    demand(group.request_cancel(id) == Err(Error::Stale))?;
    checks.action(group, Some(id), "cancel", "stale", before)?;
    demand(group.completion(id) == Err(Error::Stale))?;
    checks.action(group, Some(id), "query", "stale", before)?;
    demand(group.reap(id) == Err(Error::Stale))?;
    checks.action(group, Some(id), "reap", "stale", before)
}
#[inline(never)]
fn reject_admission(checks: &mut Checks<'_>, group: &mut Group<'_, '_>, expected: Error) -> Check {
    let owned_image = image("hello");
    let image = owned_image.as_ref().map_err(|_| ())?;
    let before = checks.pool.stats().free;
    demand(group.admit(image, true, None) == Err(expected))?;
    checks.action(
        group,
        None,
        "admit_reject",
        if expected == Error::Full {
            "full"
        } else {
            "resources"
        },
        before,
    )
}
#[inline(never)]
fn held_completions(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check<TaskId> {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.admit(group, "hello", true, None)?;
    let b = checks.admit(group, "hello", true, None)?;
    let c = checks.admit(group, "hello", true, None)?;
    let d = checks.admit(group, "hello", true, None)?;
    let retired_token = (u64::from(
        group
            .info(a)
            .map_err(|_| ())?
            .console_generation()
            .ok_or(())?,
    ) << 32)
        | 1;
    // Retain the four completions without emitting the group's final IDLE:
    // slot zero is reaped and reused below before that single observation.
    while [a, b, c, d]
        .iter()
        .any(|id| group.completion(*id).ok().flatten().is_none())
    {
        demand(!matches!(checks.step(group)?, Turn::Idle))?;
    }
    demand(checks.pool.stats().free == checks.baseline && group.charged_frames() == 0)?;
    reject_admission(checks, group, Error::Full)?;
    for id in [a, b, c, d] {
        checks.expect(group, id, End::Exited(0), Some(2), Some(15))?;
        for _ in 0..2 {
            let before = checks.pool.stats().free;
            demand(group.completion(id).map_err(|_| ())?.is_some())?;
            checks.action(group, Some(id), "query", "ok", before)?;
        }
    }
    checks.reap(group, a)?;
    let replacement = {
        let input = Input::from_bytes(&retired_token.to_le_bytes()).map_err(|_| ())?;
        checks.admit(group, "grant", true, Some(&input))?
    };
    demand(replacement.slot() == a.slot() && replacement.generation() == a.generation() + 1)?;
    reject_token(checks, group, a)?;
    checks.refused_release(group, a, replacement, true)?;
    let before = checks.pool.stats().free;
    demand(group.reap(replacement) == Err(Error::Pending))?;
    checks.action(group, Some(replacement), "reap", "pending", before)?;
    checks.drain(group, &[replacement])?;
    checks.expect(group, replacement, End::Exited(0), Some(3), Some(12))?;
    for id in [replacement, b, c, d] {
        checks.reap(group, id)?;
    }
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }?;
    Ok(replacement)
}
#[inline(never)]
fn completion_tokens(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check {
    let foreign = held_completions(checks, factory, executor)?;
    foreign_tokens(checks, factory, executor, foreign)
}
#[inline(never)]
fn foreign_tokens(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    foreign: TaskId,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.probe(group, ContextProbe::A)?;
    let b = checks.admit(group, "hello", true, None)?;
    reject_token(checks, group, foreign)?;
    checks.refused_release(group, foreign, a, true)?;
    checks.refused_release(group, b, b, false)?;
    let mut polls = 0;
    let mut released = false;
    let mut refused_completed = false;
    while group.completion(a).map_err(|_| ())?.is_none()
        || group.completion(b).map_err(|_| ())?.is_none()
    {
        match checks.step(group)? {
            Turn::Progress(report) if report.task().id() == a && poll(report) => {
                polls += 1;
                if polls >= 2 && refused_completed && !released {
                    checks.release(group, a)?;
                    released = true;
                }
            }
            Turn::Completed(done) if done.report().task().id() == b => {
                checks.refused_release(group, b, a, false)?;
                refused_completed = true;
            }
            Turn::Idle => return Err(()),
            _ => {}
        }
    }
    demand(released && refused_completed)?;
    checks.idle(group)?;
    checks.expect(group, a, End::Exited(0), Some(5), Some(832))?;
    checks.expect(group, b, End::Exited(0), Some(2), Some(15))?;
    checks.reap(group, a)?;
    checks.reap(group, b)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
#[inline(never)]
fn hello_loaded_frames() -> Check<usize> {
    image("hello")?.page_count().checked_add(28).ok_or(())
}
#[inline(never)]
fn admission_rollback(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check {
    let n = hello_loaded_frames()?;
    let generation = executor.last_prepared_generation().unwrap_or(0);
    rollback_cap(
        checks,
        factory,
        executor,
        n + 1 + process_group::BOX_FRAMES,
        generation,
    )?;
    for (phase, leave) in [0, n, n + 1].into_iter().enumerate() {
        rollback_pressure(checks, factory, executor, leave, phase, generation)?;
    }
    Ok(())
}
#[inline(never)]
fn rollback_cap(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    charge: usize,
    generation: u32,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), charge - 1);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, charge - 1)?;
    reject_admission(checks, group, Error::Resources)?;
    demand(group.last_prepared_generation() == Some(generation))?;
    let namespace = group.namespace();
    let before = checks.pool.stats().free;
    drop(owned_group);
    checks.dropped(namespace, before)
}
#[inline(never)]
fn rollback_pressure(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
    leave: usize,
    phase: usize,
    generation: u32,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let pressure = checks
        .pool
        .allocate_heap(
            checks
                .baseline
                .checked_sub(leave)
                .and_then(|pages| pages.checked_mul(4096))
                .ok_or(())?,
            4096,
        )
        .map_err(|_| ())?;
    demand(checks.pool.stats().free == leave)?;
    reject_admission(checks, group, Error::Resources)?;
    demand(group.charged_frames() == 0)?;
    demand(
        group.last_prepared_generation()
            == Some(if phase == 2 {
                generation.checked_add(2).ok_or(())?
            } else {
                generation
            }),
    )?;
    drop(pressure);
    demand(checks.pool.stats().free == checks.baseline)?;
    if phase == 2 {
        let id = checks.admit(group, "hello", true, None)?;
        demand(
            group.info(id).map_err(|_| ())?.console_generation()
                == Some(generation.checked_add(3).ok_or(())?),
        )?;
        checks.drain(group, &[id])?;
        checks.expect(group, id, End::Exited(0), Some(2), Some(15))?;
        checks.reap(group, id)?;
    }
    let namespace = group.namespace();
    let before = checks.pool.stats().free;
    drop(owned_group);
    checks.dropped(namespace, before)
}
#[inline(never)]
fn drop_suspended(
    checks: &mut Checks<'_>,
    factory: &Factory<'_>,
    executor: &mut Executor,
) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let a = checks.probe(group, ContextProbe::A)?;
    let b = checks.probe(group, ContextProbe::B)?;
    let mut seen = [false; 2];
    while !seen.iter().all(|v| *v) {
        match checks.step(group)? {
            Turn::Progress(report) => {
                if poll(report) {
                    if report.task().id() == a {
                        seen[0] = true;
                    } else if report.task().id() == b {
                        seen[1] = true;
                    }
                }
            }
            _ => return Err(()),
        }
    }
    demand(checks.word(group, a)? == 0 && checks.word(group, b)? == 0)?;
    demand(group.charged_frames() > 0)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
#[inline(never)]
fn drop_live(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    drop_suspended(checks, factory, executor)?;
    fresh_hello(checks, factory, executor)
}
#[inline(never)]
fn fresh_hello(checks: &mut Checks<'_>, factory: &Factory<'_>, executor: &mut Executor) -> Check {
    let mut owned_group = Group::new(executor, factory.reborrow(), 512);
    let group = owned_group.as_mut().map_err(|_| ())?;
    checks.group(group, 512)?;
    let id = checks.admit(group, "hello", true, None)?;
    checks.drain(group, &[id])?;
    checks.expect(group, id, End::Exited(0), Some(2), Some(15))?;
    checks.reap(group, id)?;
    {
        let namespace = group.namespace();
        let before = checks.pool.stats().free;
        drop(owned_group);
        checks.dropped(namespace, before)
    }
}
