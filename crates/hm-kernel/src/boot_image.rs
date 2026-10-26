//! Checked archive-to-ELF composition; no physical allocation or execution.
#![forbid(unsafe_code)]

use crate::{elf, initramfs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    Archive(initramfs::Error),
    Missing,
    Directory,
    Elf(elf::Error),
}

/// Admit the entire archive before selecting an exact regular-file path.
/// The returned image borrows only immutable archive bytes, not `path` or
/// the temporary archive plan. Malformed suffixes cannot publish an image.
pub fn admit<'a>(archive: &'a [u8], path: &str) -> Result<elf::Image<'a>, AdmissionError> {
    let archive = initramfs::Archive::parse(archive).map_err(AdmissionError::Archive)?;
    let entry = archive.get(path).ok_or(AdmissionError::Missing)?;
    if entry.kind() != initramfs::Kind::File {
        return Err(AdmissionError::Directory);
    }
    elf::Image::parse(entry.bytes()).map_err(AdmissionError::Elf)
}
