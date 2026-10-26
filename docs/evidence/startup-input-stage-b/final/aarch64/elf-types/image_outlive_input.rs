#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client() -> Image<'static> {
        let input = [0u8; 4096];
        Image::parse(&input).unwrap()
    }
