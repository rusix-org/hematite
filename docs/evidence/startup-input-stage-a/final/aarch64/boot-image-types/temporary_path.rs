#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
pub fn client(input: &[u8]) -> Image<'_> { let path = *b"bin/hello"; admit(input, core::str::from_utf8(&path).unwrap()).unwrap() }
