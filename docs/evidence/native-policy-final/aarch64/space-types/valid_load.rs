#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client<'p>(factory: &Factory<'p>, input: &[u8]) -> Option<Space<'p>> {
        factory.load(&Image::parse(input).ok()?, 284).ok()
    }
