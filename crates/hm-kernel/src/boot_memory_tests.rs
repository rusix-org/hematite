//! In-guest DTB fixtures; no host/device pointers or allocation.
#![forbid(unsafe_code)]

use crate::boot_memory::{MemoryMap, Region, parse};

const STRUCTURE: usize = 608;
const STRINGS: &[u8] = b"#address-cells\0#size-cells\0device_type\0reg\0status\0ranges\0size\0alignment\0alloc-ranges\0no-map\0reusable\0compatible\0";

struct Blob {
    bytes: [u8; 4096],
    cursor: usize,
    len: usize,
    okay: bool,
}

impl Blob {
    fn new() -> Self {
        Self {
            bytes: [0; 4096],
            cursor: STRUCTURE,
            len: 0,
            okay: true,
        }
    }

    fn at32(&mut self, offset: usize, value: u32) {
        if let Some(target) = self.bytes.get_mut(offset..offset.saturating_add(4)) {
            target.copy_from_slice(&value.to_be_bytes());
        } else {
            self.okay = false;
        }
    }

    fn at64(&mut self, offset: usize, value: u64) {
        if let Some(target) = self.bytes.get_mut(offset..offset.saturating_add(8)) {
            target.copy_from_slice(&value.to_be_bytes());
        } else {
            self.okay = false;
        }
    }

    fn raw(&mut self, data: &[u8]) {
        let end = self.cursor.saturating_add(data.len());
        if let Some(target) = self.bytes.get_mut(self.cursor..end) {
            target.copy_from_slice(data);
            self.cursor = end;
        } else {
            self.okay = false;
        }
    }

    fn word(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }

    fn align(&mut self) {
        while !self.cursor.is_multiple_of(4) && self.okay {
            self.raw(&[0]);
        }
    }

    fn begin(&mut self, name: &[u8]) {
        self.word(1);
        self.raw(name);
        self.raw(&[0]);
        self.align();
    }

    fn end(&mut self) {
        self.word(2);
    }

    fn property(&mut self, name: &[u8], data: &[u8]) -> usize {
        let offset = STRINGS
            .windows(name.len() + 1)
            .position(|part| part[..name.len()] == *name && part[name.len()] == 0);
        let Some(offset) = offset else {
            self.okay = false;
            return 0;
        };
        let start = self.cursor;
        self.word(3);
        self.word(data.len() as u32);
        self.word(offset as u32);
        self.raw(data);
        self.align();
        start
    }

    fn cells(&mut self) {
        self.property(b"#address-cells", &2u32.to_be_bytes());
        self.property(b"#size-cells", &2u32.to_be_bytes());
    }

    fn root(&mut self) {
        self.begin(b"");
        self.cells();
    }

    fn region(&mut self, start: u64, size: u64) -> usize {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&start.to_be_bytes());
        bytes[8..].copy_from_slice(&size.to_be_bytes());
        self.property(b"reg", &bytes)
    }

    fn memory(&mut self) {
        self.begin(b"memory@40000000");
        self.property(b"device_type", b"memory\0");
        self.region(0x40000000, 0x08000000);
        self.end();
    }

    fn finish(&mut self) {
        self.word(9);
        let end = self.cursor;
        self.raw(STRINGS);
        self.len = self.cursor;
        self.at32(0, 0xd00dfeed);
        self.at32(4, self.len as u32);
        self.at32(8, STRUCTURE as u32);
        self.at32(12, end as u32);
        self.at32(16, 40);
        self.at32(20, 17);
        self.at32(24, 16);
        self.at32(32, STRINGS.len() as u32);
        self.at32(36, (end - STRUCTURE) as u32);
    }

    fn data(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    fn rejected(&self) -> bool {
        self.okay && parse(self.data()).is_err()
    }
}

#[inline(never)]
fn basic() -> Blob {
    let mut b = Blob::new();
    b.root();
    b.memory();
    b.end();
    b.finish();
    b
}

fn sound_map(map: &MemoryMap) -> bool {
    let mut sum = 0u64;
    for (index, range) in map.ram().iter().enumerate() {
        if range.start >= range.end {
            return false;
        }
        let Some(next) = sum.checked_add(range.end - range.start) else {
            return false;
        };
        sum = next;
        for other in &map.ram()[..index] {
            if range.start < other.end && other.start < range.end {
                return false;
            }
        }
    }
    !map.ram().is_empty()
        && map.ram().len() <= 16
        && map.reservations().len() <= 32
        && map.reservations().iter().all(|r| r.start < r.end)
        && map.total_bytes() == sum
}

