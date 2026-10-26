//! Byte fixtures exercise the same parser used by the eventual loader.
#![forbid(unsafe_code)]
use crate::elf::{
    Image, MAX_IMAGE_PAGES, PAGE_SIZE, Permissions, USER_IMAGE_END, USER_IMAGE_START as BASE,
};
fn p16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn p32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn p64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn load(
    bytes: &mut [u8],
    at: usize,
    offset: u64,
    address: u64,
    file: u64,
    memory: u64,
    flags: u32,
) {
    p32(bytes, at, 1);
    p32(bytes, at + 4, flags);
    p64(bytes, at + 8, offset);
    p64(bytes, at + 16, address);
    p64(bytes, at + 24, u64::MAX);
    p64(bytes, at + 32, file);
    p64(bytes, at + 40, memory);
    p64(bytes, at + 48, 4096);
}
fn fixture() -> [u8; 512] {
    let mut bytes = [0; 512];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(&mut bytes, 16, 2);
    p16(&mut bytes, 18, 183);
    p32(&mut bytes, 20, 1);
    p64(&mut bytes, 24, BASE + 0x100);
    p64(&mut bytes, 32, 64);
    p16(&mut bytes, 52, 64);
    p16(&mut bytes, 54, 56);
    p16(&mut bytes, 56, 2);
    load(&mut bytes, 64, 0x100, BASE + 0x100, 32, 32, 5);
    load(&mut bytes, 120, 0x180, BASE + 0x2180, 16, 4097, 6);
    bytes[0x100..0x120].fill(0xa5);
    bytes[0x180..0x190].fill(0x5a);
    bytes
}
#[inline(never)]
pub fn valid() -> bool {
    let bytes = fixture();
    let Ok(image) = Image::parse(&bytes) else {
        return false;
    };
    if image.entry() != BASE + 0x100 || image.page_count() != 3 || image.segments().len() != 2 {
        return false;
    }
    let code = image.segments()[0];
    let data = image.segments()[1];
    if code.file_bytes() != [0xa5; 32]
        || code.permissions() != Permissions::ReadExecute
        || code.virtual_address() != BASE + 0x100
        || code.memory_size() != 32
        || code.page_start() != BASE
        || code.page_end() != BASE + 4096
        || data.file_bytes() != [0x5a; 16]
        || data.permissions() != Permissions::ReadWrite
        || data.memory_size() != 4097
        || data.page_start() != BASE + 0x2000
        || data.page_end() != BASE + 0x4000
    {
        return false;
    }
    for alignment in [0, 1, 4096, 8192] {
        let mut variant = fixture();
        p64(&mut variant, 112, alignment);
        p64(&mut variant, 168, alignment);
        if Image::parse(&variant).is_err() {
            return false;
        }
    }
    let mut bss = fixture();
    p64(&mut bss, 152, 0);
    if !Image::parse(&bss).is_ok_and(|i| {
        i.segments()[1].file_bytes().is_empty() && i.segments()[1].memory_size() == 4097
    }) {
        return false;
    }
    let mut readonly = fixture();
    p32(&mut readonly, 124, 4);
    if !Image::parse(&readonly)
        .is_ok_and(|i| i.segments()[1].permissions() == Permissions::ReadOnly)
    {
        return false;
    }
    let mut sections = fixture();
    p64(&mut sections, 40, u64::MAX);
    p16(&mut sections, 58, u16::MAX);
    p16(&mut sections, 60, u16::MAX);
    p16(&mut sections, 62, u16::MAX);
    if Image::parse(&sections).is_err() {
        return false;
    }
    let mut maximum = fixture();
    p16(&mut maximum, 56, 1);
    p64(&mut maximum, 104, USER_IMAGE_END - BASE - 0x100);
    if !Image::parse(&maximum).is_ok_and(|i| {
        i.page_count() == MAX_IMAGE_PAGES && i.segments()[0].page_end() == USER_IMAGE_END
    }) {
        return false;
    }
    let mut many = [0u8; 1024];
    many[..512].copy_from_slice(&fixture());
    p16(&mut many, 56, 16);
    p64(&mut many, 24, BASE + 960);
    for index in 0..8 {
        if index == 0 {
            load(&mut many, 64, 960, BASE + 960, 16, 16, 5);
        } else {
            load(
                &mut many,
                64 + index * 56,
                0,
                BASE + (index * 2 * PAGE_SIZE) as u64,
                0,
                4096,
                4,
            );
        }
    }
    // Unused headers have deliberately nonsensical payloads, but no type.
    for index in 8..16 {
        let start = 64 + index * 56;
        many[start..start + 56].fill(0xff);
        p32(&mut many, start, 0);
    }
    if !Image::parse(&many).is_ok_and(|i| i.segments().len() == 8 && i.page_count() == 8) {
        return false;
    }
    let stack = 64 + 8 * 56;
    many[stack..stack + 56].fill(0);
    p32(&mut many, stack, 0x6474_e551);
    p32(&mut many, stack + 4, 6);
    p64(&mut many, stack + 48, 16);
    Image::parse(&many).is_ok()
}
#[inline(never)]
pub fn malformed() -> bool {
    let original = fixture();
    for length in 0..0x190 {
        if Image::parse(&original[..length]).is_ok() {
            return false;
        }
    }
    for (at, value) in [
        (0, 0),
        (4, 1),
        (5, 2),
        (6, 0),
        (7, 3),
        (8, 1),
        (9, 1),
        (15, 1),
    ] {
        let mut b = original;
        b[at] = value;
        if Image::parse(&b).is_ok() {
            return false;
        }
    }
    for (at, value) in [
        (16, 3),
        (18, 62),
        (52, 63),
        (54, 55),
        (56, 0),
        (56, 17),
        (56, 65535),
    ] {
        let mut b = original;
        p16(&mut b, at, value);
        if Image::parse(&b).is_ok() {
            return false;
        }
    }
    for (at, value) in [
        (20, 0),
        (48, 1),
        (64, 2),
        (64, 3),
        (64, 6),
        (64, 7),
        (64, u32::MAX),
        (68, 0),
        (68, 1),
        (68, 2),
        (68, 3),
        (68, 7),
        (68, 0x80000005),
    ] {
        let mut b = original;
        p32(&mut b, at, value);
        if Image::parse(&b).is_ok() {
            return false;
        }
    }
    for (at, value) in [
        (32, 0),
        (32, 63),
        (32, u64::MAX),
        (72, u64::MAX),
        (72, 0x101),
        (96, 33),
        (96, u64::MAX),
        (104, 0),
        (104, u64::MAX),
        (80, 0x100),
        (80, USER_IMAGE_END + 0x100),
        (80, u64::MAX - 255),
        (112, 3),
        (112, 0x2_0000_0000),
        (136, BASE + 0x180),
        (152, u64::MAX),
        (160, u64::MAX),
        (24, BASE + 0x101),
        (24, BASE + 0x11c + 4),
        (24, BASE + 0x2180),
        (24, u64::MAX - 3),
    ] {
        let mut b = original;
        p64(&mut b, at, value);
        if Image::parse(&b).is_ok() {
            return false;
        }
    }
    let mut file_overflow = original;
    p64(&mut file_overflow, 96, u64::MAX);
    p64(&mut file_overflow, 104, u64::MAX);
    if Image::parse(&file_overflow).is_ok() {
        return false;
    }
    let mut partial_instruction = original;
    p64(&mut partial_instruction, 96, 31);
    p64(&mut partial_instruction, 24, BASE + 0x11c);
    if Image::parse(&partial_instruction).is_ok() {
        return false;
    }
    let mut descending = original;
    let first: [u8; 56] = original[64..120].try_into().unwrap_or([0; 56]);
    descending[64..120].copy_from_slice(&original[120..176]);
    descending[120..176].copy_from_slice(&first);
    if Image::parse(&descending).is_ok() {
        return false;
    }
    let mut no_load = original;
    p32(&mut no_load, 64, 0);
    p32(&mut no_load, 120, 0);
    if Image::parse(&no_load).is_ok() {
        return false;
    }
    let mut entry_bss = original;
    p64(&mut entry_bss, 104, 64);
    p64(&mut entry_bss, 24, BASE + 0x120);
    if Image::parse(&entry_bss).is_ok() {
        return false;
    }
    let mut too_many = [0; 1024];
    too_many[..512].copy_from_slice(&original);
    p16(&mut too_many, 56, 9);
    p64(&mut too_many, 24, BASE + 960);
    for index in 0..9 {
        if index == 0 {
            load(&mut too_many, 64, 960, BASE + 960, 16, 16, 5);
        } else {
            load(
                &mut too_many,
                64 + index * 56,
                0,
                BASE + (index * 2 * PAGE_SIZE) as u64,
                0,
                4096,
                4,
            );
        }
    }
    if Image::parse(&too_many).is_ok() {
        return false;
    }
    for (flags, file, memory, align) in [(7, 0, 0, 16), (6, 1, 0, 16), (6, 0, 1, 16), (6, 0, 0, 3)]
    {
        let mut b = original;
        p16(&mut b, 56, 3);
        p32(&mut b, 176, 0x6474_e551);
        p32(&mut b, 180, flags);
        p64(&mut b, 208, file);
        p64(&mut b, 216, memory);
        p64(&mut b, 224, align);
        if Image::parse(&b).is_ok() {
            return false;
        }
    }
    let mut duplicate = original;
    p16(&mut duplicate, 56, 4);
    for start in [176, 232] {
        p32(&mut duplicate, start, 0x6474_e551);
        p32(&mut duplicate, start + 4, 6);
    }
    Image::parse(&duplicate).is_err()
}
