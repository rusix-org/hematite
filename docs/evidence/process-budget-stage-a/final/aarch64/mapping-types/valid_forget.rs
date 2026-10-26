#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(m: Mapper, p: Pool) {
        let Ok(f) = p.allocate() else { return; };
        let Ok(page) = m.map(f) else { return; };
        core::mem::forget(page); drop(m); drop(p);
    }
