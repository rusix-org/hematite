#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool, Block, KernelBox};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::heap_api::OwnedBlock;
pub fn client(b: &KernelBox<'_,u64>) { let _ = <KernelBox<'_,u64> as Clone>::clone(b); }
