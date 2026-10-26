#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(m: &Mapper, f: Frame<'_>) -> u8 {
        let _page = m.map(f); f.bytes()[0]
    }
