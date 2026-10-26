#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(pool: &Pool, topology: &hm_kernel::cpu_topology::Topology) {
        let _ = Factory::from_boot(pool, topology);
    }
