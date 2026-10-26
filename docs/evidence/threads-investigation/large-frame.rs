#![no_std]
#![forbid(unsafe_code)]

#[inline(never)]
pub fn large_frame(index: usize) -> u8 {
    let mut bytes = [0u8; 131072];
    core::hint::black_box(&mut bytes);
    bytes[index % bytes.len()]
}
