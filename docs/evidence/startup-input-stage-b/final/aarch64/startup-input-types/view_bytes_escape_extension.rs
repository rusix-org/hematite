#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client() -> &'static [u8] {
        let extension = [0u8; EXTENSION_SIZE]; decode(&extension).unwrap().bytes()
    }
