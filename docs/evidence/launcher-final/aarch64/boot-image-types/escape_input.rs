#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
pub fn client() -> Image<'static> { let bytes = [0u8;128]; admit(&bytes, "bin/hello").unwrap() }
