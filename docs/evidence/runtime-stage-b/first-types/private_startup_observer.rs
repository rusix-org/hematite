#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
pub fn client(p: &Process<'_>) { let _ = p.copy_startup(0, &mut [0; 64]); }
