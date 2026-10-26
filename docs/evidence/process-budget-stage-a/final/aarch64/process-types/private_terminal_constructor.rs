#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_kernel::elf::Image;
pub fn client(t: &hm_kernel::cpu_topology::Topology) { let _ = Executor::from_boot_terminal(t); }
