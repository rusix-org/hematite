#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(event: &hm_kernel::wait_api::Event) {
        event.sequence.store(0, core::sync::atomic::Ordering::Relaxed);
    }
