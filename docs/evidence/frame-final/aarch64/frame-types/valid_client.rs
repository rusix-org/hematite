#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
pub fn client(pool: &Pool) -> bool {
        let Ok(mut a) = pool.allocate() else { return false; };
        let Ok(mut b) = pool.allocate() else { return false; };
        a.bytes_mut()[0] = 17; b.bytes_mut()[0] = 29;
        a.bytes()[0] == 17 && b.bytes()[0] == 29
    }
