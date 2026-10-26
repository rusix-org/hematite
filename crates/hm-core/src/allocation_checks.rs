//! Safe clients of the shared boot allocator; design 0020.
#![forbid(unsafe_code)]

use super::frames::{Frame, Pool};
use super::secondary::{Runner, SmpReport, with_pool};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use hm_kernel::frame_api::{FrameError, OwnedFrame};
use hm_kernel::frame_plan::{MAX_FRAMES, PAGE_SIZE};
use hm_kernel::heap_api::{HeapError, OwnedBlock};
use hm_kernel::thread_api::{ThreadContext, ThreadEntry};

const BASE: u64 = 0x4000_0000;
const CHURN: usize = 64;
const TYPED: usize = 32;
const CYCLES: usize = 2;
const CAPACITY: usize = MAX_FRAMES / 8;
static LIVE: [AtomicU64; MAX_FRAMES / 64] = [const { AtomicU64::new(0) }; MAX_FRAMES / 64];
static LIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicBool = AtomicBool::new(false);
static READY: AtomicUsize = AtomicUsize::new(0);
static CHURN_RELEASED: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
static CHURN_ALLOCATED: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
static INITIAL_FREE: AtomicUsize = AtomicUsize::new(0);
static NEIGHBOUR: AtomicU64 = AtomicU64::new(0);
static FORGOTTEN: AtomicU64 = AtomicU64::new(0);
static FINAL_FREE: AtomicUsize = AtomicUsize::new(0);
static FULL: [AtomicUsize; CYCLES] = [const { AtomicUsize::new(0) }; CYCLES];
static CHECKED: [AtomicUsize; CYCLES] = [const { AtomicUsize::new(0) }; CYCLES];
static RELEASED: [AtomicUsize; CYCLES] = [const { AtomicUsize::new(0) }; CYCLES];
static FINISHED: [AtomicUsize; CYCLES] = [const { AtomicUsize::new(0) }; CYCLES];

fn register(pool: &Pool, address: u64, pages: usize) -> bool {
    let Some(offset) = address.checked_sub(BASE) else {
        return false;
    };
    if !offset.is_multiple_of(PAGE_SIZE as u64) {
        return false;
    }
    let start = (offset / PAGE_SIZE as u64) as usize;
    let Some(end) = start.checked_add(pages) else {
        return false;
    };
    if pages == 0 || end > MAX_FRAMES {
        return false;
    }
    for index in start..end {
        let physical = BASE + (index * PAGE_SIZE) as u64;
        if physical == NEIGHBOUR.load(Ordering::Acquire)
            || physical == FORGOTTEN.load(Ordering::Acquire)
            || pool.excludes_region(physical, physical + PAGE_SIZE as u64)
            || LIVE[index / 64].fetch_or(1u64 << (index % 64), Ordering::AcqRel)
                & (1u64 << (index % 64))
                != 0
        {
            return false;
        }
    }
    let count = LIVE_COUNT.fetch_add(pages, Ordering::AcqRel) + pages;
    PEAK.fetch_max(count, Ordering::Relaxed);
    true
}
fn unregister(address: u64, pages: usize) {
    let start = ((address - BASE) / PAGE_SIZE as u64) as usize;
    for index in start..start + pages {
        let bit = 1u64 << (index % 64);
        if LIVE[index / 64].fetch_and(!bit, Ordering::AcqRel) & bit == 0 {
            FAILED.store(true, Ordering::Release);
        }
    }
    LIVE_COUNT.fetch_sub(pages, Ordering::AcqRel);
}
fn barrier(counter: &AtomicUsize, context: &mut dyn ThreadContext) {
    counter.fetch_add(1, Ordering::AcqRel);
    while counter.load(Ordering::Acquire) != 8 {
        context.yield_now();
    }
}
fn irq_ok(before: u64) -> bool {
    super::frames::irq_state() == before && before & 0x80 == 0
}
fn start(context: &dyn ThreadContext, input: u64) -> bool {
    super::smp_checks::record(context, input) && super::frames::irq_state() & 0x80 == 0
}

