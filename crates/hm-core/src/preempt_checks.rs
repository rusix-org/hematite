//! Safe clients exercise preempted mutex ownership and nested-run rejection.
#![forbid(unsafe_code)]

use super::{mutex::Mutex, threads::Runner};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use hm_kernel::thread_api::{ThreadContext, ThreadError, ThreadJob};

const SALT: u64 = 0xcba5_72e1_94d6_803f;
struct Shared {
    count: Cell<u64>,
    checksum: Cell<u64>,
}
static SHARED: Mutex<Shared> = Mutex::new(Shared {
    count: Cell::new(0),
    checksum: Cell::new(SALT),
});
static CHECK_BUSY: AtomicBool = AtomicBool::new(false);
static OWNER_READY: AtomicBool = AtomicBool::new(false);
static PEERS: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static NESTED: AtomicUsize = AtomicUsize::new(0);

struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        CHECK_BUSY.store(false, Ordering::Release);
    }
}
struct Witness;
impl Drop for Witness {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

fn worker(context: &mut dyn ThreadContext, _: u64) -> u64 {
    let _witness = Witness;
    if super::preempt::run(&[]) != Err(ThreadError::Busy) {
        return 0;
    }
    NESTED.fetch_add(1, Ordering::Relaxed);
    if context.id() == 0 {
        let Some(guard) = SHARED.try_lock() else {
            return 0;
        };
        if guard.count.get() != 0 || guard.checksum.get() != SALT {
            return 0;
        }
        guard.count.set(1);
        OWNER_READY.store(true, Ordering::Release);
        // No yield: peers cannot observe this publication without the timer
        // suspending this guard's owner. The protected pair is inconsistent.
        while PEERS.load(Ordering::Acquire) != 0b1110 {
            core::hint::spin_loop();
        }
        if guard.count.get() != 1 || guard.checksum.get() != SALT {
            return 0;
        }
        guard.checksum.set(1 ^ SALT);
        drop(guard);
        return 1;
    }
    let id = context.id();
    if id > 3 {
        return 0;
    }
    while !OWNER_READY.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    if SHARED.try_lock().is_some() {
        return 0;
    }
    PEERS.fetch_or(1usize << id, Ordering::Release);
    let guard = SHARED.lock_cooperative(context);
    let old = guard.count.get();
    if guard.checksum.get() != old ^ SALT {
        return 0;
    }
    let Some(next) = old.checked_add(1) else {
        return 0;
    };
    guard.count.set(next);
    context.yield_now();
    if guard.count.get() != next || guard.checksum.get() != old ^ SALT {
        return 0;
    }
    guard.checksum.set(next ^ SALT);
    1
}

pub(super) fn checks(runner: &Runner) -> bool {
    if CHECK_BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return false;
    }
    let _lease = Lease;
    for _ in 0..2 {
        let Some(reset) = SHARED.try_lock() else {
            return false;
        };
        reset.count.set(0);
        reset.checksum.set(SALT);
        drop(reset);
        OWNER_READY.store(false, Ordering::Relaxed);
        PEERS.store(0, Ordering::Relaxed);
        DROPS.store(0, Ordering::Relaxed);
        NESTED.store(0, Ordering::Relaxed);
        let Ok(report) = runner.run_preemptive(
            &[ThreadJob {
                entry: worker,
                input: 0,
            }; 4],
        ) else {
            return false;
        };
        let Some(value) = SHARED.try_lock() else {
            return false;
        };
        if report.run.completed != 4
            || report.run.results != [1; 4]
            || report.preemptions[0] == 0
            || report.run.yields[0] != 0
            || report.run.yields[1..].contains(&0)
            || value.count.get() != 4
            || value.checksum.get() != 4 ^ SALT
            || PEERS.load(Ordering::Relaxed) != 0b1110
            || DROPS.load(Ordering::Relaxed) != 4
            || NESTED.load(Ordering::Relaxed) != 4
        {
            return false;
        }
    }
    true
}
