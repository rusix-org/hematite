#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(runner: &hm_threads::SecondaryRunner) {
        let _ = <hm_threads::SecondaryRunner as Clone>::clone(runner);
    }
