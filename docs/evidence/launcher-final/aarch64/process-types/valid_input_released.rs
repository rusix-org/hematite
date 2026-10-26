#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client<'p>(e: &mut Executor, f: &Factory<'p>) -> Option<Process<'p>> {
        let mut input = [0u8; 8192]; let image = Image::parse(&input).ok()?;
        let space = f.load(&image, 284).ok()?; drop(image); input.fill(0);
        e.prepare(space, true, 1).ok()
    }
