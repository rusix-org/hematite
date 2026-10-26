#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(m: &Mapper, f: Frame<'_>) {
        let _ = MappedPage { mapper: m, slot: 0, frame: core::mem::ManuallyDrop::new(f) };
    }
