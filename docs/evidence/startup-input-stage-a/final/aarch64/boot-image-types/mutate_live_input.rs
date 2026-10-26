#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
pub fn client(input: &mut [u8]) -> u64 { let image = admit(input, "bin/hello").unwrap(); input[0] = 0; image.entry() }
