#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(archive: Archive<'_>) { let _ = Archive { ..archive }; }
