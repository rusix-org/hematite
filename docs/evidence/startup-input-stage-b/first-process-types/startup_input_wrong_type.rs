#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, space: Space<'_>) {
        let _ = e.prepare_with_input(space, true, 1, b"unchecked");
    }
