#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(factory: &Factory<'_>) { let _ = <Factory<'_> as Clone>::clone(factory); }
