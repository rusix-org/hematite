#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client() -> Archive<'static> {
        let bytes = [0u8; 128]; Archive::parse(&bytes).unwrap()
    }
