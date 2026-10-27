#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(done: hm_process::TaskCompletion) { let _ = hm_process::TaskCompletion { ..done }; }
