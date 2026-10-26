//! Real event waits through the platform; no duplicate scheduler model.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::preemption_tests::report_valid;
use crate::thread_api::{DispatchCause, ThreadContext, ThreadError, ThreadJob};
use crate::wait_api::{Event, WaitError};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

static BUSY: AtomicBool = AtomicBool::new(false);
static EVENTS: [Event; 2] = [const { Event::new() }; 2];
static READY: AtomicUsize = AtomicUsize::new(0);
static PHASE: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static VALUE: AtomicU64 = AtomicU64::new(0);
static ACK: AtomicU64 = AtomicU64::new(0);
const ROUNDS: u64 = 16;

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

pub(super) fn events() -> bool {
    struct Unsupported;
    impl ThreadContext for Unsupported {
        fn id(&self) -> usize {
            0
        }
        fn yield_now(&mut self) {}
    }
    let mut context = Unsupported;
    let event = Event::default();
    crate::wait_api::contract()
        && context.wait(&event, event.snapshot()) == Err(WaitError::Unsupported)
        && context.wait(&event, u64::MAX) == Err(WaitError::Unsupported)
}

fn changed(context: &mut dyn ThreadContext, _: u64) -> u64 {
    let local = Event::new();
    let observed = local.snapshot();
    if local.notify() != Ok(1)
        || context.wait(&local, observed) != Ok(())
        || context.wait(&local, u64::MAX) != Ok(())
        || !crate::wait_api::terminal_waits(context)
    {
        return 0;
    }
    // Moving the local after completed calls cannot leave scheduler pointers.
    let moved = local;
    u64::from(moved.snapshot() == 1 && moved.notify() == Ok(2))
}

#[inline(never)]
fn broadcast_waiter(context: &mut dyn ThreadContext, input: u64) -> u64 {
    let id = context.id();
    if id >= 3 {
        return 0;
    }
    let pattern = input as u8;
    let mut bytes = [pattern; 256];
    let dropped = Cell::new(0usize);
    {
        let _witness = Witness(&dropped);
        let observed = EVENTS[0].snapshot();
        READY.fetch_or(1usize << id, Ordering::Release);
        core::hint::black_box(&mut bytes);
        if context.wait(&EVENTS[0], observed) != Ok(())
            || PHASE.load(Ordering::Relaxed) != 2
            || VALUE.load(Ordering::Relaxed) != 0xa891_12bc
            || bytes.iter().any(|byte| *byte != pattern)
            || context.id() != id
            || dropped.get() != 0
        {
            return 0;
        }
    }
    u64::from(dropped.get() == 1)
}

fn broadcast_notifier(context: &mut dyn ThreadContext, _: u64) -> u64 {
    while READY.load(Ordering::Acquire) != 7 {
        context.yield_now();
    }
    // Let every announced waiter reach its actual wait call, even when the
    // boundary-pressure image interrupts immediately before registration.
    for _ in 0..8 {
        context.yield_now();
    }
    PHASE.store(1, Ordering::Relaxed);
    if EVENTS[1].notify().is_err() {
        return 0;
    }
    // Unrelated notifications and runnable peers must not dispatch waiters.
    for _ in 0..8 {
        context.yield_now();
    }
    VALUE.store(0xa891_12bc, Ordering::Relaxed);
    PHASE.store(2, Ordering::Relaxed);
    // No explicit yield follows publication: completion must trigger scanning.
    u64::from(EVENTS[0].notify().is_ok())
}

#[inline(never)]
fn consumer(context: &mut dyn ThreadContext, _: u64) -> u64 {
    let dropped = Cell::new(0usize);
    let mut local = [0u64; 32];
    {
        let _witness = Witness(&dropped);
        for round in 1..=ROUNDS {
            loop {
                let observed = EVENTS[0].snapshot();
                if VALUE.load(Ordering::Relaxed) == round {
                    break;
                }
                if context.wait(&EVENTS[0], observed).is_err() {
                    return 0;
                }
            }
            local[round as usize] = round.wrapping_mul(0x1234);
            core::hint::black_box(&mut local);
            if context.id() != 0 || dropped.get() != 0 {
                return 0;
            }
            ACK.store(round, Ordering::Relaxed);
            if EVENTS[1].notify().is_err() {
                return 0;
            }
            if round % 4 == 0 {
                context.yield_now();
            }
        }
    }
    u64::from(
        dropped.get() == 1
            && (1..=ROUNDS).all(|round| local[round as usize] == round.wrapping_mul(0x1234)),
    )
}

