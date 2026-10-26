#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#[path="/workspace/crates/hm-core/src/frames.rs"] pub mod frames;
#[path="/workspace/crates/hm-core/src/mappings.rs"] pub mod mappings;
pub use frames::{Frame, Pool};
pub use mappings::{Mapper, MappedPage};
