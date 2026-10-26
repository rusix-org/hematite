#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(input: &[u8]) -> Option<Entry<'_>> {
        let archive = Archive::parse(input).ok()?;
        let entry = archive.get("bin/hello")?; drop(archive); Some(entry)
    }
