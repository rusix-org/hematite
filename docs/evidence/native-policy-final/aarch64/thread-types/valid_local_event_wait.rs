#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(context: &mut dyn ThreadContext) {
        let event = hm_kernel::wait_api::Event::new();
        let observed = event.snapshot();
        let _ = context.wait(&event, observed);
        let _ = event.notify();
    }
