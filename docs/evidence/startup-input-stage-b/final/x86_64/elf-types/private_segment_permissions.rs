#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(mut segment: Segment<'_>) {
        segment.permissions = Permissions::ReadWrite;
    }
