#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
fn sync<T: Sync>() {} pub fn client() { sync::<Pool>(); }
