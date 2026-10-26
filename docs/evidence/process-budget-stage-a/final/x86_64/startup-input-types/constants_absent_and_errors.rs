#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
pub fn client(destination: &mut [u8]) -> Result<(), Error> {
        let input = Input::absent(); let _ = input.as_bytes();
        let _ = [MAX_INPUT_SIZE, EXTENSION_OFFSET, EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET];
        let _ = [Error::Length, Error::Size, Error::Magic, Error::Version, Error::Header, Error::Reserved, Error::Padding];
        input.encode_into(destination)
    }
