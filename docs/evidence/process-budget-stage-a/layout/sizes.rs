#![no_std]
#![forbid(unsafe_code)]
use hm_layout::{Process,Space,KernelBox};
pub fn process_size()->usize { core::mem::size_of::<Process<'static>>() }
pub fn space_size()->usize { core::mem::size_of::<Space<'static>>() }
pub fn box_size()->usize { core::mem::size_of::<KernelBox<'static,Process<'static>>>() }
pub fn slot_owner_size()->usize { core::mem::size_of::<Option<KernelBox<'static,Process<'static>>>>() }
pub fn owner_pages()->usize { core::mem::size_of::<Process<'static>>().div_ceil(4096) }
