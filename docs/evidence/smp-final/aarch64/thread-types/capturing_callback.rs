#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
pub fn client(runner: &Runner, input: u64) {
        let _ = runner.run(&[ThreadJob { entry: |_, _| input, input: 0 }]);
    }
