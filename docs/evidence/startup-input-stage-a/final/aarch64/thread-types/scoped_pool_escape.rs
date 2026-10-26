#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client() -> Option<&'static hm_threads::Pool> {
        hm_threads::with_pool(|pool| pool)
    }
