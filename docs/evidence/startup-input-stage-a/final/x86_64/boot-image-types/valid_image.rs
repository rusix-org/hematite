#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
pub fn client(input: &[u8]) -> Result<Image<'_>, AdmissionError> { admit(input, "bin/hello") }
