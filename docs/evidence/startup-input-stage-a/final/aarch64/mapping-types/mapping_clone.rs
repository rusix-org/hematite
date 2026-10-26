#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(p: &MappedPage<'_, '_>) {
        let _ = <MappedPage<'_, '_> as Clone>::clone(p);
    }
