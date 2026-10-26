//! Safe concurrent clients of the reviewed two-CPU runner (design 0019).
#![forbid(unsafe_code)]

use super::mutex::Mutex;
use super::secondary::{Error, Runner, SmpReport};
use core::cell::Cell;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use hm_kernel::thread_api::{ThreadContext, ThreadEntry, ThreadJob};
use hm_kernel::wait_api::Event;

const ITERATIONS: u64 = 2000;
const SALT: u64 = 0xca35_97b2_f06e_148d;
static TEST_BUSY: AtomicBool = AtomicBool::new(false);
static STACKS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static START: AtomicUsize = AtomicUsize::new(0);
static SECOND: AtomicUsize = AtomicUsize::new(0);
static PEERS: AtomicUsize = AtomicUsize::new(0);
static OWNER: AtomicBool = AtomicBool::new(false);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static EVENT: Event = Event::new();
static UNRELATED: Event = Event::new();
static OBSERVED: AtomicU64 = AtomicU64::new(0);
static DATA: AtomicU64 = AtomicU64::new(0);
static PHASE: AtomicUsize = AtomicUsize::new(0);
static WOKE: AtomicBool = AtomicBool::new(false);
static SHARED: Mutex<Shared> = Mutex::new(Shared {
    count: Cell::new(0),
    checksum: Cell::new(SALT),
});
struct Shared {
    count: Cell<u64>,
    checksum: Cell<u64>,
}
struct TestLease;
impl Drop for TestLease {
    fn drop(&mut self) {
        TEST_BUSY.store(false, Ordering::Release);
    }
}
struct Witness;
impl Drop for Witness {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Release);
    }
}

fn record(context: &dyn ThreadContext, input: u64) -> bool {
    let Ok(slot) = usize::try_from(input) else {
        return false;
    };
    let Some(storage) = STACKS.get(slot) else {
        return false;
    };
    let Some((cpu, sp)) = super::preempt::worker_observation() else {
        return false;
    };
    if cpu != (slot / 4) as u64 || context.cpu_id() != slot / 4 || context.id() != slot % 4 {
        return false;
    }
    storage.store(sp, Ordering::Release);
    true
}

fn jobs(entry: ThreadEntry, cpu: usize) -> [ThreadJob; 4] {
    core::array::from_fn(|id| ThreadJob {
        entry,
        input: (cpu * 4 + id) as u64,
    })
}

fn execute(
    runner: &mut Runner,
    entry0: ThreadEntry,
    n0: usize,
    entry1: ThreadEntry,
    n1: usize,
) -> Option<SmpReport> {
    for stack in &STACKS {
        stack.store(0, Ordering::Relaxed);
    }
    let left = jobs(entry0, 0);
    let right = jobs(entry1, 1);
    let report = runner.run_smp(left.get(..n0)?, right.get(..n1)?).ok()?;
    if report.cpus != [0, 1] || !report.off || report.peer_checks != 255 {
        return None;
    }
    for (cpu, count) in [n0, n1].into_iter().enumerate() {
        let local = &report.reports[cpu];
        let guards = super::threads::local_guards(cpu)?;
        let tops = super::threads::local_tops(cpu)?;
        if local.run.completed != count || !local.run.results.get(..count)?.iter().all(|v| *v == 1)
        {
            return None;
        }
        let irqs = local
            .preemptions
            .iter()
            .try_fold(0u64, |sum, n| sum.checked_add(*n))?;
        if report.irq_counts[cpu] != irqs {
            return None;
        }
        for id in 0..count {
            let slot = cpu * 4 + id;
            let sp = STACKS[slot].load(Ordering::Acquire);
            let start = guards[id].checked_add(4096)? as u64;
            if sp < start || sp >= tops[id] as u64 || !sp.is_multiple_of(16) {
                return None;
            }
            for prior in &STACKS[..slot] {
                if prior.load(Ordering::Acquire) == sp {
                    return None;
                }
            }
        }
    }
    Some(report)
}

