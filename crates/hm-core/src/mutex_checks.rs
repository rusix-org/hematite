//! Real cooperative workers consume only the safe mutex and thread interfaces.
#![forbid(unsafe_code)]

use super::{mutex::Mutex, threads::Runner};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use hm_kernel::MutexChecks;
use hm_kernel::thread_api::{ThreadContext, ThreadJob};

struct Shared {
    count: Cell<u64>,
    checksum: Cell<u64>,
}
const SALT: u64 = 0xd634_a812_9b57_c0ef;
const ROUNDS: u64 = 16;
static SHARED: Mutex<Shared> = Mutex::new(Shared {
    count: Cell::new(0),
    checksum: Cell::new(SALT),
});
static CONTENTIONS: AtomicUsize = AtomicUsize::new(0);
static CHECK_BUSY: AtomicBool = AtomicBool::new(false);

struct CheckLease;
impl Drop for CheckLease {
    fn drop(&mut self) {
        CHECK_BUSY.store(false, Ordering::Release);
    }
}

fn worker(context: &mut dyn ThreadContext, _: u64) -> u64 {
    for _ in 0..ROUNDS {
        let guard = match SHARED.try_lock() {
            Some(guard) => guard,
            None => {
                CONTENTIONS.fetch_add(1, Ordering::Relaxed);
                SHARED.lock_cooperative(context)
            }
        };
        let old = guard.count.get();
        if guard.checksum.get() != old ^ SALT {
            return 0;
        }
        let Some(next) = old.checked_add(1) else {
            return 0;
        };
        guard.count.set(next);
        // Intentionally leave the pair inconsistent across a real context
        // switch: no other worker may observe it until this guard is dropped.
        context.yield_now();
        if guard.count.get() != next || guard.checksum.get() != old ^ SALT {
            return 0;
        }
        guard.checksum.set(next ^ SALT);
        drop(guard);
        context.yield_now();
    }
    1
}

pub(super) fn checks(runner: &Runner) -> MutexChecks {
    let failed = || MutexChecks {
        contention: false,
        publication: false,
        completion: false,
    };
    if CHECK_BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return failed();
    }
    let _lease = CheckLease;
    let Some(reset) = SHARED.try_lock() else {
        return failed();
    };
    reset.count.set(0);
    reset.checksum.set(SALT);
    drop(reset);
    CONTENTIONS.store(0, Ordering::Relaxed);
    let jobs = [ThreadJob {
        entry: worker,
        input: 0,
    }; 4];
    let Ok(report) = runner.run(&jobs) else {
        return failed();
    };
    let Some(final_value) = SHARED.try_lock() else {
        return failed();
    };
    MutexChecks {
        contention: CONTENTIONS.load(Ordering::Relaxed) > 0,
        publication: final_value.count.get() == 4 * ROUNDS
            && final_value.checksum.get() == (4 * ROUNDS) ^ SALT,
        completion: report.completed == 4
            && report.results == [1; 4]
            && report.yields.iter().all(|&count| count >= 2 * ROUNDS),
    }
}
