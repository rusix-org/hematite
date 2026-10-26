#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

use core::panic::PanicInfo;
use hm_kernel::Platform;

#[cfg(target_arch = "aarch64")]
mod frames;

#[cfg(target_arch = "x86_64")]
#[path = "x86_64.rs"]
mod arch;
#[cfg(target_arch = "aarch64")]
#[path = "aarch64.rs"]
mod arch;

extern "C" fn boot_main(_boot_info: u64) -> ! {
    let mut platform = arch::init();
    platform.write(arch::BANNER);
    hm_kernel::report_memory(&mut platform);
    #[cfg(feature = "boot-tests")]
    arch::exit(hm_kernel::run_tests(&mut platform));
    #[cfg(not(feature = "boot-tests"))]
    hm_kernel::run_monitor(&mut platform)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    arch::exit(false)
}