#[inline(never)]
pub fn valid() -> bool {
    if !capacity_boundary() {
        return false;
    }
    let mut b = Blob::new();
    b.at64(40, 0x40001000);
    b.at64(48, 0x2000);
    b.root();
    let property = b.property(b"status", b"okay\0");
    // QEMU's generated properties can have nonzero alignment padding.
    // All three padding bytes are semantically inert, including token bytes.
    b.bytes[property + 17..property + 20].copy_from_slice(&[0xff, 3, 9]);
    b.word(4);
    b.begin(b"bus");
    b.property(b"compatible", b"test,bus\0");
    b.begin(b"device@0");
    b.region(0, 0x1000);
    b.end();
    b.end();
    b.memory();
    b.begin(b"memory@50000000");
    b.property(b"device_type", b"memory\0");
    b.property(b"status", b"ok\0");
    b.region(0x50000000, 0x1000000);
    b.end();
    b.begin(b"reserved-memory");
    b.cells();
    b.property(b"ranges", &[]);
    b.begin(b"buffer@40002000");
    b.region(0x40002000, 0x2000);
    b.end();
    b.begin(b"outside@90000000");
    b.region(0x90000000, 0x1000);
    b.end();
    b.end();
    b.end();
    b.finish();
    let original = b.bytes;
    let Ok(map) = parse(b.data()) else {
        return false;
    };
    b.okay
        && b.bytes == original
        && sound_map(&map)
        && map.dtb_size() == b.len
        && map.ram()
            == [
                Region {
                    start: 0x40000000,
                    end: 0x48000000,
                },
                Region {
                    start: 0x50000000,
                    end: 0x51000000,
                },
            ]
        && map.reservations()
            == [
                Region {
                    start: 0x40001000,
                    end: 0x40003000,
                },
                Region {
                    start: 0x40002000,
                    end: 0x40004000,
                },
                Region {
                    start: 0x90000000,
                    end: 0x90001000,
                },
            ]
}

#[inline(never)]
fn capacity_boundary() -> bool {
    let mut b = Blob::new();
    for index in 0..32 {
        b.at64(40 + index * 16, 0x90000000 + index as u64 * 4096);
        b.at64(48 + index * 16, 4096);
    }
    b.root();
    b.begin(b"memory@40000000");
    b.property(b"device_type", b"memory\0");
    let mut ranges = [0u8; 16 * 16];
    for (index, range) in ranges.as_chunks_mut::<16>().0.iter_mut().enumerate() {
        range[..8].copy_from_slice(&(0x40000000u64 + index as u64 * 4096).to_be_bytes());
        range[8..].copy_from_slice(&4096u64.to_be_bytes());
    }
    b.property(b"reg", &ranges);
    b.end();
    b.end();
    b.finish();
    let Ok(map) = parse(b.data()) else {
        return false;
    };
    b.okay
        && sound_map(&map)
        && map.ram().len() == 16
        && map.reservations().len() == 32
        && map.total_bytes() == 16 * 4096
}

#[inline(never)]
fn invalid_tree(case: u8) -> bool {
    let mut b = Blob::new();
    b.root();
    match case {
        0 => {
            b.property(b"#size-cells", &2u32.to_be_bytes());
        }
        1 => {
            b.property(b"status", b"disabled\0");
        }
        2 => {
            b.end();
            b.begin(b"second-root");
        }
        _ => {}
    }
    if case == 3 {
        b.begin(b"bus");
    }
    b.begin(b"memory@40000000");
    if case != 4 {
        b.property(b"device_type", b"memory\0");
    }
    match case {
        5 => {
            b.region(u64::MAX - 1, 4);
        }
        6 => {
            b.region(0x40000000, 0);
        }
        7 => {
            b.property(b"reg", &[0; 15]);
        }
        8 => {
            b.property(b"reg", &[]);
        }
        9 => {
            let mut ranges = [0u8; 17 * 16];
            for (index, range) in ranges.as_chunks_mut::<16>().0.iter_mut().enumerate() {
                range[..8].copy_from_slice(&(0x40000000u64 + index as u64 * 4096).to_be_bytes());
                range[8..].copy_from_slice(&4096u64.to_be_bytes());
            }
            b.property(b"reg", &ranges);
        }
        _ => {
            b.region(0x40000000, 0x08000000);
        }
    }
    match case {
        10 => {
            b.region(0x50000000, 4096);
        }
        11 => {
            b.property(b"status", b"disabled\0");
        }
        12 => {
            b.property(b"status", b"okay");
        }
        13 => {
            b.begin(b"child");
            b.end();
        }
        14 => {
            b.property(b"device_type", b"memory\0");
        }
        _ => {}
    }
    b.end();
    if case == 3 {
        b.end();
    }
    if case == 15 {
        b.memory();
    }
    if case == 16 {
        b.property(b"status", b"okay\0");
    }
    if case == 17 {
        b.word(7);
    }
    if case == 18 {
        for _ in 0..33 {
            b.begin(b"a");
        }
        for _ in 0..33 {
            b.end();
        }
    }
    b.end();
    if case == 19 {
        b.end();
    }
    b.finish();
    b.rejected()
}

