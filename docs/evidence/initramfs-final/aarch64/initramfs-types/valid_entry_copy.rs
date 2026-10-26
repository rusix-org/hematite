#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(entry: Entry<'_>) -> usize {
        let copied = entry; entry.bytes().len() + copied.bytes().len()
    }
