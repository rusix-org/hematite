#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client(mut input: Input) -> usize {
        let bytes = input.as_bytes().unwrap(); input = Input::absent(); bytes.len()
    }
