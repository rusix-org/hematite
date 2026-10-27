#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(space: &Space<'_>) { let _ = space.validate_rw(0, 1); }
