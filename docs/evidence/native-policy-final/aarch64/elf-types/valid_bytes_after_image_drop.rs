#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &[u8]) -> Result<&[u8], Error> {
        let image = Image::parse(input)?;
        let bytes = image.segments()[0].file_bytes();
        drop(image);
        Ok(bytes)
    }
