#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn task(context: &mut dyn ThreadContext, input: u64) -> u64 {
        core::mem::forget(context); input
    }
    pub fn client(runner: &Runner) { let _ = runner.run(&[ThreadJob { entry: task, input: 29 }]); }
    
