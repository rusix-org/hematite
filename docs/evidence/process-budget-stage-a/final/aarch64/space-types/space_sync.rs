#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
fn sync<T: Sync>() {} pub fn client() { sync::<Space<'static>>(); }
