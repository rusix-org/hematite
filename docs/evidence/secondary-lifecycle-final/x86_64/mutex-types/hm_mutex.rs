#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#[path="/workspace/crates/hm-core/src/mutex.rs"] pub mod mutex;
pub use mutex::{Mutex, Guard};
