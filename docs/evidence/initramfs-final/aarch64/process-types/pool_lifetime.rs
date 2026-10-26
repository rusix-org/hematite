#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, make: for<'p> fn(&'p Pool) -> Factory<'p>, pool: Pool, input: &[u8]) -> Option<Process<'static>> {
        let factory = make(&pool); let space = factory.load(&Image::parse(input).ok()?, 284).ok()?;
        e.prepare(space, true, 1).ok()
    }
