#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client(extension: &mut [u8]) -> usize {
        let view = decode(extension).unwrap(); extension[0] = 0; view.bytes().len()
    }
