#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client() -> Option<&'static [u8;4096]> {
        use hm_kernel::frame_api::OwnedFrame;
        hm_threads::with_pool(|pool| {
            let frame = pool.allocate().unwrap(); frame.bytes()
        })
    }