#[inline(never)]
fn producer(context: &mut dyn ThreadContext, _: u64) -> u64 {
    let dropped = Cell::new(0usize);
    let mut local = [0x93u8; 256];
    {
        let _witness = Witness(&dropped);
        for round in 1..=ROUNDS {
            VALUE.store(round, Ordering::Relaxed);
            if EVENTS[0].notify().is_err() {
                return 0;
            }
            if round % 4 == 0 {
                // A running notifier must be timer-preempted to let the
                // parked consumer observe publication and acknowledge it.
                while ACK.load(Ordering::Acquire) != round {
                    core::hint::spin_loop();
                }
            } else {
                loop {
                    let observed = EVENTS[1].snapshot();
                    if ACK.load(Ordering::Relaxed) == round {
                        break;
                    }
                    if context.wait(&EVENTS[1], observed).is_err() {
                        return 0;
                    }
                }
            }
            core::hint::black_box(&mut local);
            if local != [0x93; 256] || dropped.get() != 0 || context.id() != 1 {
                return 0;
            }
        }
    }
    u64::from(dropped.get() == 1)
}

fn protections(platform: &impl Platform) -> bool {
    let c = platform.checks();
    c.write_protect
        && c.nx
        && c.wx
        && c.stack_guard
        && c.null_guard
        && c.user_execute_protect
        && c.user_access_protect
}

#[inline(never)]
pub(super) fn workers(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return platform.run_preemptive(&[]) == Err(ThreadError::Unsupported);
    }
    let Some(_fixture) = Fixture::acquire() else {
        return false;
    };
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    let Ok(immediate) = platform.run_preemptive(&[ThreadJob {
        entry: changed,
        input: 0,
    }]) else {
        return false;
    };
    if !report_valid(&immediate, 1)
        || immediate.run.results != [1, 0, 0, 0]
        || immediate.parks != [0; 4]
        || immediate.run.yields != [0; 4]
    {
        return false;
    }
    for _ in 0..2 {
        READY.store(0, Ordering::Relaxed);
        PHASE.store(0, Ordering::Relaxed);
        VALUE.store(0, Ordering::Relaxed);
        DROPS.store(0, Ordering::Relaxed);
        let Ok(report) = platform.run_preemptive(&[
            ThreadJob {
                entry: broadcast_waiter,
                input: 17,
            },
            ThreadJob {
                entry: broadcast_waiter,
                input: 29,
            },
            ThreadJob {
                entry: broadcast_waiter,
                input: 41,
            },
            ThreadJob {
                entry: broadcast_notifier,
                input: 0,
            },
        ]) else {
            return false;
        };
        if !report_valid(&report, 4)
            || report.run.results != [1; 4]
            || report.parks != [1, 1, 1, 0]
            || report.run.yields[..3] != [0; 3]
            || DROPS.load(Ordering::Relaxed) != 3
            || PHASE.load(Ordering::Relaxed) != 2
        {
            return false;
        }
        // This bounded workload fits the trace; each blocked continuation has
        // exactly one Wake dispatch, never an explicit-yield substitute.
        if report.run.truncated {
            return false;
        }
        for id in 0..3 {
            if (0..report.run.trace_len)
                .filter(|index| {
                    report.run.trace[*index] == id && report.causes[*index] == DispatchCause::Wake
                })
                .count()
                != 1
            {
                return false;
            }
        }
        VALUE.store(0, Ordering::Relaxed);
        ACK.store(0, Ordering::Relaxed);
        DROPS.store(0, Ordering::Relaxed);
        let Ok(report) = platform.run_preemptive(&[
            ThreadJob {
                entry: consumer,
                input: 0,
            },
            ThreadJob {
                entry: producer,
                input: 0,
            },
        ]) else {
            return false;
        };
        if !report_valid(&report, 2)
            || report.run.results != [1, 1, 0, 0]
            || report.parks[0] == 0
            || report.parks[1] == 0
            || report.preemptions[1] == 0
            || report.run.yields[0] != ROUNDS / 4
            || DROPS.load(Ordering::Relaxed) != 2
            || VALUE.load(Ordering::Relaxed) != ROUNDS
            || ACK.load(Ordering::Relaxed) != ROUNDS
        {
            return false;
        }
    }
    platform.frame_stats() == frames
        && platform.mapping_stats() == mappings
        && protections(platform)
}
