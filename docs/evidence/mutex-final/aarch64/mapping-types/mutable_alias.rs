#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(mut p: MappedPage<'_, '_>) {
        let a = p.bytes_mut(); let b = p.bytes_mut(); a[0] = b[0];
    }
