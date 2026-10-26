#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool, Block, KernelBox};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::heap_api::OwnedBlock;
pub fn client(b: Block<'_>) { let _=KernelBox { _block:b, value:core::ptr::null_mut::<u64>(), owns:core::marker::PhantomData }; }
