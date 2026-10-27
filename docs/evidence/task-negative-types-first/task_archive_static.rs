#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &[u8]) {
        let _ = hm_process::task_group(e, f, image, archive);
    }
