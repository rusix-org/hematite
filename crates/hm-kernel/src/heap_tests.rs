//! Actual ARM heap tests through safe policy traits; x86 rejection is explicit.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::frame_api::OwnedFrame;
use crate::heap_api::{HeapError, OwnedBlock};
use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};

fn disjoint(a: u64, a_size: u64, b: u64, b_size: u64) -> bool {
    match (a.checked_add(a_size), b.checked_add(b_size)) {
        (Some(a_end), Some(b_end)) => a_end <= b || b_end <= a,
        _ => false,
    }
}

#[inline(never)]
pub fn blocks(platform: &impl Platform) -> bool {
    let Some(before) = platform.frame_stats() else {
        return cfg!(target_arch = "x86_64")
            && matches!(platform.allocate_heap(1, 1), Err(HeapError::Unsupported));
    };
    if !cfg!(target_arch = "aarch64") || before.free < 8 {
        return false;
    }
    for (size, align) in [(1, 0), (1, 3), (usize::MAX, 1), (usize::MAX, 4096)] {
        if !matches!(
            platform.allocate_heap(size, align),
            Err(HeapError::InvalidLayout)
        ) || platform.frame_stats() != Some(before)
        {
            return false;
        }
    }
    // Both are valid Layouts; neither can fit this fixed mapped arena.
    for (size, align) in [(17 * 1024 * 1024, 4096), (1, 1usize << 40)] {
        if !matches!(
            platform.allocate_heap(size, align),
            Err(HeapError::Exhausted)
        ) || platform.frame_stats() != Some(before)
        {
            return false;
        }
    }

    let Ok(mut frame) = platform.allocate_frame() else {
        return false;
    };
    frame.bytes_mut().fill(0x67);
    let Ok(mut first) = platform.allocate_heap(8193, 8192) else {
        return false;
    };
    let Ok(mut second) = platform.allocate_heap(4097, 4096) else {
        return false;
    };
    let address = first.physical_address();
    if !address.is_multiple_of(8192)
        || !second.physical_address().is_multiple_of(4096)
        || first.bytes().len() != 8193
        || second.bytes().len() != 4097
        || first.bytes().iter().any(|b| *b != 0)
        || second.bytes().iter().any(|b| *b != 0)
        || !disjoint(address, 12288, second.physical_address(), 8192)
        || !disjoint(address, 12288, frame.physical_address(), 4096)
        || !disjoint(
            second.physical_address(),
            8192,
            frame.physical_address(),
            4096,
        )
        || platform.frame_stats().map(|s| s.free) != before.free.checked_sub(6)
    {
        return false;
    }
    first.bytes_mut().fill(0xa5);
    second.bytes_mut().fill(0x5a);
    if first.bytes().iter().any(|b| *b != 0xa5) || frame.bytes().iter().any(|b| *b != 0x67) {
        return false;
    }
    drop(first);
    let Ok(first) = platform.allocate_heap(8193, 8192) else {
        return false;
    };
    if first.physical_address() != address
        || first.bytes().iter().any(|b| *b != 0)
        || second.bytes().iter().any(|b| *b != 0x5a)
        || frame.bytes().iter().any(|b| *b != 0x67)
    {
        return false;
    }
    drop(first);
    drop(second);
    drop(frame);
    if platform.frame_stats() != Some(before) {
        return false;
    }

    let Ok(mut empty) = platform.allocate_heap(0, 8192) else {
        return false;
    };
    if !empty.bytes().is_empty()
        || !empty.bytes_mut().is_empty()
        || !empty.physical_address().is_multiple_of(8192)
        || platform.frame_stats().map(|s| s.free) != before.free.checked_sub(1)
    {
        return false;
    }
    drop(empty);
    platform.frame_stats() == Some(before)
}

struct Counted<'a> {
    drops: &'a Cell<usize>,
    value: u64,
}
impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
    }
}

#[inline(never)]
fn counted_value(platform: &impl Platform) -> bool {
    let before = platform.frame_stats();
    let drops = Cell::new(0);
    let Ok(mut value) = platform.box_value(Counted {
        drops: &drops,
        value: 17,
    }) else {
        return false;
    };
    if value.value != 17 || drops.get() != 0 {
        return false;
    }
    value.value = 29;
    if value.value != 29
        || platform.frame_stats().map(|s| s.free) != before.and_then(|s| s.free.checked_sub(1))
    {
        return false;
    }
    drop(value);
    drops.get() == 1 && platform.frame_stats() == before
}

