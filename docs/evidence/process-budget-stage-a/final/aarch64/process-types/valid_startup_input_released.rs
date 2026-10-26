#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client<'p>(e: &mut Executor, space: Space<'p>) -> Option<Process<'p>> {
        let mut source = [7u8; 256];
        let input = hm_kernel::startup_input::Input::from_bytes(&source).ok()?;
        source.fill(0);
        let process = e.prepare_with_input(space, true, 1, &input).ok()?;
        drop(input); Some(process)
    }
