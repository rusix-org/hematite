#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
pub fn client(image: Image<'_>) { let _ = Image { ..image }; }
