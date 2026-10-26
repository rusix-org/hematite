#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(space: Space<'_>) -> [u8; 8] {
        let mut output = [0; 8]; let _ = space.read_into(space.entry(), &mut output);
        drop(space); output
    }
