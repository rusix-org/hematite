//! Single-CPU frame ownership. Construction proof: design 0005.
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};
use hm_kernel::frame_api::{FrameError, FrameStats, OwnedFrame};
use hm_kernel::frame_plan::{MAX_FRAMES, PAGE_SIZE, Plan};

static CLAIMED: AtomicBool = AtomicBool::new(false);
const WORDS: usize = MAX_FRAMES / 64;

pub struct Pool {
    plan: Plan,
    used: Cell<[u64; WORDS]>,
    leak_checked: Cell<bool>,
}

pub struct Frame<'pool> {
    pool: &'pool Pool,
    index: usize,
}

impl Pool {
    /// If this call claims the previously-unclaimed latch, the caller proves
    /// every eligible page is exclusively owned, initialized,
    /// mapped writable normal RAM, with no aliases, users or DMA. The plan
    /// excludes all boot storage and reservations. No mapping may change
    /// while the pool or any frame exists. Single CPU, interrupts masked.
    /// If already claimed, no physical precondition applies: the method returns
    /// AlreadyInitialized before using the supplied plan.
    // SAFETY: Caller establishes the documented physical ownership contract
    // when claiming the latch; a rejected call never accesses physical memory.
    pub(crate) unsafe fn from_boot(plan: Plan) -> Result<Self, FrameError> {
        CLAIMED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| FrameError::AlreadyInitialized)?;
        Ok(Self {
            plan,
            used: Cell::new([0; WORDS]),
            leak_checked: Cell::new(false),
        })
    }

    fn claim(&self) -> Result<usize, FrameError> {
        let mut used = self.used.get();
        for index in 0..self.plan.page_count() {
            let mask = 1u64 << (index % 64);
            if self.plan.eligible(index) && used[index / 64] & mask == 0 {
                used[index / 64] |= mask;
                self.used.set(used);
                return Ok(index);
            }
        }
        Err(FrameError::Exhausted)
    }

    /// Caller owns this allocated index exclusively, with no surviving byte
    /// borrows. Only Frame::drop or the private exhaustion ledger returns bits.
    // SAFETY: Caller owns the claimed valid index without live byte aliases.
    unsafe fn release(&self, index: usize) {
        let mut used = self.used.get();
        used[index / 64] &= !(1u64 << (index % 64));
        self.used.set(used);
    }

    fn address(&self, index: usize) -> usize {
        (self.plan.base() + index as u64 * PAGE_SIZE as u64) as usize
    }

    pub fn allocate(&self) -> Result<Frame<'_>, FrameError> {
        let index = self.claim()?;
        // SAFETY: Constructor proves all eligible pages are exclusive mapped
        // RAM. claim just removed this page from availability; no Frame or
        // byte reference to it exists. Full zeroing initializes every byte
        // before returning its sole owner. No callback/reentrancy occurs.
        unsafe {
            core::ptr::write_bytes(self.address(index) as *mut u8, 0, PAGE_SIZE);
        }
        Ok(Frame { pool: self, index })
    }

    pub fn stats(&self) -> FrameStats {
        let used: usize = self
            .used
            .get()
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum();
        FrameStats {
            total: self.plan.total(),
            free: self.plan.total() - used,
        }
    }

    /// Diagnostic owns only its newly claimed indices. No active frame is
    /// released, even on a failed check; all ledger claims are cleaned up.
    #[inline(never)]
    pub fn exhaustion_check(&self) -> bool {
        let Ok(empty) = Plan::build(
            hm_kernel::boot_memory::Region {
                start: 0,
                end: PAGE_SIZE as u64,
            },
            &[],
            &[],
        ) else {
            return false;
        };
        // SAFETY: This existing pool proves the latch was already claimed,
        // so construction must reject without accessing physical memory.
        // The empty plan also has no pages requiring an ownership proof.
        let attempted = unsafe { Self::from_boot(empty) };
        if !matches!(attempted, Err(FrameError::AlreadyInitialized)) {
            return false;
        }
        if !self.leak_checked.get() {
            let Ok(frame) = self.allocate() else {
                return false;
            };
            core::mem::forget(frame);
            self.leak_checked.set(true);
        }
        let original = self.used.get();
        let before = self.stats();
        let mut ledger = [0u64; WORDS];
        let mut count = 0;
        let mut okay = true;
        while let Ok(index) = self.claim() {
            let mask = 1u64 << (index % 64);
            if ledger[index / 64] & mask != 0
                || original[index / 64] & mask != 0
                || !self.plan.eligible(index)
            {
                okay = false;
                break;
            }
            ledger[index / 64] |= mask;
            count += 1;
        }
        okay &= count == before.free && self.stats().free == 0;
        for _ in 0..3 {
            okay &= matches!(self.allocate(), Err(FrameError::Exhausted));
        }
        for index in 0..self.plan.page_count() {
            if ledger[index / 64] & (1u64 << (index % 64)) != 0 {
                // SAFETY: Ledger contains only indices claimed by this call;
                // no Frame or byte reference was created for any of them.
                unsafe {
                    self.release(index);
                }
            }
        }
        okay && self.used.get() == original && self.stats() == before
    }
}

impl OwnedFrame for Frame<'_> {
    fn physical_address(&self) -> u64 {
        self.pool.address(self.index) as u64
    }
    fn bytes(&self) -> &[u8; PAGE_SIZE] {
        // SAFETY: This live non-cloneable frame retains its pool's allocation.
        // Memory is fully initialized and mapped; shared borrow prevents any
        // concurrent bytes_mut or drop through safe code. Lifetime is &self.
        unsafe { &*(self.pool.address(self.index) as *const [u8; PAGE_SIZE]) }
    }
    fn bytes_mut(&mut self) -> &mut [u8; PAGE_SIZE] {
        // SAFETY: Sole frame ownership and exclusive frame borrow establish
        // unique access to initialized mapped RAM. No user/device/other alias
        // exists; the returned lifetime cannot outlive this frame borrow.
        unsafe { &mut *(self.pool.address(self.index) as *mut [u8; PAGE_SIZE]) }
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        // SAFETY: The private index belongs to this unique frame. Its exclusive
        // drop borrow proves no byte reference can still be used by safe code.
        unsafe {
            self.pool.release(self.index);
        }
    }
}
