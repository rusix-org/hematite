//! Canonical immutable task-case archives and independently generated layout witness.
#![forbid(unsafe_code)]
pub(super) static ARCHIVE: &[u8] = include_bytes!("../fixtures/task-cases-initramfs.cpio");
pub(super) static LARGE_ARCHIVE: &[u8] =
    include_bytes!("../fixtures/task-cases-large-initramfs.cpio");
// Only numeric constants emitted from the actual pinned core layout witness.
include!("../fixtures/task-cases/layout.rs");
