#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &'static [u8]) {
        let _ = Group::new_task(e, f, image, archive, true);
    }
