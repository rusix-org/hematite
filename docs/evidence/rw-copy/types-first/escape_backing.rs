#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client<'a, 'p>(space: &'a mut Space<'p>) -> &'a mut [u8] { space.pages[0].as_mut().unwrap().bytes_mut() }
