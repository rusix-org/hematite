//! Exercise actual mutex ownership through the safe platform boundary.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::mutex_api::OwnedMutex;
use crate::thread_api::ThreadContext;
use core::cell::Cell;

struct ReleaseOnYield<G> {
    held: Option<G>,
    yields: usize,
}

impl<G> ThreadContext for ReleaseOnYield<G> {
    fn id(&self) -> usize {
        0
    }

    fn yield_now(&mut self) {
        self.yields = self.yields.saturating_add(1);
        drop(self.held.take());
    }
}

#[inline(never)]
pub fn contract(platform: &impl Platform) -> bool {
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    let mut mutex = platform.new_mutex([1u64, 2, 3]);
    let independent = platform.new_mutex(17u64);
    let Some(mut first) = mutex.try_lock() else {
        return false;
    };
    first[1] = 29;
    if mutex.try_lock().is_some() {
        return false;
    }
    let Some(mut other) = independent.try_lock() else {
        return false;
    };
    *other += 1;
    if *first != [1, 29, 3] || *other != 18 {
        return false;
    }
    drop(other);
    // A real lock attempt must release no value and invoke the scheduling
    // context only after failing to acquire. This context models completion
    // of the earlier owner's work by dropping its guard on the first yield.
    let mut context = ReleaseOnYield {
        held: Some(first),
        yields: 0,
    };
    let mut acquired = mutex.lock_cooperative(&mut context);
    if context.yields != 1 || context.held.is_some() || *acquired != [1, 29, 3] {
        return false;
    }
    acquired[2] = 41;
    drop(acquired);
    let acquired = mutex.lock_cooperative(&mut context);
    if context.yields != 1 || *acquired != [1, 29, 41] {
        return false;
    }
    drop(acquired);
    drop(context);
    mutex.get_mut()[0] = 53;
    mutex.into_inner() == [53, 29, 41]
        && independent.into_inner() == 18
        && platform.frame_stats() == frames
        && platform.mapping_stats() == mappings
}

struct Tracked<'a> {
    drops: &'a Cell<u64>,
    value: u64,
}

impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
    }
}

#[inline(never)]
pub fn lifetime(platform: &impl Platform) -> bool {
    let drops = Cell::new(0);
    let mutex = platform.new_mutex(Tracked {
        drops: &drops,
        value: 17,
    });
    for expected in 17..21 {
        let Some(mut guard) = mutex.try_lock() else {
            return false;
        };
        if guard.value != expected || drops.get() != 0 {
            return false;
        }
        guard.value += 1;
    }
    let value = mutex.into_inner();
    if value.value != 21 || drops.get() != 0 {
        return false;
    }
    drop(value);
    if drops.get() != 1 {
        return false;
    }
    let mut forgotten = platform.new_mutex(Tracked {
        drops: &drops,
        value: 31,
    });
    let Some(guard) = forgotten.try_lock() else {
        return false;
    };
    core::mem::forget(guard);
    // Forget does not unlock or destroy protected data. Shared acquisition
    // stays unavailable; exclusive ownership still permits get_mut/into_inner.
    if forgotten.try_lock().is_some() || drops.get() != 1 {
        return false;
    }
    forgotten.get_mut().value = 43;
    if forgotten.try_lock().is_some() {
        return false;
    }
    let value = forgotten.into_inner();
    if value.value != 43 || drops.get() != 1 {
        return false;
    }
    drop(value);
    if drops.get() != 2 {
        return false;
    }
    {
        let mutex = platform.new_mutex(Tracked {
            drops: &drops,
            value: 59,
        });
        let Some(guard) = mutex.try_lock() else {
            return false;
        };
        if guard.value != 59 {
            return false;
        }
        drop(guard);
    }
    drops.get() == 3
}
