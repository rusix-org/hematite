#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &'static [u8], p: Process<'_>) {
        let g = hm_process::task_group(e, f, image, archive).ok().unwrap();
        let _ = e.run(p); let _ = g.task_root();
    }
