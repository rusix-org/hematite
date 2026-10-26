#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn store(_: &'static mut dyn ThreadContext) {}
        pub fn client(context: &mut dyn ThreadContext) { store(context); }
    
