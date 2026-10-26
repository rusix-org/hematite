#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn sync<T: Sync>() {} pub fn client() {
        sync::<hm_threads::SecondaryRunner>();
    }
