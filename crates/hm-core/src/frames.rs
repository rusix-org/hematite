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
    heap_leak_checked: Cell<bool>,
}

pub struct Frame<'pool> {
    pool: &'pool Pool,
    index: usize,
}

impl Pool {
    /// If this call claims the previously-unclaimed latch, the caller proves
    /// every eligible page is exclusively owned, initialized,
    /// mapped writable normal RAM, initially with no aliases, users or DMA.
    /// The plan excludes all boot storage and reservations. Identity mappings
    /// never change. Only the consuming owned-mapping protocol (design 0008)
    /// may create matching-attribute aliases, without identity byte access until
    /// invalidation completes. Single CPU, interrupts masked.
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
            heap_leak_checked: Cell::new(false),
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

/// Unique page-backed byte allocation. No public construction or raw release.
pub struct Block<'pool> {
    pool: &'pool Pool,
    start: usize,
    pages: usize,
    length: usize,
}

/// A fully initialized Rust value. Its backing bytes are deliberately private:
/// T's padding need not remain initialized after a typed write.
pub struct KernelBox<'pool, T> {
    _block: Block<'pool>,
    value: *mut T,
    owns: core::marker::PhantomData<T>,
}

impl Pool {
    fn claim_run(
        &self,
        pages: usize,
        align: usize,
    ) -> Result<usize, hm_kernel::heap_api::HeapError> {
        use hm_kernel::heap_api::HeapError;
        if pages == 0 || !align.is_power_of_two() {
            return Err(HeapError::InvalidLayout);
        }
        let Some(last) = self.plan.page_count().checked_sub(pages) else {
            return Err(HeapError::Exhausted);
        };
        let mut used = self.used.get();
        for start in 0..=last {
            if !self.address(start).is_multiple_of(align) {
                continue;
            }
            let end = start + pages; // bounded by page_count <= MAX_FRAMES
            if (start..end).all(|i| self.plan.eligible(i) && used[i / 64] & (1u64 << (i % 64)) == 0)
            {
                for i in start..end {
                    used[i / 64] |= 1u64 << (i % 64);
                }
                self.used.set(used);
                return Ok(start);
            }
        }
        Err(HeapError::Exhausted)
    }

    /// Caller owns every bit in this bounded run, with no surviving byte/typed
    /// aliases or overlapping Frame/Block. Reclamation follows value destruction.
    // SAFETY: Caller supplies exclusive ownership of the whole claimed run.
    unsafe fn release_run(&self, start: usize, pages: usize) {
        let mut used = self.used.get();
        for i in start..start + pages {
            used[i / 64] &= !(1u64 << (i % 64));
        }
        self.used.set(used);
    }

