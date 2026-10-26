#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn shared<T: Send + Sync>() {}
        pub fn client() { shared::<hm_kernel::wait_api::Event>(); }
    
