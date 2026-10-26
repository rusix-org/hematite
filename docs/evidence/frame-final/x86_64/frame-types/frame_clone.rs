#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
pub fn client(f: &Frame<'_>) { let _ = <Frame<'_> as Clone>::clone(f); }
