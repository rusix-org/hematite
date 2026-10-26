#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &[u8]) {
        let image = Image::parse(input).unwrap();
        let segment = image.segments()[0];
        let _ = Image { entry: 0, segments: [segment; MAX_LOAD_SEGMENTS], count: 1, pages: 1 };
    }
