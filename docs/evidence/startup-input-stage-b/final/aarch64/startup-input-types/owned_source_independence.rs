#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client() -> Input {
        let mut source = [7u8; MAX_INPUT_SIZE];
        let input = Input::from_bytes(&source).unwrap();
        source.fill(9); drop(source); input
    }
