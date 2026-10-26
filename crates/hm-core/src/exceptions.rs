//! Fixed-site synchronous recovery; no arbitrary-address safe memory API.
use core::arch::global_asm;
use hm_kernel::FaultChecks;

// SAFETY: Reviewed design 0007 binds vector entry, separately owned stacks,
// exact fault sites and ABI-preserving probes. No dynamic recovery entries.
global_asm!(include_str!("exceptions-aarch64.S"));

// SAFETY: These no-argument assembly functions follow AAPCS64 and touch only
// fixed image-owned probes. All Rust entry follows the configured EL1 boot.
unsafe extern "C" {
    fn arm_probe_null() -> u64;
    fn arm_probe_stack_guard() -> u64;
    fn arm_probe_exception_guard() -> u64;
    fn arm_probe_read_only() -> u64;
    fn arm_probe_no_execute() -> u64;
    fn arm_probe_context() -> u64;
    #[cfg(all(feature = "unhandled-fault-test", not(feature = "nested-fault-test")))]
    fn arm_probe_unexpected() -> !;
    #[cfg(feature = "nested-fault-test")]
    fn arm_probe_nested() -> !;
}

pub(super) fn checks() -> FaultChecks {
    let mut checks = FaultChecks {
        null_read: true,
        stack_guard: true,
        exception_guard: true,
        read_only: true,
        no_execute: true,
        context: true,
    };
    // Repeat every probe to check that emergency stack ownership is reusable.
    for _ in 0..2 {
        // SAFETY: Boot established EL1h, masked DAIF, FP access, both owned
        // stacks, whitelist vectors and fixed mappings before reaching Rust.
        // No caller supplies addresses; each probe restores its public ABI.
        unsafe {
            checks.null_read &= arm_probe_null() == 1;
            checks.stack_guard &= arm_probe_stack_guard() == 1;
            checks.exception_guard &= arm_probe_exception_guard() == 1;
            checks.read_only &= arm_probe_read_only() == 1;
            checks.no_execute &= arm_probe_no_execute() == 1;
            checks.context &= arm_probe_context() == 1;
        }
    }
    checks
}

#[cfg(all(feature = "unhandled-fault-test", not(feature = "nested-fault-test")))]
pub(super) fn unexpected() -> ! {
    // SAFETY: Dedicated negative image enters nonreturning assembly after boot.
    // It corrupts only its own normal SP and can never return to Rust.
    unsafe { arm_probe_unexpected() }
}

#[cfg(feature = "nested-fault-test")]
pub(super) fn nested() -> ! {
    // SAFETY: Dedicated negative image enters nonreturning assembly after boot.
    // It corrupts only its emergency SP and can never return to Rust.
    unsafe { arm_probe_nested() }
}
