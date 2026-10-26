//! Bounded physical-page eligibility metadata, never memory authority.
//! See docs/design/0005-frame-allocation.md for the trusted construction boundary.

use crate::boot_memory::Region;

pub const PAGE_SIZE: usize = 4096;
pub const MAX_FRAMES: usize = 4096;
pub const MAX_RAM_REGIONS: usize = 16;
pub const MAX_EXCLUSIONS: usize = 34;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Range,
    Alignment,
    Capacity,
    Overlap,
}

/// A freely inspectable plan. Its integer addresses do not authorize access.
pub struct Plan {
    base: u64,
    pages: usize,
    eligible: [u64; MAX_FRAMES / 64],
    total: usize,
}

impl Plan {
    pub fn build(window: Region, ram: &[Region], excluded: &[Region]) -> Result<Self, Error> {
        if ram.len() > MAX_RAM_REGIONS || excluded.len() > MAX_EXCLUSIONS {
            return Err(Error::Capacity);
        }
        if window.start >= window.end {
            return Err(Error::Range);
        }
        if !window.start.is_multiple_of(PAGE_SIZE as u64)
            || !window.end.is_multiple_of(PAGE_SIZE as u64)
        {
            return Err(Error::Alignment);
        }
        let count = window.end.checked_sub(window.start).ok_or(Error::Range)? / PAGE_SIZE as u64;
        if count > MAX_FRAMES as u64 {
            return Err(Error::Capacity);
        }
        for (index, region) in ram.iter().enumerate() {
            validate(*region)?;
            if ram[..index].iter().any(|other| overlaps(*region, *other)) {
                return Err(Error::Overlap);
            }
        }
        for region in excluded {
            validate(*region)?;
        }

        let mut plan = Self {
            base: window.start,
            pages: count as usize,
            eligible: [0; MAX_FRAMES / 64],
            total: 0,
        };
        for index in 0..plan.pages {
            let offset = (index as u64)
                .checked_mul(PAGE_SIZE as u64)
                .ok_or(Error::Range)?;
            let start = plan.base.checked_add(offset).ok_or(Error::Range)?;
            let page = Region {
                start,
                end: start.checked_add(PAGE_SIZE as u64).ok_or(Error::Range)?,
            };
            if ram
                .iter()
                .any(|region| region.start <= page.start && page.end <= region.end)
                && !excluded.iter().any(|region| overlaps(page, *region))
            {
                plan.eligible[index / 64] |= 1u64 << (index % 64);
                plan.total += 1;
            }
        }
        Ok(plan)
    }

    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn page_count(&self) -> usize {
        self.pages
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn eligible(&self, index: usize) -> bool {
        index < self.pages && self.eligible[index / 64] & (1u64 << (index % 64)) != 0
    }
}

fn validate(region: Region) -> Result<(), Error> {
    if region.start < region.end {
        Ok(())
    } else {
        Err(Error::Range)
    }
}

fn overlaps(left: Region, right: Region) -> bool {
    left.start < right.end && right.start < left.end
}
