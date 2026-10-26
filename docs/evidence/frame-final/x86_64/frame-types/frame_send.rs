#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
fn send<T: Send>() {} pub fn client() { send::<Frame<'static>>(); }
