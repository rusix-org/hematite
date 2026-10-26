#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &[u8]) -> Result<Segment<'_>, Error> {
        let image = Image::parse(input)?;
        Ok(image.segments()[0])
    }
