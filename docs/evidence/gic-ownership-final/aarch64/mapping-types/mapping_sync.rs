#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
fn sync<T: Sync>() {} pub fn client() { sync::<MappedPage<'static, 'static>>(); }