fn isolation_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !start(context, input) {
        return 0;
    }
    with_pool(|pool| {
        let irq = super::frames::irq_state();
        if !pool.critical_section_check() || !irq_ok(irq) {
            return 0;
        }
        for round in 0..CHURN {
            if context.cpu_id() == 1 && round != 0 {
                while CHURN_RELEASED[context.id()].load(Ordering::Acquire) < round {
                    context.yield_now();
                }
            }
            let Ok(mut frame) = pool.allocate() else {
                return 0;
            };
            let (size, align) = [(4097, 4096), (8193, 8192), (1, 1)][round % 3];
            let Ok(mut block) = pool.allocate_heap(size, align) else {
                return 0;
            };
            let pages = size.div_ceil(PAGE_SIZE);
            if !register(pool, frame.physical_address(), 1)
                || !register(pool, block.physical_address(), pages)
                || !frame.bytes().iter().all(|b| *b == 0)
                || !block.bytes().iter().all(|b| *b == 0)
                || !block.physical_address().is_multiple_of(align as u64)
                || !irq_ok(irq)
            {
                return 0;
            }
            ALLOCATIONS.fetch_add(2, Ordering::Relaxed);
            if context.cpu_id() == 1 {
                CHURN_ALLOCATED[context.id()].store(round + 1, Ordering::Release);
            }
            let pattern = (input as u8)
                .wrapping_mul(19)
                .wrapping_add(round as u8)
                .wrapping_add(1);
            frame.bytes_mut().fill(pattern);
            block.bytes_mut().fill(!pattern);
            if round == 0 {
                barrier(&READY, context);
                if super::preempt::worker_register_probe(input) != 1 {
                    return 0;
                }
            }
            context.yield_now();
            if !frame.bytes().iter().all(|b| *b == pattern)
                || !block.bytes().iter().all(|b| *b == !pattern)
                || !irq_ok(irq)
            {
                return 0;
            }
            unregister(frame.physical_address(), 1);
            drop(frame);
            if context.cpu_id() == 0 {
                CHURN_RELEASED[context.id()].store(round + 1, Ordering::Release);
                if round + 1 < CHURN {
                    while CHURN_ALLOCATED[context.id()].load(Ordering::Acquire) < round + 2 {
                        context.yield_now();
                    }
                }
            }
            context.yield_now();
            if !block.bytes().iter().all(|b| *b == !pattern) {
                return 0;
            }
            unregister(block.physical_address(), pages);
            drop(block);
            if !irq_ok(irq) {
                return 0;
            }
        }
        1
    })
    .unwrap_or(0)
}

struct FailureValue(u64);
impl Drop for FailureValue {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(never)]
fn exhaust_owned(pool: &Pool, context: &mut dyn ThreadContext, input: u64) -> u64 {
    // Actual owner storage is bounded; emitted assembly is checked separately.
    let mut owners: [Option<Frame<'_>>; CAPACITY] = core::array::from_fn(|_| None);
    let irq = super::frames::irq_state();
    for cycle in 0..CYCLES {
        for slot in &mut owners {
            match pool.allocate() {
                Ok(mut frame) => {
                    if !register(pool, frame.physical_address(), 1)
                        || !frame.bytes().iter().all(|b| *b == 0)
                    {
                        return 0;
                    }
                    frame.bytes_mut().fill((input as u8).wrapping_add(1));
                    *slot = Some(frame);
                    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                }
                Err(FrameError::Exhausted) => break,
                Err(_) => return 0,
            }
            context.yield_now();
        }
        barrier(&FULL[cycle], context);
        if pool.stats().free != 0
            || LIVE_COUNT.load(Ordering::Acquire) != INITIAL_FREE.load(Ordering::Acquire)
        {
            return 0;
        }
        for _ in 0..3 {
            if !matches!(pool.allocate(), Err(FrameError::Exhausted))
                || !matches!(pool.allocate_heap(4097, 8192), Err(HeapError::Exhausted))
                || !matches!(pool.allocate_heap(0, 1), Err(HeapError::Exhausted))
            {
                return 0;
            }
        }
        match pool.box_value(FailureValue(0x1234_5678)) {
            Err((HeapError::Exhausted, value)) if value.0 == 0x1234_5678 => drop(value),
            _ => return 0,
        }
        if !matches!(pool.allocate_heap(7, 3), Err(HeapError::InvalidLayout)) || !irq_ok(irq) {
            return 0;
        }
        barrier(&CHECKED[cycle], context);
        for slot in &mut owners {
            if let Some(frame) = slot.take() {
                if !frame
                    .bytes()
                    .iter()
                    .all(|b| *b == (input as u8).wrapping_add(1))
                {
                    return 0;
                }
                unregister(frame.physical_address(), 1);
                drop(frame);
            }
        }
        barrier(&RELEASED[cycle], context);
        if pool.stats().free != INITIAL_FREE.load(Ordering::Acquire)
            || LIVE_COUNT.load(Ordering::Acquire) != 0
            || !irq_ok(irq)
        {
            return 0;
        }
        barrier(&FINISHED[cycle], context);
    }
    1
}
fn exhaustion_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !start(context, input) {
        return 0;
    }
    with_pool(|pool| exhaust_owned(pool, context, input)).unwrap_or(0)
}

