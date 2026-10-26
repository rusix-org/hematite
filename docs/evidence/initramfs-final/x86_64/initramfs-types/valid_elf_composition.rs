#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(input: &[u8]) -> Option<hm_kernel::elf::Image<'_>> {
        let archive = Archive::parse(input).ok()?;
        let entry = archive.get("bin/hello")?;
        if entry.kind() != Kind::File { return None; }
        hm_kernel::elf::Image::parse(entry.bytes()).ok()
    }
