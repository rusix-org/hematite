#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#[path="/workspace/crates/hm-core/src/frames.rs"] mod frames;
#[path="/workspace/crates/hm-core/src/cpu_local.rs"] mod cpu_local;
#[path="/workspace/crates/hm-core/src/threads.rs"] mod threads;
#[path="/workspace/crates/hm-core/src/preempt.rs"] mod preempt;
#[path="/workspace/crates/hm-core/src/secondary.rs"] mod secondary;
#[path="/workspace/crates/hm-core/src/user_space.rs"] mod user_space;
pub use user_space::{Factory, Space};
pub use frames::Pool;
fn internal_adapter(space: &mut Space<'_>, bytes: &[u8]) -> bool { space.validate_rw(0x100002000, bytes.len()).is_ok() && space.copy_into_rw(0x100002000, bytes).is_ok() }
