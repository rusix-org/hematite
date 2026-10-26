#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(entry: Entry<'_>) { entry.path().as_bytes()[0] = b'x'; }