#[inline(never)]
fn invalid_reservation(case: u8) -> bool {
    let mut b = Blob::new();
    b.root();
    b.memory();
    b.begin(b"reserved-memory");
    b.cells();
    if case == 0 {
        b.property(b"ranges", &[0; 16]);
    } else if case != 1 {
        b.property(b"ranges", &[]);
    }
    if case == 2 {
        b.property(b"status", b"disabled\0");
    }
    b.begin(b"buffer");
    if case != 3 {
        b.region(0x40010000, 4096);
    }
    match case {
        3 | 4 => {
            b.property(b"size", &4096u64.to_be_bytes());
        }
        5 => {
            b.property(b"alignment", &4096u64.to_be_bytes());
        }
        6 => {
            b.property(b"alloc-ranges", &[0; 16]);
        }
        7 => {
            b.property(b"no-map", &[]);
        }
        8 => {
            b.property(b"reusable", &[]);
        }
        9 => {
            b.property(b"status", b"disabled\0");
        }
        10 => {
            b.begin(b"child");
            b.end();
        }
        11 => {
            b.region(0x40020000, 4096);
        }
        _ => {}
    }
    b.end();
    b.end();
    if case == 12 {
        b.begin(b"reserved-memory");
        b.cells();
        b.property(b"ranges", &[]);
        b.end();
    }
    b.end();
    b.finish();
    b.rejected()
}

// Keep each fixture in a separate non-inlined frame: the boot stack is 64 KiB.
// Multiple shadowed 4 KiB blobs previously inflated one frame beyond 38 KiB.
#[inline(never)]
fn truncations_and_mutations() -> bool {
    let mut b = basic();
    if !b.okay || parse(b.data()).is_err() {
        return false;
    }
    for cut in 0..b.len {
        if parse(&b.bytes[..cut]).is_ok() {
            return false;
        }
    }
    // Some mutations are valid; every accepted map must preserve invariants.
    for offset in 0..b.len {
        b.bytes[offset] ^= 0xff;
        if let Ok(map) = parse(b.data())
            && !sound_map(&map)
        {
            return false;
        }
        b.bytes[offset] ^= 0xff;
    }
    true
}

#[inline(never)]
fn invalid_header(offset: usize, value: u32) -> bool {
    let mut bad = basic();
    bad.at32(offset, value);
    bad.rejected()
}

#[inline(never)]
fn invalid_structure(case: u8) -> bool {
    let mut bad = basic();
    match case {
        0 => {
            bad.bytes[STRUCTURE + 5] = 1;
        }
        1 => {
            let structure_end = bad.len - STRINGS.len();
            bad.bytes[STRUCTURE + 4..structure_end].fill(b'x');
        }
        2..=4 => {
            // Reuse this blob's storage for END/order/empty-memory fixtures.
            bad.bytes.fill(0);
            bad.cursor = STRUCTURE;
            bad.len = 0;
            bad.root();
            if case != 4 {
                bad.memory();
            }
            if case != 2 {
                bad.end();
            }
            if case == 3 {
                bad.word(9);
                bad.word(4);
            }
            bad.finish();
        }
        5 => {
            bad.at32(STRUCTURE + 16, u32::MAX);
        }
        _ => {
            let last = bad.len - 1;
            bad.bytes[last] = b'x';
            // Property points to a string with no terminator within its block.
            bad.at32(
                STRUCTURE + 16,
                (STRINGS.len() - b"compatible\0".len()) as u32,
            );
        }
    }
    bad.rejected()
}

#[inline(never)]
fn invalid_header_reservation(case: u8) -> bool {
    let mut bad = basic();
    match case {
        0 => {
            // Exactly one valid tuple and no space for the required terminator.
            bad.at32(16, (STRUCTURE - 16) as u32);
            bad.at64(STRUCTURE - 16, 0x40000000);
            bad.at64(STRUCTURE - 8, 4096);
        }
        1 => {
            bad.at64(40, u64::MAX);
            bad.at64(48, 2);
        }
        2 => {
            bad.at64(40, 1);
        }
        3 => {
            bad.at64(40, 0x40000000);
            bad.at64(48, 4096);
            bad.at64(56, 0x40000001);
            bad.at64(64, 4096);
        }
        _ => {
            for index in 0..33 {
                bad.at64(40 + index * 16, 0x40000000 + index as u64 * 4096);
                bad.at64(48 + index * 16, 4096);
            }
        }
    }
    bad.rejected()
}

#[inline(never)]
pub fn malformed() -> bool {
    if !truncations_and_mutations() {
        return false;
    }
    for (offset, value) in [
        (0, 0),
        (4, 39),
        (4, 0x200001),
        (8, 609),
        (8, u32::MAX),
        (12, STRUCTURE as u32),
        (16, 41),
        (16, STRUCTURE as u32),
        (20, 16),
        (20, 18),
        (24, 17),
        (32, u32::MAX),
        (36, u32::MAX),
        (STRUCTURE, 2),
        (STRUCTURE + 8 + 12, 1),
    ] {
        if !invalid_header(offset, value) {
            return false;
        }
    }
    for case in 0..7 {
        if !invalid_structure(case) {
            return false;
        }
    }
    for case in 0..20 {
        if !invalid_tree(case) {
            return false;
        }
    }
    for case in 0..13 {
        if !invalid_reservation(case) {
            return false;
        }
    }
    for case in 0..5 {
        if !invalid_header_reservation(case) {
            return false;
        }
    }
    true
}
