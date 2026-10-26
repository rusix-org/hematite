#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
fn send<T: Send>() {} pub fn client() { send::<Space<'static>>(); }
