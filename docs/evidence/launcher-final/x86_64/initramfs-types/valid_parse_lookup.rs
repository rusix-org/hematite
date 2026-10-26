#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
pub fn client(input: &[u8]) -> Option<usize> {
        let archive = Archive::parse(input).ok()?;
        let _ = (archive.len(), archive.is_empty());
        for entry in archive.entries() { let _ = (entry.path(), entry.kind(), entry.permissions(), entry.bytes()); }
        Some(archive.get("bin/hello")?.bytes().len())
    }
