//! Safe callbacks requiring real timer scheduling, never a synthetic model.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::thread_api::{
    DispatchCause, MAX_THREADS, PreemptReport, TRACE_CAPACITY, ThreadContext, ThreadError,
    ThreadJob,
};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const ROUNDS: usize = 8;
static BUSY: AtomicBool = AtomicBool::new(false);
static STAGES: [AtomicUsize; MAX_THREADS] = [const { AtomicUsize::new(0) }; MAX_THREADS];
static DROPS: AtomicUsize = AtomicUsize::new(0);
static REJECTED: AtomicUsize = AtomicUsize::new(0);

struct Fixture;
impl Fixture {
    fn acquire() -> Option<Self> {
        BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

struct Witness<'a>(&'a Cell<usize>);
impl Drop for Witness<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_add(1));
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

fn result(input: u64, id: usize) -> u64 {
    input.rotate_left(17) ^ (id as u64).wrapping_mul(0x9e37_79b9) ^ 0xabe3_0000
}

/// Every worker waits for peers without yielding. A cooperative-only runner
/// stops at the first barrier, so the host's bounded deadline rejects it.
#[inline(never)]
fn nonyielding(context: &mut dyn ThreadContext, input: u64) -> u64 {
    let id = context.id();
    if id >= MAX_THREADS {
        return u64::MAX;
    }
    let pattern = (input as u8).wrapping_add(id as u8);
    let mut local = [pattern; 256];
    let dropped = Cell::new(0usize);
    {
        let _witness = Witness(&dropped);
        for round in 1..=ROUNDS {
            local[round] = pattern.wrapping_add(round as u8);
            core::hint::black_box(&mut local);
            STAGES[id].store(round, Ordering::Release);
            while STAGES
                .iter()
                .any(|stage| stage.load(Ordering::Acquire) < round)
            {
                core::hint::spin_loop();
            }
            if dropped.get() != 0 || context.id() != id {
                return u64::MAX;
            }
            for (index, byte) in local.iter().enumerate() {
                let expected = if (1..=round).contains(&index) {
                    pattern.wrapping_add(index as u8)
                } else {
                    pattern
                };
                if *byte != expected {
                    return u64::MAX;
                }
            }
        }
    }
    if dropped.get() != 1 {
        return u64::MAX;
    }
    result(input, id)
}

#[inline(never)]
fn yielding(context: &mut dyn ThreadContext, input: u64) -> u64 {
    let id = context.id();
    let dropped = Cell::new(0usize);
    let pattern = (input as u8).wrapping_add(id as u8);
    let mut local = [pattern; 256];
    {
        let _witness = Witness(&dropped);
        for round in 0..input {
            let offset = (round as usize) % local.len();
            local[offset] = local[offset].wrapping_add(1);
            core::hint::black_box(&mut local);
            context.yield_now();
            if context.id() != id || dropped.get() != 0 {
                return u64::MAX;
            }
        }
    }
    for (index, byte) in local.iter().enumerate() {
        let updates = input / 256 + u64::from((index as u64) < input % 256);
        if *byte != pattern.wrapping_add(updates as u8) {
            return u64::MAX;
        }
    }
    if dropped.get() != 1 {
        return u64::MAX;
    }
    result(input, id)
}

fn forbidden(_: &mut dyn ThreadContext, input: u64) -> u64 {
    REJECTED.fetch_add(1, Ordering::Relaxed);
    input
}

fn mixed_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if nonyielding(context, input) != result(input, context.id()) {
        return u64::MAX;
    }
    yielding(context, input)
}

fn unsupported(platform: &impl Platform) -> bool {
    cfg!(target_arch = "x86_64") && platform.run_preemptive(&[]) == Err(ThreadError::Unsupported)
}

fn protections(platform: &impl Platform) -> bool {
    let checks = platform.checks();
    checks.write_protect
        && checks.nx
        && checks.wx
        && checks.stack_guard
        && checks.null_guard
        && checks.user_execute_protect
        && checks.user_access_protect
}

