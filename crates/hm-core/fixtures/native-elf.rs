//! Separately linked native ELF admission fixture; never executed by this test.
#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

// SAFETY: Fixed AArch64 parse-only fixture entry. A future executor must supply
// a valid stack and mappings; this stage only builds and inspects the ELF.
global_asm!(
    ".section .text.entry,\"ax\"",
    ".balign 4",
    ".global native_start",
    "native_start:",
    "bl native_value",
    "brk #0",
    "b .",
);

// SAFETY: Unique fixed symbol consumed only by the entry above.
#[unsafe(no_mangle)]
extern "C" fn native_value() -> u64 {
    core::hint::black_box(DATA[0]).wrapping_add(u64::from(MESSAGE[3]))
}

#[used]
// SAFETY: Linker places this immutable fixture array in the R-only LOAD.
#[unsafe(link_section = ".fixture.rodata")]
static MESSAGE: [u8; 16] = *b"native ELF plan\n";

#[used]
// SAFETY: Linker places this initialized fixture array in the RW LOAD.
#[unsafe(link_section = ".fixture.data")]
static DATA: [u64; 2] = [0x1122_3344_5566_7788, 0x99aa_bbcc_ddee_ff00];

#[used]
// SAFETY: Linker retains zero-initialized storage in the RW LOAD's NOLOAD tail.
#[unsafe(link_section = ".fixture.bss")]
static mut ZERO_FILL: [u8; 8192] = [0; 8192];

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
