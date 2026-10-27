#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>) -> Option<TaskId> {
        let mut group = Group::new(e, f, 512).ok()?;
        let id = group.admit(image, false, None).ok()?; drop(group); Some(id)
    }
