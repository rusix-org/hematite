#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client() {
        let mut escaped = None;
        hm_threads::with_pool(|pool| { escaped = Some(pool.allocate().unwrap()); });
        drop(escaped);
    }
