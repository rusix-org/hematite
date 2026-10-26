#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
pub fn client(pool: &Pool) {
        let _ = Factory { pool, cpus: 1, _local: core::marker::PhantomData };
    }
