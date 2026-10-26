#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(make: for<'p> fn(&'p Pool) -> Factory<'p>, pool: Pool, input: &[u8]) -> Option<Space<'static>> {
        let factory = make(&pool); factory.load(&Image::parse(input).ok()?, 284).ok()
    }
