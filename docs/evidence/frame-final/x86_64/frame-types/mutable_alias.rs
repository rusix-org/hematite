#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
pub fn client(mut f: Frame<'_>) { let a = f.bytes_mut(); let b = f.bytes_mut(); a[0] = b[0]; }
