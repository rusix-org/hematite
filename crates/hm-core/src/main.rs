#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

use core::panic::PanicInfo;
use hm_kernel::Platform;

#[cfg(all(target_arch = "aarch64", feature = "smp-tests"))]
#[cfg_attr(
    any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test",
        feature = "smp-no-peer-timer-test",
        feature = "smp-peer-overflow-test",
        feature = "smp-all-blocked-test"
    ),
    allow(dead_code)
)]
mod allocation_checks;
#[cfg(target_arch = "aarch64")]
mod cpu_local;
mod mutex;
#[cfg(target_arch = "aarch64")]
mod mutex_checks;
#[cfg(all(target_arch = "aarch64", feature = "secondary-tests"))]
#[cfg_attr(
    any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod secondary;
#[cfg(all(target_arch = "aarch64", feature = "smp-tests"))]
#[cfg_attr(
    any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test",
        feature = "smp-no-peer-timer-test",
        feature = "smp-peer-overflow-test",
        feature = "smp-all-blocked-test"
    ),
    allow(dead_code)
)]
mod smp_checks;

#[cfg(all(target_arch = "aarch64", feature = "idle-tests"))]
#[allow(dead_code)] // Earlier diagnostic features take precedence in combined builds.
mod idle;
#[cfg(all(target_arch = "aarch64", feature = "idle-tests"))]
#[allow(dead_code)] // Earlier diagnostic features take precedence in combined builds.
mod idle_checks;
#[cfg(all(target_arch = "aarch64", feature = "rw-copy-tests"))]
#[allow(dead_code)] // Earlier diagnostic feature routes take precedence in combined builds.
mod rw_copy_checks;
#[cfg(all(target_arch = "aarch64", feature = "space-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod space_checks;
#[cfg(target_arch = "aarch64")]
#[cfg_attr(
    any(
        not(feature = "space-tests"),
        feature = "process-tests",
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod user_space;

#[cfg(target_arch = "aarch64")]
mod launcher;
#[cfg(target_arch = "aarch64")]
#[cfg_attr(
    any(
        not(feature = "process-tests"),
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod process;
#[cfg(all(target_arch = "aarch64", feature = "process-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod process_checks;
#[cfg(target_arch = "aarch64")]
#[allow(dead_code)]
mod process_group;
#[cfg(all(target_arch = "aarch64", feature = "process-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod process_inputs;
#[cfg(all(target_arch = "aarch64", feature = "runtime-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "startup-input-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod runtime_checks;
#[cfg(all(target_arch = "aarch64", feature = "runtime-tests"))]
mod runtime_inputs;
#[cfg(all(target_arch = "aarch64", feature = "task-tests"))]
#[cfg_attr(
    any(
        feature = "runtime-tests",
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "startup-input-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod task_checks;
#[cfg(all(target_arch = "aarch64", feature = "task-tests"))]
mod task_inputs;
#[cfg(all(target_arch = "aarch64", feature = "task-negative-tests"))]
#[cfg_attr(
    any(
        feature = "task-tests",
        feature = "runtime-tests",
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "startup-input-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod task_negative_checks;

#[cfg(all(target_arch = "aarch64", feature = "startup-input-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod startup_input_checks;
#[cfg(all(target_arch = "aarch64", feature = "startup-input-tests"))]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
mod startup_input_inputs;

#[cfg(target_arch = "aarch64")]
mod exceptions;
#[cfg(target_arch = "aarch64")]
mod frames;
#[cfg(target_arch = "aarch64")]
mod mappings;
#[cfg(target_arch = "aarch64")]
mod preempt;
#[cfg(target_arch = "aarch64")]
mod preempt_checks;
#[cfg(target_arch = "aarch64")]
mod threads;
#[cfg(target_arch = "aarch64")]
mod timer;

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
    #[cfg(all(target_arch = "aarch64", feature = "nested-fault-test"))]
    exceptions::nested();
    #[cfg(all(
        target_arch = "aarch64",
        feature = "unhandled-fault-test",
        not(feature = "nested-fault-test")
    ))]
    exceptions::unexpected();
    #[cfg(all(
        target_arch = "aarch64",
        feature = "thread-overflow-test",
        not(any(feature = "nested-fault-test", feature = "unhandled-fault-test"))
    ))]
    arch::thread_overflow(&platform);
    #[cfg(all(
        target_arch = "aarch64",
        feature = "preemption-overflow-test",
        not(any(
            feature = "nested-fault-test",
            feature = "unhandled-fault-test",
            feature = "thread-overflow-test"
        ))
    ))]
    {
        platform.write("HMPREEMPT EXPECT OVERFLOW\n");
        arch::preemption_overflow(&platform);
    }
    #[cfg(all(
        target_arch = "aarch64",
        feature = "wait-all-blocked-test",
        not(any(
            feature = "nested-fault-test",
            feature = "unhandled-fault-test",
            feature = "thread-overflow-test",
            feature = "preemption-overflow-test"
        ))
    ))]
    arch::wait_all_blocked(&platform);
    #[cfg(not(all(
        target_arch = "aarch64",
        any(
            feature = "unhandled-fault-test",
            feature = "nested-fault-test",
            feature = "thread-overflow-test",
            feature = "preemption-overflow-test",
            feature = "wait-all-blocked-test"
        )
    )))]
    {
        #[cfg(all(target_arch = "aarch64", feature = "smp-tests"))]
        {
            if !arch::smp_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "idle-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests",
                feature = "runtime-tests",
                feature = "task-tests",
                feature = "task-negative-tests",
                feature = "space-tests",
                feature = "rw-copy-tests"
            ))
        ))]
        {
            if !arch::idle_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "secondary-tests",
            not(feature = "smp-tests")
        ))]
        {
            if !arch::secondary_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "process-tests",
            not(feature = "secondary-tests")
        ))]
        {
            if !arch::process_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "startup-input-tests",
            not(any(feature = "secondary-tests", feature = "process-tests"))
        ))]
        {
            if !arch::startup_input_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "runtime-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests"
            ))
        ))]
        {
            if !arch::runtime_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "task-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests",
                feature = "runtime-tests"
            ))
        ))]
        {
            if !arch::task_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "task-negative-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests",
                feature = "runtime-tests",
                feature = "task-tests"
            ))
        ))]
        {
            if !arch::task_negative_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "space-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests",
                feature = "runtime-tests",
                feature = "task-tests",
                feature = "task-negative-tests"
            ))
        ))]
        {
            if !arch::space_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            target_arch = "aarch64",
            feature = "rw-copy-tests",
            not(any(
                feature = "secondary-tests",
                feature = "process-tests",
                feature = "startup-input-tests",
                feature = "runtime-tests",
                feature = "task-tests",
                feature = "task-negative-tests",
                feature = "space-tests"
            ))
        ))]
        {
            if !arch::rw_copy_tests(&mut platform) {
                arch::exit(false);
            }
            arch::exit(hm_kernel::run_tests(&mut platform));
        }
        #[cfg(all(
            feature = "preemption-no-timer-test",
            not(all(
                target_arch = "aarch64",
                any(
                    feature = "secondary-tests",
                    feature = "space-tests",
                    feature = "process-tests",
                    feature = "startup-input-tests",
                    feature = "runtime-tests",
                    feature = "task-tests",
                    feature = "task-negative-tests",
                    feature = "rw-copy-tests",
                    feature = "idle-tests"
                )
            ))
        ))]
        platform.write("HMPREEMPT EXPECT NO-TIMER\n");
        #[cfg(all(
            feature = "preemption-tests",
            not(all(
                target_arch = "aarch64",
                any(
                    feature = "secondary-tests",
                    feature = "space-tests",
                    feature = "process-tests",
                    feature = "startup-input-tests",
                    feature = "runtime-tests",
                    feature = "task-tests",
                    feature = "task-negative-tests",
                    feature = "rw-copy-tests",
                    feature = "idle-tests"
                )
            ))
        ))]
        {
            let scheduling = hm_kernel::run_preemption_tests(&mut platform);
            let regression = hm_kernel::run_tests(&mut platform);
            arch::exit(scheduling && regression);
        }
        #[cfg(all(
            feature = "boot-tests",
            not(feature = "preemption-tests"),
            not(all(
                target_arch = "aarch64",
                any(
                    feature = "secondary-tests",
                    feature = "space-tests",
                    feature = "process-tests",
                    feature = "startup-input-tests",
                    feature = "runtime-tests",
                    feature = "task-tests",
                    feature = "task-negative-tests",
                    feature = "rw-copy-tests",
                    feature = "idle-tests"
                )
            ))
        ))]
        arch::exit(hm_kernel::run_tests(&mut platform));
        #[cfg(not(any(
            feature = "boot-tests",
            feature = "preemption-tests",
            all(
                target_arch = "aarch64",
                any(
                    feature = "secondary-tests",
                    feature = "space-tests",
                    feature = "process-tests",
                    feature = "startup-input-tests",
                    feature = "runtime-tests",
                    feature = "task-tests",
                    feature = "task-negative-tests",
                    feature = "rw-copy-tests",
                    feature = "idle-tests"
                )
            )
        )))]
        hm_kernel::run_monitor(&mut platform)
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    arch::exit(false)
}
