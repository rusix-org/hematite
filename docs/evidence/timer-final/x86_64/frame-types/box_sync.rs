#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool, Block, KernelBox};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::heap_api::OwnedBlock;
fn sync<T: Sync>() {} pub fn client() { sync::<KernelBox<'static,u64>>(); }
