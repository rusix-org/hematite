#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(input: &mut [u8]) -> usize {
        let archive = Archive::parse(input).unwrap(); let entry = archive.get("bin/hello").unwrap();
        drop(archive); input[0] = 0; entry.bytes().len()
    }
