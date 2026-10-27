#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &'static mut Executor, pool: Pool, make: for<'p> fn(&'p Pool)->Factory<'p>, image: &Image<'_>, archive: &'static [u8]) -> Option<Group<'static, 'static>> {
        hm_process::task_group(e, make(&pool), image, archive).ok()
    }
