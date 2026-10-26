#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client(space: Space<'_>, session: hm_kernel::native::Session) {
        let _ = Process { space, session, context: [0;106], handle: 1 };
    }
