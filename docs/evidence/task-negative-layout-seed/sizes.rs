#![no_std]
#![forbid(unsafe_code)]
use hm_layout::{Process,Space,KernelBox,TASK_LAYOUT};
#[used]
pub static LAYOUT: [usize;11] = [
    core::mem::size_of::<Process<'static>>(),
    core::mem::size_of::<Space<'static>>(),
    core::mem::size_of::<KernelBox<'static,Process<'static>>>(),
    core::mem::size_of::<Option<KernelBox<'static,Process<'static>>>>(),
    core::mem::size_of::<Process<'static>>().div_ceil(4096),
    TASK_LAYOUT[0], TASK_LAYOUT[1], TASK_LAYOUT[2],
    TASK_LAYOUT[3], TASK_LAYOUT[4], TASK_LAYOUT[5],
];
