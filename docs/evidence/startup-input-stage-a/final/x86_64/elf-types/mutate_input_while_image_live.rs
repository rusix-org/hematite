#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &mut [u8]) -> u64 {
        let image = Image::parse(input).unwrap();
        input[0] = 0;
        image.entry()
    }
