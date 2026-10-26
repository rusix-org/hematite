#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn send<T: Send>() {} pub fn client() {
        send::<hm_threads::SecondaryRunner>();
    }
