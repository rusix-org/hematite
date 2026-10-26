#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
pub fn client(m: &Mapper, mut f: Frame<'_>) -> bool {
        f.bytes_mut()[0] = 31;
        let Ok(mut page) = m.map(f) else { return false; };
        let observed = page.bytes()[0] == 31;
        page.bytes_mut()[0] = 47;
        let f = page.unmap();
        observed && f.bytes()[0] == 47
    }
