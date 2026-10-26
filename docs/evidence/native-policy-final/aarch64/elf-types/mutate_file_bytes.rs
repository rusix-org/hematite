#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(image: &Image<'_>) {
        let bytes = image.segments()[0].file_bytes();
        bytes[0] = 0;
    }
