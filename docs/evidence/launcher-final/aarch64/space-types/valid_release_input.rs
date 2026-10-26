#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client<'p>(factory: &Factory<'p>) -> Option<Space<'p>> {
        let mut input = [0u8; 8192];
        let image = Image::parse(&input).ok()?;
        let space = factory.load(&image, 284).ok()?;
        drop(image); input.fill(0); Some(space)
    }
