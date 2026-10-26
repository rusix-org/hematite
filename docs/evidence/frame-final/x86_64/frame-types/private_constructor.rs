#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
pub fn client(p: hm_kernel::frame_plan::Plan) { let _ = Pool::from_boot(p); }
