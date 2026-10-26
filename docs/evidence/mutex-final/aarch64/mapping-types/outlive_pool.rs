#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client<'a>(m: &'a Mapper, p: Pool) -> Option<MappedPage<'a, 'a>> {
        let f = p.allocate().ok()?; m.map(f).ok()
    }
