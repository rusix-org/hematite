#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client(extension: &[u8]) -> &[u8] {
        let view = decode(extension).unwrap(); let copied = view;
        let _ = view.is_present(); copied.bytes()
    }