struct Reenter<'a> {
    pool: &'a Pool,
    value: Cell<u64>,
}
impl Drop for Reenter<'_> {
    fn drop(&mut self) {
        let own = self as *const Self as u64;
        match self.pool.allocate() {
            Ok(mut frame) => {
                let address = frame.physical_address();
                if address <= own && own < address + PAGE_SIZE as u64
                    || !register(self.pool, address, 1)
                    || !frame.bytes().iter().all(|b| *b == 0)
                {
                    FAILED.store(true, Ordering::Release);
                    return;
                }
                frame.bytes_mut().fill(0x5a);
                ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                unregister(address, 1);
                drop(frame);
            }
            Err(_) => FAILED.store(true, Ordering::Release),
        }
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
struct Zero;
impl Drop for Zero {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
#[repr(align(8192))]
struct Aligned([u8; 17]);
fn reentry_worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    if !start(context, input) {
        return 0;
    }
    with_pool(|pool| {
        let irq = super::frames::irq_state();
        for round in 0..TYPED {
            let local = input.wrapping_add(round as u64);
            let Ok(value) = pool.box_value(Reenter {
                pool,
                value: Cell::new(local),
            }) else {
                return 0;
            };
            let Ok(borrowed) = pool.box_value(&local) else {
                return 0;
            };
            let Ok(aligned) = pool.box_value(Aligned([round as u8; 17])) else {
                return 0;
            };
            let Ok(zero) = pool.box_value(Zero) else {
                return 0;
            };
            ALLOCATIONS.fetch_add(4, Ordering::Relaxed);
            if round == 0 {
                barrier(&READY, context);
                if super::preempt::worker_register_probe(input) != 1 {
                    return 0;
                }
            }
            context.yield_now();
            value.value.set(local.wrapping_add(1));
            if **borrowed != local
                || aligned.0 != [round as u8; 17]
                || !((&*aligned as *const Aligned as usize).is_multiple_of(8192))
                || value.value.get() != local.wrapping_add(1)
                || !irq_ok(irq)
            {
                return 0;
            }
            drop(value);
            drop(borrowed);
            drop(aligned);
            drop(zero);
            if !irq_ok(irq) {
                return 0;
            }
        }
        1
    })
    .unwrap_or(0)
}

fn run_case(runner: &mut Runner, pool: &Pool, worker: ThreadEntry) -> Option<SmpReport> {
    if with_pool(|_| ()).is_some()
        || !pool.critical_section_check()
        || super::frames::irq_state() != 0x3c0
    {
        return None;
    }
    if FORGOTTEN.load(Ordering::Acquire) == 0 {
        let mut leaked = pool.allocate().ok()?;
        leaked.bytes_mut().fill(0x6d);
        FORGOTTEN.store(leaked.physical_address(), Ordering::Release);
        core::mem::forget(leaked);
    }
    let masked_before = pool.stats();
    if !matches!(pool.allocate_heap(7, 3), Err(HeapError::InvalidLayout))
        || !matches!(
            pool.allocate_heap(MAX_FRAMES * PAGE_SIZE, PAGE_SIZE),
            Err(HeapError::Exhausted)
        )
        || super::frames::irq_state() != 0x3c0
        || pool.stats() != masked_before
    {
        return None;
    }
    let mut neighbour = pool.allocate().ok()?;
    NEIGHBOUR.store(neighbour.physical_address(), Ordering::Release);
    neighbour.bytes_mut().fill(0xc7);
    let before = pool.stats();
    INITIAL_FREE.store(before.free, Ordering::Relaxed);
    FINAL_FREE.store(0, Ordering::Relaxed);
    for word in &LIVE {
        word.store(0, Ordering::Relaxed);
    }
    for counter in [&LIVE_COUNT, &PEAK, &ALLOCATIONS, &DROPS, &READY] {
        counter.store(0, Ordering::Relaxed);
    }
    for group in [&FULL, &CHECKED, &RELEASED, &FINISHED] {
        for counter in group {
            counter.store(0, Ordering::Relaxed);
        }
    }
    for group in [&CHURN_RELEASED, &CHURN_ALLOCATED] {
        for counter in group {
            counter.store(0, Ordering::Relaxed);
        }
    }
    FAILED.store(false, Ordering::Relaxed);
    let report = super::smp_checks::execute_with_pool(runner, pool, worker, 4, worker, 4)?;
    let after = pool.stats();
    FINAL_FREE.store(after.free, Ordering::Relaxed);
    if before != after
        || FAILED.load(Ordering::Acquire)
        || LIVE_COUNT.load(Ordering::Acquire) != 0
        || LIVE.iter().any(|word| word.load(Ordering::Acquire) != 0)
        || !neighbour.bytes().iter().all(|b| *b == 0xc7)
        || with_pool(|_| ()).is_some()
        || super::frames::irq_state() != 0x3c0
    {
        return None;
    }
    Some(report)
}
pub(super) fn isolation(runner: &mut Runner, pool: &Pool) -> Option<SmpReport> {
    let report = run_case(runner, pool, isolation_worker)?;
    if CHURN_RELEASED
        .iter()
        .chain(CHURN_ALLOCATED.iter())
        .any(|v| v.load(Ordering::Acquire) != CHURN)
        || ALLOCATIONS.load(Ordering::Relaxed) != 8 * CHURN * 2
        || PEAK.load(Ordering::Relaxed) < 24
        || report.reports.iter().any(|r| r.preemptions.contains(&0))
    {
        return None;
    }
    Some(report)
}
pub(super) fn exhaustion(runner: &mut Runner, pool: &Pool) -> Option<SmpReport> {
    let report = run_case(runner, pool, exhaustion_worker)?;
    if ALLOCATIONS.load(Ordering::Relaxed) != INITIAL_FREE.load(Ordering::Relaxed) * CYCLES
        || DROPS.load(Ordering::Relaxed) != 8 * CYCLES
        || PEAK.load(Ordering::Relaxed) != INITIAL_FREE.load(Ordering::Relaxed)
    {
        return None;
    }
    Some(report)
}
pub(super) fn reentry(runner: &mut Runner, pool: &Pool) -> Option<SmpReport> {
    let report = run_case(runner, pool, reentry_worker)?;
    if ALLOCATIONS.load(Ordering::Relaxed) != 8 * TYPED * 5
        || DROPS.load(Ordering::Relaxed) != 8 * TYPED * 2
        || report.reports.iter().any(|r| r.preemptions.contains(&0))
    {
        return None;
    }
    Some(report)
}
pub(super) fn metrics() -> [usize; 5] {
    [&INITIAL_FREE, &FINAL_FREE, &ALLOCATIONS, &PEAK, &DROPS].map(|v| v.load(Ordering::Acquire))
}
