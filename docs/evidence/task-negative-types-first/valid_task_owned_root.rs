#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, bytes: &mut [u8], archive: &'static [u8]) -> Option<Group<'e, 'p>> {
        let image = Image::parse(bytes).ok()?;
        let group = hm_process::task_group(e, f, &image, archive).ok()?;
        drop(image); bytes.fill(0); Some(group)
    }
