//! Single-CPU owned data mappings; design 0008 specifies the physical proof.
use core::arch::asm;
use core::cell::Cell;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicBool, Ordering};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::{MapError, MapStats, OwnedMapping};

use super::frames::{Frame, Pool};

pub const SLOTS: usize = 64;
const BASE: usize = 0x8000_0000;
const PAGE: usize = 4096;
const RW_NX: u64 = 0x0060_0000_0000_0703;
static CLAIMED: AtomicBool = AtomicBool::new(false);

// SAFETY: Bootstrap/linker provide permanent aligned hardware table arrays,
// excluded from allocation. Rust forms no references to these mutable tables.
unsafe extern "C" {
    static mut arm_owned_l2: [u64; 512];
    static mut arm_owned_l3: [u64; 512];
}

pub struct Mapper {
    occupied: Cell<[u64; SLOTS]>,
    leak_checked: Cell<bool>,
    _local: core::marker::PhantomData<*mut ()>,
}

pub struct MappedPage<'mapper, 'pool> {
    mapper: &'mapper Mapper,
    slot: usize,
    frame: ManuallyDrop<Frame<'pool>>,
}

impl Mapper {
    /// Caller proves the permanent arena hierarchy is installed, all leaves
    /// invalid, and one CPU at EL1 with asynchronous exceptions masked owns
    /// all table writes. Pool identity mappings cannot change. No DMA exists.
    /// Later scoped timer IRQs touch only private assembly/GIC state; all
    /// mapping operations remain masked (design 0009). Preemptible jobs cannot
    /// capture this private non-Sync capability; scheduler IRQs never map.
    // SAFETY: Successful construction transfers sole arena update authority;
    // the permanent latch prevents reconstruction even after forgotten owners.
    pub(crate) unsafe fn from_boot() -> Result<Self, ()> {
        CLAIMED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ())?;
        Ok(Self {
            occupied: Cell::new([0; SLOTS]),
            leak_checked: Cell::new(false),
            _local: core::marker::PhantomData,
        })
    }

    pub fn map<'mapper, 'pool>(
        &'mapper self,
        frame: Frame<'pool>,
    ) -> Result<MappedPage<'mapper, 'pool>, (MapError, Frame<'pool>)> {
        let mut occupied = self.occupied.get();
        let Some(slot) = occupied.iter().position(|address| *address == 0) else {
            return Err((MapError::Exhausted, frame));
        };
        if leaf(slot) != 0 {
            return Err((MapError::CorruptState, frame));
        }
        let address = frame.physical_address();
        let mapped = MappedPage {
            mapper: self,
            slot,
            frame: ManuallyDrop::new(frame),
        };
        occupied[slot] = address;
        self.occupied.set(occupied);
        // SAFETY: Slot is vacant and bounded, and mapped exclusively retains
        // its initialized allocator frame. No previous frame-byte borrow can
        // survive the move. Matching normal-memory attributes avoid conflicting
        // aliases; frame identity bytes remain inaccessible until unmap.
        unsafe {
            core::ptr::addr_of_mut!(arm_owned_l3)
                .cast::<u64>()
                .add(slot)
                .write_volatile(address | RW_NX);
            asm!("dsb sy", "isb", options(nostack, preserves_flags));
        }
        Ok(mapped)
    }

    pub fn stats(&self) -> MapStats {
        MapStats {
            total: SLOTS,
            free: self.occupied.get().iter().filter(|a| **a == 0).count(),
        }
    }

    /// Caller owns the unique live mapping for slot, without live byte borrows.
    // SAFETY: Only owning MappedPage removal calls this with its private slot;
    // retaining the frame until completion prevents early physical reuse.
    unsafe fn remove(&self, slot: usize) {
        // SAFETY: Caller provides a live bounded slot under the single-CPU
        // contract. Full local invalidation finishes all old translations
        // before ledger release; asm retains compiler memory side effects.
        unsafe {
            core::ptr::addr_of_mut!(arm_owned_l3)
                .cast::<u64>()
                .add(slot)
                .write_volatile(0);
            asm!(
                "dsb sy",
                "tlbi vmalle1",
                "dsb sy",
                "isb",
                options(nostack, preserves_flags)
            );
        }
        let mut occupied = self.occupied.get();
        occupied[slot] = 0;
        self.occupied.set(occupied);
    }

    /// Exercises actual translation observations and one permanent leaked owner.
    #[inline(never)]
    pub fn hardware_check(&self, pool: &Pool) -> bool {
        let before = pool.stats();
        let slots_before = self.stats();
        let Ok(mut frame) = pool.allocate() else {
            return false;
        };
        frame.bytes_mut().fill(0x37);
        let old_pa = frame.physical_address();
        let Ok(mut mapped) = self.map(frame) else {
            return false;
        };
        let slot = mapped.slot;
        let mut okay = translation(slot) == Some(old_pa)
            && mapped.bytes().iter().all(|b| *b == 0x37)
            && tables_match(Some(self));
        mapped.bytes_mut().fill(0x49);
        let old_frame = mapped.unmap();
        okay &= translation(slot).is_none() && old_frame.bytes().iter().all(|b| *b == 0x49);
        // Retain the old physical owner while repeatedly changing the same VA
        // to another physical page, warming actual data translations each time.
        for value in 1..=16u8 {
            let Ok(mut next) = pool.allocate() else {
                return false;
            };
            let next_pa = next.physical_address();
            next.bytes_mut().fill(value);
            let Ok(mut mapped) = self.map(next) else {
                return false;
            };
            okay &= mapped.slot == slot
                && next_pa != old_pa
                && translation(slot) == Some(next_pa)
                && mapped.bytes().iter().all(|b| *b == value);
            mapped.bytes_mut().fill(value.wrapping_add(100));
            okay &= mapped.bytes().iter().all(|b| *b == value.wrapping_add(100));
            drop(mapped);
            okay &= translation(slot).is_none() && old_frame.bytes().iter().all(|b| *b == 0x49);
            let Ok(reused) = pool.allocate() else {
                return false;
            };
            okay &= reused.physical_address() == next_pa && reused.bytes().iter().all(|b| *b == 0);
            drop(reused);
        }
        drop(old_frame);
        okay &= pool.stats() == before && self.stats() == slots_before;
        let newly_leaked = !self.leak_checked.get();
        if newly_leaked {
            let Ok(frame) = pool.allocate() else {
                return false;
            };
            let Ok(mapped) = self.map(frame) else {
                return false;
            };
            let leaked_slot = mapped.slot;
            let leaked_pa = mapped.physical_address();
            core::mem::forget(mapped);
            self.leak_checked.set(true);
            okay &= self.occupied.get()[leaked_slot] == leaked_pa
                && translation(leaked_slot) == Some(leaked_pa);
            let Ok(other) = pool.allocate() else {
                return false;
            };
            okay &= other.physical_address() != leaked_pa;
            let Ok(other) = self.map(other) else {
                return false;
            };
            okay &= other.slot != leaked_slot;
            drop(other);
        }
        let consumed = usize::from(newly_leaked);
        okay && pool.stats().free.checked_add(consumed) == Some(before.free)
            && self.stats().free.checked_add(consumed) == Some(slots_before.free)
            && tables_match(Some(self))
    }
}

