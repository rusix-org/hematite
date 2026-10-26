//! One immutable archive shared by runtime admission and canonical test probes.
#![forbid(unsafe_code)]
pub(super) static ARCHIVE: &[u8] = include_bytes!("../fixtures/runtime-initramfs.cpio");