#[inline(never)]
fn borrowed_value(platform: &impl Platform) -> bool {
    let before = platform.frame_stats();
    let mut value = 17u64;
    let Ok(mut owner) = platform.box_value(&mut value) else {
        return false;
    };
    **owner = 31;
    drop(owner);
    value == 31 && platform.frame_stats() == before
}

static ZERO_DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(8192))]
struct Zero;
impl Drop for Zero {
    fn drop(&mut self) {
        ZERO_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(never)]
fn zero_value(platform: &impl Platform) -> bool {
    let before = platform.frame_stats();
    ZERO_DROPS.store(0, Ordering::Relaxed);
    let Ok(value) = platform.box_value(Zero) else {
        return false;
    };
    let address = &*value as *const Zero as usize;
    if core::mem::size_of_val(&*value) != 0
        || address == 0
        || !address.is_multiple_of(8192)
        || ZERO_DROPS.load(Ordering::Relaxed) != 0
        || platform.frame_stats().map(|s| s.free) != before.and_then(|s| s.free.checked_sub(1))
    {
        return false;
    }
    drop(value);
    ZERO_DROPS.load(Ordering::Relaxed) == 1 && platform.frame_stats() == before
}

struct Reentrant<'a, P: Platform> {
    platform: &'a P,
    drops: &'a Cell<usize>,
    okay: &'a Cell<bool>,
    expected_free: usize,
}
impl<P: Platform> Drop for Reentrant<'_, P> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
        let before = self.platform.frame_stats();
        let retained = before.map(|s| s.free) == Some(self.expected_free);
        let Ok(mut block) = self.platform.allocate_heap(4097, 4096) else {
            self.okay.set(false);
            return;
        };
        let original = self as *const Self as usize as u64;
        let separate = disjoint(
            original,
            core::mem::size_of::<Self>() as u64,
            block.physical_address(),
            8192,
        );
        if !separate {
            self.okay.set(false);
            return;
        }
        block.bytes_mut().fill(0x9d);
        let contents = block.bytes().len() == 4097 && block.bytes().iter().all(|b| *b == 0x9d);
        drop(block);
        self.okay
            .set(retained && contents && self.platform.frame_stats() == before);
    }
}

#[inline(never)]
fn reentrant_value<P: Platform>(platform: &P) -> bool {
    let Some(before) = platform.frame_stats() else {
        return false;
    };
    let Some(expected_free) = before.free.checked_sub(1) else {
        return false;
    };
    let drops = Cell::new(0);
    let okay = Cell::new(false);
    let Ok(value) = platform.box_value(Reentrant {
        platform,
        drops: &drops,
        okay: &okay,
        expected_free,
    }) else {
        return false;
    };
    drop(value);
    drops.get() == 1 && okay.get() && platform.frame_stats() == Some(before)
}

#[inline(never)]
pub fn values(platform: &impl Platform) -> bool {
    if platform.frame_stats().is_none() {
        let drops = Cell::new(0);
        let result = platform.box_value(Counted {
            drops: &drops,
            value: 71,
        });
        let Err((error, value)) = result else {
            return false;
        };
        let retained = error == HeapError::Unsupported && value.value == 71 && drops.get() == 0;
        drop(value);
        return cfg!(target_arch = "x86_64") && retained && drops.get() == 1;
    }
    if !cfg!(target_arch = "aarch64") {
        return false;
    }
    let Some(before) = platform.frame_stats() else {
        return false;
    };
    if !counted_value(platform)
        || !borrowed_value(platform)
        || !zero_value(platform)
        || !reentrant_value(platform)
        || platform.frame_stats() != Some(before)
    {
        return false;
    }
    // Preserve a live typed value across exhaustion and fragmented-run denial.
    let Ok(mut retained) = platform.box_value(0x1234_5678_u64) else {
        return false;
    };
    if !platform.heap_exhaustion_check() || *retained != 0x1234_5678 {
        return false;
    }
    *retained = 0x8765_4321;
    drop(retained);
    let Some(after) = platform.frame_stats() else {
        return false;
    };
    // Core diagnostics retain one deliberately forgotten typed value once per
    // pool. Repeated diagnostics must preserve all remaining capacity.
    after.total == before.total
        && (after.free == before.free || after.free.checked_add(1) == Some(before.free))
        && platform.heap_exhaustion_check()
        && platform.frame_stats() == Some(after)
}
