#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, bytes: &mut [u8]) {
        let mut group = Group::new(e, f, 512).ok().unwrap();
        let image = Image::parse(bytes).ok().unwrap();
        let input = hm_kernel::startup_input::Input::from_bytes(b"job").ok().unwrap();
        let id = group.admit(&image, true, Some(&input)).ok().unwrap();
        drop(input); drop(image); bytes.fill(0);
        let copied = id; let _ = (id.group(), id.slot(), id.generation());
        let _ = group.info(copied); let _ = group.turn();
        let _ = group.request_cancel(id); let _ = group.completion(id); let _ = group.reap(id);
    }