fn progress_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) || context.id() > 1 {
        return 0;
    }
    let bit = 1usize << (context.cpu_id() * 2 + context.id());
    START.fetch_or(bit, Ordering::Release);
    while START.load(Ordering::Acquire) != 15 {
        core::hint::spin_loop();
    }
    SECOND.fetch_or(bit, Ordering::Release);
    while SECOND.load(Ordering::Acquire) != 15 {
        core::hint::spin_loop();
    }
    super::preempt::worker_register_probe(input)
}
fn progress_contexts(runner: &mut Runner) -> Option<SmpReport> {
    START.store(0, Ordering::Relaxed);
    SECOND.store(0, Ordering::Relaxed);
    let report = execute(runner, progress_worker, 2, progress_worker, 2)?;
    for local in &report.reports {
        if local.preemptions[..2].contains(&0) || local.run.yields != [0; 4] {
            return None;
        }
    }
    Some(report)
}

fn mutex_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    let _witness = Witness;
    if !record(context, input) {
        return 0;
    }
    let mut start = 0;
    if input == 0 {
        let Some(guard) = SHARED.try_lock() else {
            return 0;
        };
        if guard.count.get() != 0 || guard.checksum.get() != SALT {
            return 0;
        }
        guard.count.set(1);
        OWNER.store(true, Ordering::Release);
        while PEERS.load(Ordering::Acquire) != 0xfe {
            core::hint::spin_loop();
        }
        if guard.count.get() != 1 || guard.checksum.get() != SALT {
            return 0;
        }
        guard.checksum.set(1 ^ SALT);
        drop(guard);
        start = 1;
    } else {
        while !OWNER.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        if SHARED.try_lock().is_some() {
            return 0;
        }
        PEERS.fetch_or(1usize << input, Ordering::Release);
    }
    for iteration in start..ITERATIONS {
        let guard = SHARED.lock_cooperative(context);
        let count = guard.count.get();
        if guard.checksum.get() != count ^ SALT {
            return 0;
        }
        let Some(next) = count.checked_add(1) else {
            return 0;
        };
        guard.count.set(next);
        guard.checksum.set(next ^ SALT);
        if iteration % 13 == 0 {
            context.yield_now();
        }
        if guard.count.get() != next || guard.checksum.get() != next ^ SALT {
            return 0;
        }
    }
    1
}
fn mutex(runner: &mut Runner) -> Option<SmpReport> {
    {
        let guard = SHARED.try_lock()?;
        guard.count.set(0);
        guard.checksum.set(SALT);
    }
    OWNER.store(false, Ordering::Relaxed);
    PEERS.store(0, Ordering::Relaxed);
    DROPS.store(0, Ordering::Relaxed);
    let report = execute(runner, mutex_worker, 4, mutex_worker, 4)?;
    let guard = SHARED.try_lock()?;
    if guard.count.get() != 8 * ITERATIONS
        || guard.checksum.get() != (8 * ITERATIONS) ^ SALT
        || DROPS.load(Ordering::Acquire) != 8
        || report.reports[0].preemptions[0] == 0
    {
        return None;
    }
    Some(report)
}

fn yielding_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    if super::preempt::worker_register_probe(input) != 1 {
        return 0;
    }
    for _ in 0..100 {
        context.yield_now();
    }
    1
}
fn mixed_truncation(runner: &mut Runner) -> Option<SmpReport> {
    let report = execute(runner, yielding_worker, 4, yielding_worker, 4)?;
    for local in &report.reports {
        if !local.run.truncated
            || local.run.trace_len != hm_kernel::thread_api::TRACE_CAPACITY
            || local.run.yields != [100; 4]
            || local.preemptions.contains(&0)
        {
            return None;
        }
    }
    Some(report)
}

