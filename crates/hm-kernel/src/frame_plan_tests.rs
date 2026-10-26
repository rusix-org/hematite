//! In-guest selection tests. No addresses are dereferenced.
#![forbid(unsafe_code)]

use crate::boot_memory::Region;
use crate::frame_plan::{Error, MAX_EXCLUSIONS, MAX_FRAMES, MAX_RAM_REGIONS, PAGE_SIZE, Plan};

const fn region(start: u64, end: u64) -> Region {
    Region { start, end }
}

// Deliberately express the oracle using clipped intersections rather than
// the planner's containment and overlap inequalities.
fn matches_oracle(window: Region, ram: &[Region], excluded: &[Region]) -> bool {
    let Ok(plan) = Plan::build(window, ram, excluded) else {
        return false;
    };
    let count = ((window.end - window.start) / PAGE_SIZE as u64) as usize;
    let mut eligible_count = 0;
    for index in 0..count {
        let start = window.start + index as u64 * PAGE_SIZE as u64;
        let end = start + PAGE_SIZE as u64;
        let covered = ram
            .iter()
            .any(|r| end.min(r.end).saturating_sub(start.max(r.start)) == PAGE_SIZE as u64);
        let excluded_bytes = excluded
            .iter()
            .any(|r| end.min(r.end).saturating_sub(start.max(r.start)) != 0);
        let expected = covered && !excluded_bytes;
        if plan.eligible(index) != expected {
            return false;
        }
        eligible_count += usize::from(expected);
    }
    plan.base() == window.start
        && plan.page_count() == count
        && plan.total() == eligible_count
        && !plan.eligible(count)
        && !plan.eligible(usize::MAX)
}

#[inline(never)]
pub fn ranges() -> bool {
    let window = region(0x1000, 0x11000);
    let ram = [region(0, 0x6000), region(0x7001, 0x10001)];
    let excluded = [
        region(0x2001, 0x2002),
        region(0x3fff, 0x4001),
        region(0x4000, 0x5000),
        region(0x20000, 0x30000),
        region(0x10000, 0x11000),
    ];
    let Ok(plan) = Plan::build(window, &ram, &excluded) else {
        return false;
    };
    if plan.total() != 10 || !matches_oracle(window, &ram, &excluded) {
        return false;
    }
    // The DTB prefix, kernel tail and single-byte reservations each exclude
    // complete pages. Advertised RAM beyond the accessible window is ignored.
    let window = region(0x40000000, 0x41000000);
    let ram = [region(0x40000000, 0x48000000)];
    let excluded = [
        region(0x40000000, 0x40200000),
        region(0x40200000, 0x40230123),
        region(0x40900001, 0x40900002),
    ];
    if !matches_oracle(window, &ram, &excluded) {
        return false;
    }
    let Ok(plan) = Plan::build(window, &ram, &excluded) else {
        return false;
    };
    if plan.total() != 3534 || plan.eligible(560) || !plan.eligible(561) {
        return false;
    }
    // Joining two partial RAM descriptions must not invent one complete page.
    let window = region(0, 0x2000);
    let split = [region(0, 0x800), region(0x800, 0x2000)];
    let Ok(plan) = Plan::build(window, &split, &[]) else {
        return false;
    };
    plan.total() == 1
        && !plan.eligible(0)
        && plan.eligible(1)
        && matches_oracle(window, &split, &[])
        && matches_oracle(window, &[region(0, 0x1000), region(0x1000, 0x2000)], &[])
        && matches_oracle(window, &[], &[])
        && matches_oracle(window, &[window], &[window])
}

#[inline(never)]
pub fn boundaries() -> bool {
    let maximum = region(0, (MAX_FRAMES * PAGE_SIZE) as u64);
    let Ok(plan) = Plan::build(maximum, &[maximum], &[]) else {
        return false;
    };
    if plan.total() != MAX_FRAMES || !matches_oracle(maximum, &[maximum], &[]) {
        return false;
    }
    let window = region(0, PAGE_SIZE as u64);
    for (bad, expected) in [
        (region(0, 0), Error::Range),
        (region(4096, 0), Error::Range),
        (region(1, 4096), Error::Alignment),
        (region(0, 4097), Error::Alignment),
        (region(0, maximum.end + 4096), Error::Capacity),
        (region(0, u64::MAX), Error::Alignment),
    ] {
        if !matches!(Plan::build(bad, &[], &[]), Err(error) if error == expected) {
            return false;
        }
    }
    // Invalid ranges are rejected even when entirely outside the window.
    for bad in [region(9000, 9000), region(u64::MAX, 0)] {
        if !matches!(Plan::build(window, &[bad], &[]), Err(Error::Range))
            || !matches!(Plan::build(window, &[window], &[bad]), Err(Error::Range))
        {
            return false;
        }
    }
    if !matches!(
        Plan::build(window, &[window, region(4095, 8192)], &[]),
        Err(Error::Overlap)
    ) || !matches!(
        Plan::build(window, &[window; MAX_RAM_REGIONS + 1], &[]),
        Err(Error::Capacity)
    ) || !matches!(
        Plan::build(window, &[window], &[window; MAX_EXCLUSIONS + 1]),
        Err(Error::Capacity)
    ) {
        return false;
    }
    let mut ram = [window; MAX_RAM_REGIONS];
    for (index, range) in ram.iter_mut().enumerate() {
        let start = index as u64 * PAGE_SIZE as u64;
        *range = region(start, start + PAGE_SIZE as u64);
    }
    if !matches_oracle(
        maximum,
        &ram,
        &[region(0x10000000, 0x10001000); MAX_EXCLUSIONS],
    ) {
        return false;
    }
    // Last representable aligned page: no round-up overflow near u64::MAX.
    let high = region(u64::MAX - 8191, u64::MAX - 4095);
    let Ok(plan) = Plan::build(high, &[region(high.start, u64::MAX)], &[]) else {
        return false;
    };
    plan.total() == 1
        && matches_oracle(high, &[region(high.start, u64::MAX)], &[])
        && matches_oracle(high, &[high], &[region(high.end - 1, u64::MAX)])
}
