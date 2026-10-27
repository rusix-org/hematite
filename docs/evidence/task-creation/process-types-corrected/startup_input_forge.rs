#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, space: Space<'_>, input: hm_kernel::startup_input::Input) {
        let forged = hm_kernel::startup_input::Input { ..input };
        let _ = e.prepare_with_input(space, true, 1, &forged);
    }
