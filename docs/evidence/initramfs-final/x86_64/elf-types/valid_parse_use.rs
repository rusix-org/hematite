#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &[u8]) -> Result<(u64, usize), Error> {
        let image = Image::parse(input)?;
        for segment in image.segments() {
            let _ = (segment.virtual_address(), segment.memory_size(),
                segment.file_bytes(), segment.page_start(), segment.page_end(),
                segment.permissions().readable(), segment.permissions().writable(),
                segment.permissions().executable());
        }
        Ok((image.entry(), image.page_count()))
    }
