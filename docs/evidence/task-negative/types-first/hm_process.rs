#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#[path="/workspace/crates/hm-core/src/frames.rs"] mod frames;
#[path="/workspace/crates/hm-core/src/cpu_local.rs"] mod cpu_local;
#[path="/workspace/crates/hm-core/src/threads.rs"] mod threads;
#[path="/workspace/crates/hm-core/src/preempt.rs"] mod preempt;
#[path="/workspace/crates/hm-core/src/secondary.rs"] mod secondary;
#[path="/workspace/crates/hm-core/src/user_space.rs"] mod user_space;
#[path="/workspace/crates/hm-core/src/process.rs"] mod process;
#[path="/workspace/crates/hm-core/src/process_group.rs"] mod process_group;
pub use user_space::{Factory, Space};
pub use frames::Pool;
pub use process::{Executor, Process, RunReport, Error};
pub use process_group::{Group, TaskId, TaskCompletion};
// Compile-only bridge calls the real crate-private constructor unchanged.
pub fn task_group<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, image: &hm_kernel::elf::Image<'_>, archive: &'static [u8]) -> Result<Group<'e, 'p>, process_group::Error> {
    Group::new_task(e, f, image, archive, true)
}
