#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, pool: Pool, bytes: &[u8], make: for<'a> fn(&'a Pool)->Factory<'a>) -> Option<Process<'static>> {
        let factory = make(&pool); let space = factory.load(&Image::parse(bytes).ok()?, 284).ok()?;
        e.prepare_with_input(space, true, 1, &hm_kernel::startup_input::Input::absent()).ok()
    }
