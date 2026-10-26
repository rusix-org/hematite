#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(runner: &Runner) {
        let _ = <Runner as Clone>::clone(runner);
    }
