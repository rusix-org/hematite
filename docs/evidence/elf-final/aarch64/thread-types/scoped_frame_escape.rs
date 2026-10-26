#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client() -> Option<hm_threads::Frame<'static>> {
        hm_threads::with_pool(|pool| pool.allocate().unwrap())
    }
