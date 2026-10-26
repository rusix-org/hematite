#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client() {
        let _ = Runner { cpu_id: Some(0), _single_cpu: core::marker::PhantomData };
    }
