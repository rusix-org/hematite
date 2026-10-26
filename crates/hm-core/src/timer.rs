//! Scoped ARM timer IRQ delivery; Rust always executes with DAIF masked.
use core::arch::global_asm;
use hm_kernel::TimerChecks;

// SAFETY: Design 0009 confines IRQ delivery to an ABI-preserving assembly
// window; the handler touches only private timer/GIC state and its own stack.
global_asm!(include_str!("timer-aarch64.S"));

// SAFETY: Fixed no-argument assembly probes preserve AAPCS64, both stacks and
// DAIF. They access only boot-mapped GIC registers and private image storage.
unsafe extern "C" {
    fn arm_timer_probe() -> u64;
    fn arm_timer_timeout_probe() -> u64;
}

pub(super) fn checks() -> TimerChecks {
    // SAFETY: Boot established the reviewed EL1, one-CPU GICv2 profile and
    // mapped controller pages. All Rust execution is masked. Scoped IRQs never
    // touch live Rust borrows, allocation/mapping state or invoke callbacks.
    let (normal, timeout) = unsafe { (arm_timer_probe(), arm_timer_timeout_probe()) };
    TimerChecks {
        delivery: normal & 1 != 0,
        context: normal & 2 != 0 && timeout & 2 != 0,
        quiescent: normal & 4 != 0 && timeout & 4 != 0,
        timeout: timeout & 8 != 0 && timeout & 1 == 0,
    }
}