    pub fn allocate_heap(
        &self,
        size: usize,
        align: usize,
    ) -> Result<Block<'_>, hm_kernel::heap_api::HeapError> {
        use hm_kernel::heap_api::HeapError;
        let layout = core::alloc::Layout::from_size_align(size, align)
            .map_err(|_| HeapError::InvalidLayout)?;
        let rounded = layout
            .size()
            .max(1)
            .checked_add(PAGE_SIZE - 1)
            .ok_or(HeapError::InvalidLayout)?;
        let pages = rounded / PAGE_SIZE;
        let start = self.claim_run(pages, layout.align())?;
        // SAFETY: Entire contiguous eligible run was claimed transactionally.
        // Constructor proves mapped exclusive initialized normal RAM; no live
        // Frame, Block or reference overlaps it. pages <= MAX_FRAMES bounds
        // the full zeroing length, which precedes any public byte access.
        unsafe {
            core::ptr::write_bytes(self.address(start) as *mut u8, 0, pages * PAGE_SIZE);
        }
        Ok(Block {
            pool: self,
            start,
            pages,
            length: layout.size(),
        })
    }

    pub fn box_value<T>(
        &self,
        value: T,
    ) -> Result<KernelBox<'_, T>, (hm_kernel::heap_api::HeapError, T)> {
        let layout = core::alloc::Layout::new::<T>();
        let block = match self.allocate_heap(layout.size(), layout.align()) {
            Ok(block) => block,
            Err(error) => return Err((error, value)),
        };
        let pointer = block.pool.address(block.start) as *mut T;
        // SAFETY: Block exclusively owns at least size_of<T> bytes at an
        // address satisfying align_of<T>, including a non-null page for ZSTs.
        // ptr::write moves a valid supplied T without dropping untyped storage.
        // No byte reference exists or is exposed after this typed initialization.
        unsafe {
            pointer.write(value);
        }
        Ok(KernelBox {
            _block: block,
            value: pointer,
            owns: core::marker::PhantomData,
        })
    }

    /// Bounded runtime diagnostic. Retains one deliberate forgotten typed value
    /// once per pool, then proves failures return their input values intact.
    #[inline(never)]
    pub fn heap_exhaustion_check(&self) -> bool {
        use hm_kernel::heap_api::HeapError;
        struct Tracked<'a> {
            drops: &'a Cell<usize>,
            marker: u64,
        }
        impl Drop for Tracked<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get().saturating_add(1));
            }
        }
        let forgotten_drops = Cell::new(0);
        if !self.heap_leak_checked.get() {
            let Ok(value) = self.box_value(Tracked {
                drops: &forgotten_drops,
                marker: 17,
            }) else {
                return false;
            };
            core::mem::forget(value);
            self.heap_leak_checked.set(true);
            if forgotten_drops.get() != 0 {
                return false;
            }
        }
        let original = self.used.get();
        let before = self.stats();
        let mut ledger = [0u64; WORDS];
        let mut count = 0;
        let mut okay = true;
        while let Ok(index) = self.claim() {
            let mask = 1u64 << (index % 64);
            if ledger[index / 64] & mask != 0 || original[index / 64] & mask != 0 {
                okay = false;
                break;
            }
            ledger[index / 64] |= mask;
            count += 1;
        }
        okay &= count == before.free && self.stats().free == 0;
        let full = self.used.get();
        okay &= matches!(self.allocate_heap(8193, 8192), Err(HeapError::Exhausted));
        okay &= matches!(self.allocate_heap(0, 1), Err(HeapError::Exhausted));
        let drops = Cell::new(0);
        match self.box_value(Tracked {
            drops: &drops,
            marker: 0x12345678,
        }) {
            Err((HeapError::Exhausted, value)) => {
                okay &= value.marker == 0x12345678 && drops.get() == 0;
                drop(value);
            }
            other => {
                okay = false;
                drop(other);
            }
        }
        okay &= drops.get() == 1 && self.used.get() == full;
        // Free only even ledger-owned indices: enough total capacity remains,
        // but every potential two-page run has an unavailable neighbour.
        for index in (0..self.plan.page_count()).step_by(2) {
            let mask = 1u64 << (index % 64);
            if ledger[index / 64] & mask != 0 {
                // SAFETY: This diagnostic exclusively owns the ledger index;
                // no byte or typed reference was ever created for this claim.
                unsafe {
                    self.release(index);
                }
                ledger[index / 64] &= !mask;
            }
        }
        let fragmented = self.used.get();
        okay &= self.stats().free >= 2
            && matches!(
                self.allocate_heap(4097, PAGE_SIZE),
                Err(HeapError::Exhausted)
            )
            && self.used.get() == fragmented;
        for index in 0..self.plan.page_count() {
            if ledger[index / 64] & (1u64 << (index % 64)) != 0 {
                // SAFETY: Only this diagnostic claimed each ledger index;
                // failed public allocations created no overlapping owners.
                unsafe {
                    self.release(index);
                }
            }
        }
        okay && self.used.get() == original && self.stats() == before
    }
}

impl hm_kernel::heap_api::OwnedBlock for Block<'_> {
    fn physical_address(&self) -> u64 {
        self.pool.address(self.start) as u64
    }
    fn bytes(&self) -> &[u8] {
        // SAFETY: This unique byte Block retains its initialized contiguous
        // run. Borrow ties the slice to self and prevents mutation/drop. Typed
        // KernelBox backing blocks are private and never expose this API.
        unsafe {
            core::slice::from_raw_parts(self.pool.address(self.start) as *const u8, self.length)
        }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: Exclusive Block borrow plus unique run ownership rules out
        // all aliases. Layout bounds length by isize::MAX; backing is normal
        // mapped initialized RAM, and lifetime cannot outlive this borrow.
        unsafe {
            core::slice::from_raw_parts_mut(self.pool.address(self.start) as *mut u8, self.length)
        }
    }
}

impl Drop for Block<'_> {
    fn drop(&mut self) {
        // SAFETY: Private non-cloneable Block owns exactly this run, and its
        // exclusive drop borrow rules out surviving byte aliases. KernelBox
        // calls T's destructor before this backing field is dropped.
        unsafe {
            self.pool.release_run(self.start, self.pages);
        }
    }
}

impl<T> core::ops::Deref for KernelBox<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: box_value initialized T at this aligned private pointer and
        // backing Block retains storage. Shared borrow permits only shared T.
        unsafe { &*self.value }
    }
}
impl<T> core::ops::DerefMut for KernelBox<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: Unique box and exclusive borrow permit mutable access to its
        // live T. No byte API exposes backing storage or invalid T padding.
        unsafe { &mut *self.value }
    }
}
impl<T> Drop for KernelBox<'_, T> {
    fn drop(&mut self) {
        // SAFETY: This is the unique initialized T, with no surviving borrows.
        // Run its destructor once while Block still owns storage. No Cell
        // borrow/lock is held, so destructors may allocate other disjoint runs.
        unsafe {
            core::ptr::drop_in_place(self.value);
        }
        // Rust drops self._block only after this destructor returns.
    }
}
