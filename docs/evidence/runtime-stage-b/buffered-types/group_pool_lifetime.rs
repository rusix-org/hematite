#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &'static mut Executor, pool: Pool, make: for<'p> fn(&'p Pool)->Factory<'p>) -> Option<Group<'static, 'static>> {
        Group::new(e, make(&pool), 512).ok()
    }
