#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, space: Space<'_>) -> Option<RunReport> {
        let process = e.prepare(space, true, 1).ok()?;
        let _ = (process.entry(), process.root_address(), process.frame_count(), process.handle());
        e.run(process).ok()
    }
