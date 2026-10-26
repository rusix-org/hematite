#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client() -> &'static [u8] {
        let input = [0u8; 4096];
        let image = Image::parse(&input).unwrap();
        image.segments()[0].file_bytes()
    }
