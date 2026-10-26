#![no_std]
#![forbid(unsafe_code)]
use hm_layout::{Process,Space,KernelBox};
#[used]
pub static LAYOUT: [usize;5] = [
    core::mem::size_of::<Process<'static>>(),
    core::mem::size_of::<Space<'static>>(),
    core::mem::size_of::<KernelBox<'static,Process<'static>>>(),
    core::mem::size_of::<Option<KernelBox<'static,Process<'static>>>>(),
    core::mem::size_of::<Process<'static>>().div_ceil(4096),
];