/// Cross-check all recorded causes and their counters. Truncated traces are
/// prefixes, so their per-worker counts may be lower than the complete totals.
fn report_valid(report: &PreemptReport, jobs: usize) -> bool {
    let run = &report.run;
    let Some(suspensions) = run
        .yields
        .iter()
        .chain(&report.preemptions)
        .try_fold(0u64, |total, count| total.checked_add(*count))
    else {
        return false;
    };
    if jobs > MAX_THREADS
        || run.completed != jobs
        || run.dispatches != suspensions.saturating_add(jobs as u64)
        || run.trace_len != (run.dispatches.min(TRACE_CAPACITY as u64) as usize)
        || run.truncated != (run.dispatches > TRACE_CAPACITY as u64)
        || run.results[jobs..].iter().any(|v| *v != 0)
        || run.yields[jobs..].iter().any(|v| *v != 0)
        || report.preemptions[jobs..].iter().any(|v| *v != 0)
    {
        return false;
    }
    let mut initial = [0usize; MAX_THREADS];
    let mut yields = [0u64; MAX_THREADS];
    let mut timers = [0u64; MAX_THREADS];
    for index in 0..run.trace_len {
        let id = run.trace[index];
        if id >= jobs {
            return false;
        }
        match report.causes[index] {
            DispatchCause::Initial => initial[id] += 1,
            DispatchCause::Yield => yields[id] += 1,
            DispatchCause::Timer => timers[id] += 1,
        }
        if initial[id] != 1 {
            return false;
        }
    }
    for id in 0..jobs {
        if initial[id] > 1 || yields[id] > run.yields[id] || timers[id] > report.preemptions[id] {
            return false;
        }
        if !run.truncated
            && (initial[id] != 1
                || yields[id] != run.yields[id]
                || timers[id] != report.preemptions[id])
        {
            return false;
        }
    }
    run.trace[run.trace_len..].iter().all(|id| *id == 0)
        && report.causes[run.trace_len..]
            .iter()
            .all(|cause| *cause == DispatchCause::Initial)
}

#[inline(never)]
pub(super) fn progress(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return unsupported(platform);
    }
    let Some(_fixture) = Fixture::acquire() else {
        return false;
    };
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    let jobs = core::array::from_fn::<_, MAX_THREADS, _>(|id| ThreadJob {
        entry: nonyielding,
        input: 0x1234 + id as u64,
    });
    for _ in 0..2 {
        for stage in &STAGES {
            stage.store(0, Ordering::Relaxed);
        }
        DROPS.store(0, Ordering::Relaxed);
        let Ok(report) = platform.run_preemptive(&jobs) else {
            return false;
        };
        if !report_valid(&report, MAX_THREADS)
            || report.run.results != core::array::from_fn(|id| result(jobs[id].input, id))
            || report.run.yields != [0; MAX_THREADS]
            || report.preemptions.contains(&0)
            || STAGES
                .iter()
                .any(|stage| stage.load(Ordering::Relaxed) != ROUNDS)
            || DROPS.load(Ordering::Relaxed) != MAX_THREADS
        {
            return false;
        }
    }
    platform.frame_stats() == frames
        && platform.mapping_stats() == mappings
        && protections(platform)
}

#[inline(never)]
pub(super) fn mixed(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return unsupported(platform);
    }
    let Some(_fixture) = Fixture::acquire() else {
        return false;
    };
    DROPS.store(0, Ordering::Relaxed);
    for stage in &STAGES {
        stage.store(0, Ordering::Relaxed);
    }
    // After timer-only barriers, callbacks perform different explicit yields.
    let inputs = [0, 2, 3, 4];
    let jobs = inputs.map(|input| ThreadJob {
        entry: mixed_worker,
        input,
    });
    let Ok(report) = platform.run_preemptive(&jobs) else {
        return false;
    };
    report_valid(&report, MAX_THREADS)
        && report.run.results == core::array::from_fn(|id| result(inputs[id], id))
        && report.run.yields == inputs
        && !report.preemptions.contains(&0)
        && DROPS.load(Ordering::Relaxed) == MAX_THREADS * 2
        && protections(platform)
}

#[inline(never)]
pub(super) fn limits(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return unsupported(platform);
    }
    let Some(_fixture) = Fixture::acquire() else {
        return false;
    };
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    if platform.run_preemptive(&[]) != Ok(PreemptReport::empty()) {
        return false;
    }
    REJECTED.store(0, Ordering::Relaxed);
    let oversized = [ThreadJob {
        entry: forbidden,
        input: 7,
    }; MAX_THREADS + 1];
    if platform.run_preemptive(&oversized) != Err(ThreadError::TooMany)
        || REJECTED.load(Ordering::Relaxed) != 0
    {
        return false;
    }
    DROPS.store(0, Ordering::Relaxed);
    let Ok(report) = platform.run_preemptive(&[ThreadJob {
        entry: yielding,
        input: 64,
    }]) else {
        return false;
    };
    if !report_valid(&report, 1)
        || report.run.results != [result(64, 0), 0, 0, 0]
        || report.run.yields != [64, 0, 0, 0]
        || !report.run.truncated
        || DROPS.load(Ordering::Relaxed) != 1
    {
        return false;
    }
    // Reuse after a full trace and after rejection, without stale accounting.
    let Ok(fresh) = platform.run_preemptive(&[ThreadJob {
        entry: yielding,
        input: 0,
    }]) else {
        return false;
    };
    report_valid(&fresh, 1)
        && fresh.run.results == [result(0, 0), 0, 0, 0]
        && fresh.run.yields == [0; MAX_THREADS]
        && DROPS.load(Ordering::Relaxed) == 2
        && platform.run_preemptive(&[]) == Ok(PreemptReport::empty())
        && platform.frame_stats() == frames
        && platform.mapping_stats() == mappings
        && protections(platform)
}
