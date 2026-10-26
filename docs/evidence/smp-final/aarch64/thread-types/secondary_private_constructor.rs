#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(topology: &hm_kernel::cpu_topology::Topology) {
        let _ = hm_threads::SecondaryRunner::from_boot(topology);
    }
