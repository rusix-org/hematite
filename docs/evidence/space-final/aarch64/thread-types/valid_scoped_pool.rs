#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(context: &mut dyn ThreadContext) -> Option<bool> {
        use hm_kernel::frame_api::OwnedFrame;
        hm_threads::with_pool(|pool| {
            let Ok(mut frame) = pool.allocate() else { return false; };
            frame.bytes_mut()[0] = 17;
            context.yield_now();
            let okay = frame.bytes()[0] == 17;
            drop(frame);
            okay
        })
    }
