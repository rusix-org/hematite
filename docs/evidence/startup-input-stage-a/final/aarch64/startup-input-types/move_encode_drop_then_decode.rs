#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client<'a>(source: &[u8], destination: &'a mut [u8]) -> View<'a> {
        let input = Input::from_bytes(source).unwrap(); let moved = input;
        moved.encode_into(destination).unwrap(); drop(moved); decode(destination).unwrap()
    }
