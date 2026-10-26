#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool, Block, KernelBox};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::heap_api::OwnedBlock;
pub fn client(p: &Pool) { let mut b = p.allocate_heap(8193,8192).unwrap(); b.bytes_mut()[8192]=7; let mut v=p.box_value(17u64).unwrap(); *v+=1; }
