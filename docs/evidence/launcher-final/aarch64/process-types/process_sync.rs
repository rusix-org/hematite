#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
fn sync<T: Sync>() {} pub fn client() { sync::<Process<'static>>(); }
