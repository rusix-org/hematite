#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(context: &mut dyn ThreadContext) {
        let first = &mut *context; let second = &mut *context;
        first.yield_now(); second.yield_now();
    }
