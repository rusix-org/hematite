#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::elf::{Error, Image, Permissions, Segment, MAX_LOAD_SEGMENTS};
pub fn client(input: &[u8]) {
        let _ = Segment { virtual_address: 0, memory_size: 1, file_bytes: input,
            permissions: Permissions::ReadExecute, page_start: 0, page_end: 4096 };
    }
