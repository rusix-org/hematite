//! CPU-local software state; ownership proofs: designs 0015 and 0018.
use core::arch::{asm, global_asm};

#[repr(C, align(64))]
struct Record {
    timer: [u64; 5],
    preempt: [u64; 4],
    irq_count: u64,
    boundary_flags: u64,
    wait_expiries: u64,
    cpu_id: u64,
    reserved: [u64; 3],
}
pub(super) const TIMER: usize = core::mem::offset_of!(Record, timer);
pub(super) const PREEMPT: usize = core::mem::offset_of!(Record, preempt);
pub(super) const IRQ_COUNT: usize = core::mem::offset_of!(Record, irq_count);
pub(super) const BOUNDARY_FLAGS: usize = core::mem::offset_of!(Record, boundary_flags);
pub(super) const WAIT_EXPIRIES: usize = core::mem::offset_of!(Record, wait_expiries);
const CPU_ID: usize = core::mem::offset_of!(Record, cpu_id);
pub(super) const STRIDE: usize = core::mem::size_of::<Record>();
pub(super) const CANARY: u64 = 0xa55a_c33c_9669_f00f;
pub(super) const AFFINITY_MASK: u64 = 0xff00_ffffff;
const _: () = {
    assert!(TIMER == 0 && PREEMPT == 40 && IRQ_COUNT == 72);
    assert!(BOUNDARY_FLAGS == 80 && WAIT_EXPIRIES == 88 && CPU_ID == 96);
    assert!(STRIDE == 128 && core::mem::align_of::<Record>() == 64);
};

// SAFETY: Assembly owns these two disjoint records. CPU1 accesses its slot
// only during the exclusive lifecycle session; CPU0 waits for Returned+OFF.
// IRQ-masked Rust observations use raw scalar loads and retain no references.
global_asm!(
    ".section .bss.cpu_local,\"aw\",@nobits",
    ".balign 64",
    ".global arm_cpu_records",
    "arm_cpu_records:",
    ".skip {bytes}",
    bytes = const STRIDE * 2,
);
// SAFETY: Symbol denotes the aligned assembly allocation above, not a Rust
// value that may be borrowed while assembly mutates its scalar fields.
unsafe extern "C" {
    static mut arm_cpu_records: [u8; STRIDE * 2];
}

fn base() -> usize {
    core::ptr::addr_of!(arm_cpu_records) as usize
}

/// Compiler memory effects on both asm blocks prevent observations crossing
/// the mask window. No callback, allocation or potentially panicking indexing.
fn masked<T>(observe: impl FnOnce(usize, u64) -> T) -> T {
    let (daif, pointer, mpidr): (u64, usize, u64);
    // SAFETY: EL1 boot authority is established before Rust; DAIF is restored
    // exactly, so calling from an IRQ-enabled worker preserves its state.
    unsafe {
        asm!("mrs {daif}, daif", "msr daifset, #2", "isb",
             "mrs {pointer}, tpidr_el1", "mrs {mpidr}, mpidr_el1",
             daif = out(reg) daif, pointer = out(reg) pointer,
             mpidr = out(reg) mpidr, options(nostack, preserves_flags));
    }
    let result = observe(pointer, mpidr);
    // SAFETY: Observation has finished and retained no shared mutable state;
    // restore the caller's exact mask after its final load.
    unsafe {
        asm!("msr daif, {}", "isb", in(reg) daif, options(nostack, preserves_flags));
    }
    result
}

fn valid(pointer: usize, mpidr: u64) -> bool {
    if pointer != base() || mpidr & AFFINITY_MASK != 0 {
        return false;
    }
    // SAFETY: Exact known base and CPU0 affinity checked before dereference;
    // all callers mask IRQ and boot has initialized the identity to zero.
    unsafe { ((pointer + CPU_ID) as *const u64).read_volatile() == 0 }
}

pub(super) fn current_id() -> Option<u64> {
    masked(|pointer, mpidr| valid(pointer, mpidr).then_some(0))
}

pub(super) fn irq_count() -> u64 {
    read_counter(IRQ_COUNT)
}

#[cfg(feature = "preemption-boundary-test")]
pub(super) fn wait_expiries() -> u64 {
    read_counter(WAIT_EXPIRIES)
}

fn read_counter(offset: usize) -> u64 {
    masked(|pointer, mpidr| {
        if !valid(pointer, mpidr) {
            return 0;
        }
        // SAFETY: Private callers supply only asserted scalar offsets; base
        // validation and IRQ masking exclude concurrent writes on this CPU.
        unsafe { ((pointer + offset) as *const u64).read_volatile() }
    })
}

pub(super) fn checks() -> bool {
    masked(|pointer, mpidr| {
        if !valid(pointer, mpidr)
            || valid(0, mpidr)
            || valid(base() + 1, mpidr)
            || valid(base() + STRIDE, mpidr)
            || valid(base(), 1)
            || valid(base(), 1 << 8)
            || valid(base(), 1 << 16)
            || valid(base(), 1 << 32)
        {
            return false;
        }
        for word in 0..STRIDE / 8 {
            // SAFETY: This private baseline diagnostic runs outside the lifecycle
            // lease with CPU1 OFF; bounded scalar loads stay in its 128 bytes.
            let value = unsafe { ((base() + STRIDE + word * 8) as *const u64).read_volatile() };
            if value != CANARY {
                return false;
            }
        }
        true
    })
}

/// Caller holds BUSY and has confirmed CPU1 OFF before transferring slot1.
#[cfg(feature = "secondary-tests")]
// SAFETY: No live peer can read/write the record; only fixed scalar slots are
// initialized, with no Rust references retained across admission.
pub(super) unsafe fn prepare_secondary() {
    for word in 0..STRIDE / 8 {
        // SAFETY: Exact slot1 bounds and exclusive OFF ownership above.
        unsafe {
            ((base() + STRIDE + word * 8) as *mut u64).write_volatile(0);
        }
    }
    // SAFETY: Asserted identity offset lies within the same exclusive slot.
    unsafe {
        ((base() + STRIDE + CPU_ID) as *mut u64).write_volatile(1);
    }
}

/// Caller holds BUSY and observed Acquire Returned plus PSCI OFF.
#[cfg(feature = "secondary-tests")]
// SAFETY: Every peer Rust frame ended and hardware confirms no CPU executes;
// restore inactive-slot canaries before releasing the session lease.
pub(super) unsafe fn restore_secondary() {
    for word in 0..STRIDE / 8 {
        // SAFETY: Exact slot1 bounds; no live peer reader/writer after OFF.
        unsafe {
            ((base() + STRIDE + word * 8) as *mut u64).write_volatile(CANARY);
        }
    }
}

#[cfg(feature = "secondary-tests")]
pub(super) fn secondary_identity() -> bool {
    masked(|pointer, mpidr| {
        if mpidr & AFFINITY_MASK != 1 || pointer != base() + STRIDE {
            return false;
        }
        // SAFETY: Only CPU1 reads its own validated record, with IRQ masked;
        // CPU0 cannot touch this slot between preparation and Returned+OFF.
        unsafe { ((pointer + CPU_ID) as *const u64).read_volatile() == 1 }
    })
}