impl<'pool> OwnedMapping for MappedPage<'_, 'pool> {
    type Frame = Frame<'pool>;

    fn virtual_address(&self) -> u64 {
        (BASE + self.slot * PAGE) as u64
    }
    fn physical_address(&self) -> u64 {
        self.frame.physical_address()
    }
    fn bytes(&self) -> &[u8; PAGE] {
        // SAFETY: The mapping exclusively retains initialized RAM. Its original
        // Frame bytes are private and unused. Shared borrow prevents mutation,
        // unmapping or reclaiming storage for the returned reference lifetime.
        unsafe { &*(self.virtual_address() as *const [u8; PAGE]) }
    }
    fn bytes_mut(&mut self) -> &mut [u8; PAGE] {
        // SAFETY: Exclusive mapping borrow and consumed frame ensure unique
        // byte access. Identity aliases have no usable byte reference/access
        // path. The mapped initialized page remains retained for this borrow.
        unsafe { &mut *(self.virtual_address() as *mut [u8; PAGE]) }
    }
    fn unmap(mut self) -> Self::Frame {
        // SAFETY: Consuming this unique handle ends all usable mapped borrows.
        // remove finishes invalidation before extracting the retained frame.
        // Forget then suppresses Drop, so neither removal nor frame drop repeats.
        let frame = unsafe {
            self.mapper.remove(self.slot);
            ManuallyDrop::take(&mut self.frame)
        };
        core::mem::forget(self);
        frame
    }
}

impl Drop for MappedPage<'_, '_> {
    fn drop(&mut self) {
        // SAFETY: Exclusive destructor borrow rules out usable mapped borrows.
        // Complete invalidation precedes freeing the unique retained frame.
        unsafe {
            self.mapper.remove(self.slot);
            ManuallyDrop::drop(&mut self.frame);
        }
    }
}

fn leaf(slot: usize) -> u64 {
    if slot >= 512 {
        return u64::MAX;
    }
    // SAFETY: Bounded raw access to permanent mapped linker-owned table storage;
    // single CPU and no hardware descriptor updates avoid concurrent writes.
    unsafe {
        core::ptr::addr_of!(arm_owned_l3)
            .cast::<u64>()
            .add(slot)
            .read_volatile()
    }
}

fn translation(slot: usize) -> Option<u64> {
    if slot >= SLOTS {
        return None;
    }
    let par: u64;
    // SAFETY: EL1 address translation probes only a bounded arena VA and never
    // dereferences it. ISB completes the probe before reading PAR_EL1. No other
    // CPU/interrupt handler can race this diagnostic's PAR observation.
    unsafe {
        asm!(
            "at s1e1r, {address}",
            "isb",
            "mrs {result}, par_el1",
            address = in(reg) BASE + slot * PAGE,
            result = out(reg) par,
            options(nostack, preserves_flags)
        );
    }
    if par & 1 != 0 {
        None
    } else {
        Some(par & 0x000f_ffff_ffff_f000)
    }
}

pub(super) fn tables_match(mapper: Option<&Mapper>) -> bool {
    let occupied = mapper.map_or([0; SLOTS], |m| m.occupied.get());
    let l3 = core::ptr::addr_of!(arm_owned_l3) as u64;
    for slot in 0..512 {
        // SAFETY: Bounded raw read of the permanent mapped linker-owned L2;
        // no Rust references or runtime writer to this table exist.
        let l2 = unsafe {
            core::ptr::addr_of!(arm_owned_l2)
                .cast::<u64>()
                .add(slot)
                .read_volatile()
        };
        let expected = occupied.get(slot).copied().filter(|pa| *pa != 0);
        if l2 != if slot == 0 { l3 | 3 } else { 0 }
            || leaf(slot) != expected.map_or(0, |pa| pa | RW_NX)
        {
            return false;
        }
    }
    true
}
