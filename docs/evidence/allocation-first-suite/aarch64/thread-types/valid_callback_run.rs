#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
fn task(context: &mut dyn ThreadContext, input: u64) -> u64 {
        let id = context.id(); context.yield_now(); input.wrapping_add(id as u64)
    }
    pub fn client(runner: &Runner) -> Result<RunReport, ThreadError> {
        runner.run(&[ThreadJob { entry: task, input: 17 }])
    }
