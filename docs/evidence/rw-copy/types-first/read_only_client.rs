#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(space: &Space<'_>) -> u64 { space.entry() }