fn prepare_event() -> u64 {
    let observed = EVENT.snapshot();
    OBSERVED.store(observed, Ordering::Release);
    DATA.store(0, Ordering::Relaxed);
    PHASE.store(0, Ordering::Relaxed);
    WOKE.store(false, Ordering::Relaxed);
    observed
}
fn notified_once(observed: u64) -> bool {
    observed.checked_add(1) == Some(EVENT.snapshot())
}
fn event_waiter(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    if context
        .wait(&EVENT, OBSERVED.load(Ordering::Acquire))
        .is_err()
    {
        return 0;
    }
    WOKE.store(true, Ordering::Release);
    u64::from(DATA.load(Ordering::Relaxed) == 1)
}
fn before_waiter(context: &mut dyn ThreadContext, input: u64) -> u64 {
    while PHASE.load(Ordering::Acquire) != 1 {
        core::hint::spin_loop();
    }
    event_waiter(context, input)
}
fn before_notifier(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    DATA.store(1, Ordering::Relaxed);
    if EVENT.notify().is_err() {
        return 0;
    }
    PHASE.store(1, Ordering::Release);
    1
}
fn event_before(runner: &mut Runner) -> Option<SmpReport> {
    let observed = prepare_event();
    let report = execute(runner, before_waiter, 1, before_notifier, 1)?;
    if !notified_once(observed) || report.reports[0].parks != [0; 4] {
        return None;
    }
    Some(report)
}
fn during_notifier(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    while !super::preempt::wait_race_ready() {
        core::hint::spin_loop();
    }
    DATA.store(1, Ordering::Relaxed);
    if EVENT.notify().is_err() {
        return 0;
    }
    u64::from(super::preempt::finish_wait_race())
}
fn event_during(runner: &mut Runner) -> Option<SmpReport> {
    let observed = prepare_event();
    if !super::preempt::arm_wait_race() {
        return None;
    }
    let report = execute(runner, event_waiter, 1, during_notifier, 1)?;
    if !notified_once(observed) || report.reports[0].parks != [1, 0, 0, 0] {
        return None;
    }
    Some(report)
}
fn broadcast_notifier(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    while super::preempt::blocked_masks()[1] != 15 {
        core::hint::spin_loop();
    }
    DATA.store(1, Ordering::Relaxed);
    u64::from(EVENT.notify().is_ok())
}
fn event_after_broadcast(runner: &mut Runner) -> Option<SmpReport> {
    let observed = prepare_event();
    let report = execute(runner, broadcast_notifier, 1, event_waiter, 4)?;
    if !notified_once(observed) || report.reports[1].parks != [1; 4] {
        return None;
    }
    Some(report)
}
fn unrelated_peer(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if context.id() == 0 {
        return event_waiter(context, input);
    }
    if !record(context, input) {
        return 0;
    }
    while PHASE.load(Ordering::Acquire) != 1 {
        core::hint::spin_loop();
    }
    for _ in 0..3 {
        context.yield_now();
        if WOKE.load(Ordering::Acquire) {
            return 0;
        }
    }
    PHASE.store(2, Ordering::Release);
    1
}
fn unrelated_notifier(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !record(context, input) {
        return 0;
    }
    while super::preempt::blocked_masks()[1] & 1 == 0 {
        core::hint::spin_loop();
    }
    if UNRELATED.notify().is_err() {
        return 0;
    }
    PHASE.store(1, Ordering::Release);
    while PHASE.load(Ordering::Acquire) != 2 {
        core::hint::spin_loop();
    }
    DATA.store(1, Ordering::Relaxed);
    u64::from(EVENT.notify().is_ok())
}
fn event_unrelated(runner: &mut Runner) -> Option<SmpReport> {
    let observed = prepare_event();
    let other = UNRELATED.snapshot();
    let report = execute(runner, unrelated_notifier, 1, unrelated_peer, 2)?;
    if !notified_once(observed)
        || other.checked_add(1) != Some(UNRELATED.snapshot())
        || report.reports[1].parks != [1, 0, 0, 0]
    {
        return None;
    }
    Some(report)
}
fn unused(_: &mut dyn ThreadContext, _: u64) -> u64 {
    1
}
fn limits_reuse(runner: &mut Runner) -> Option<SmpReport> {
    let excess = [ThreadJob {
        entry: unused,
        input: 0,
    }; 5];
    if !matches!(runner.run_smp(&excess, &[]), Err(Error::TooMany))
        || !matches!(runner.run_smp(&[], &excess), Err(Error::TooMany))
    {
        return None;
    }
    execute(runner, unused, 0, unused, 0)
}

