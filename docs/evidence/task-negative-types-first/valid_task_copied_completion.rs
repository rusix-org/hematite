#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(mut g: Group<'_, '_>) -> Option<hm_process::TaskCompletion> {
        fn copied<T: Copy>() {} copied::<hm_process::TaskCompletion>();
        let root = g.task_root().ok()?;
        let done = g.task_reap(root).ok()?; drop(g);
        let _ = (done.closure().root().id(), done.closure().end(), done.closure().evidence(), done.live_frames());
        Some(done)
    }