type Case = fn(&mut Runner) -> Option<SmpReport>;
pub(super) fn run(runner: &mut Runner, output: &mut dyn Write) -> bool {
    if TEST_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    let _lease = TestLease;
    let cases: [(&str, Case); 8] = [
        ("progress_contexts", progress_contexts),
        ("mutex", mutex),
        ("mixed_truncation", mixed_truncation),
        ("event_before", event_before),
        ("event_during", event_during),
        ("event_after_broadcast", event_after_broadcast),
        ("event_unrelated", event_unrelated),
        ("limits_reuse", limits_reuse),
    ];
    if output.write_str("HMSCHED BEGIN 8\n").is_err() {
        return false;
    }
    for (name, test) in cases {
        let Some(report) = test(runner) else {
            let _ = writeln!(output, "HMSCHED FAIL {name}");
            return false;
        };
        if writeln!(output, "HMSCHED SESSION generation={} cpu0={} cpu1={} completed0={} completed1={} irq0={} irq1={} off=1 checks={} root={} sp={} worker0={} worker1={}",
            report.generation, report.cpus[0], report.cpus[1], report.reports[0].run.completed,
            report.reports[1].run.completed, report.irq_counts[0], report.irq_counts[1],
            report.peer_checks, report.peer_root, report.peer_sp,
            STACKS[0].load(Ordering::Acquire), STACKS[4].load(Ordering::Acquire)).is_err()
            || writeln!(output, "HMSCHED PASS {name}").is_err() { return false; }
    }
    output.write_str("HMSCHED END 8\n").is_ok()
}

#[cfg(feature = "smp-no-peer-timer-test")]
pub(super) fn no_peer_timer(runner: &mut Runner) -> ! {
    fn peer(context: &mut dyn ThreadContext, input: u64) -> u64 {
        if !record(context, input) {
            return 0;
        }
        if context.id() == 0 {
            PHASE.store(1, Ordering::Release);
            while PHASE.load(Ordering::Acquire) != 2 {
                core::hint::spin_loop();
            }
        } else {
            PHASE.store(2, Ordering::Release);
        }
        1
    }
    fn coordinator(context: &mut dyn ThreadContext, input: u64) -> u64 {
        if !record(context, input) {
            return 0;
        }
        if context.id() == 0 {
            while PHASE.load(Ordering::Acquire) == 0 {
                core::hint::spin_loop();
            }
            super::arch::smp_peer_started_marker(STACKS[4].load(Ordering::Acquire));
        }
        while PHASE.load(Ordering::Acquire) != 2 {
            core::hint::spin_loop();
        }
        1
    }
    PHASE.store(0, Ordering::Relaxed);
    let _ = execute(runner, coordinator, 2, peer, 2);
    super::arch::exit(false)
}

#[cfg(feature = "smp-all-blocked-test")]
pub(super) fn all_blocked(runner: &mut Runner) -> ! {
    fn park(context: &mut dyn ThreadContext, input: u64) -> u64 {
        let _witness = Witness;
        let local = Event::new();
        if !record(context, input) {
            return 0;
        }
        let _ = context.wait(&local, local.snapshot());
        0
    }
    DROPS.store(0, Ordering::Relaxed);
    let _ = execute(runner, park, 1, park, 1);
    super::arch::exit(false)
}

#[cfg(feature = "smp-peer-overflow-test")]
pub(super) fn peer_overflow(runner: &mut Runner) -> ! {
    fn spin(_: &mut dyn ThreadContext, _: u64) -> u64 {
        loop {
            core::hint::spin_loop();
        }
    }
    let _ = runner.run_smp(
        &jobs(spin, 0)[..1],
        &jobs(super::preempt::peer_overflow_worker, 1)[..1],
    );
    super::arch::exit(false)
}
