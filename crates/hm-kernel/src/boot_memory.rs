//! Bounded FDT metadata decoding, not physical-memory authority.
//! See docs/design/0004-boot-memory.md for the deliberately narrow profile.

pub const MAX_DTB_SIZE: usize = 2 * 1024 * 1024;
const RAM_CAPACITY: usize = 16;
const RESERVED_CAPACITY: usize = 32;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Header,
    Bounds,
    Structure,
    Unsupported,
    Duplicate,
    Capacity,
    Range,
    Overlap,
    MissingMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Region {
    pub start: u64,
    pub end: u64,
}

impl Region {
    const EMPTY: Self = Self { start: 0, end: 0 };

    fn new(start: u64, size: u64) -> Result<Self, Error> {
        if size == 0 {
            return Err(Error::Range);
        }
        Ok(Self {
            start,
            end: start.checked_add(size).ok_or(Error::Range)?,
        })
    }

    fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Descriptive ranges only. These integers cannot mint frame ownership.
#[derive(Debug)]
pub struct MemoryMap {
    ram: [Region; RAM_CAPACITY],
    ram_count: usize,
    reserved: [Region; RESERVED_CAPACITY],
    reserved_count: usize,
    total: u64,
    dtb_size: usize,
}

impl MemoryMap {
    pub fn ram(&self) -> &[Region] {
        &self.ram[..self.ram_count]
    }

    pub fn reservations(&self) -> &[Region] {
        &self.reserved[..self.reserved_count]
    }

    pub fn total_bytes(&self) -> u64 {
        self.total
    }

    pub fn dtb_size(&self) -> usize {
        self.dtb_size
    }

    fn add_ram(&mut self, region: Region) -> Result<(), Error> {
        if self.ram().iter().any(|old| old.overlaps(region)) {
            return Err(Error::Overlap);
        }
        let total = self
            .total
            .checked_add(region.end - region.start)
            .ok_or(Error::Range)?;
        let slot = self.ram.get_mut(self.ram_count).ok_or(Error::Capacity)?;
        *slot = region;
        self.ram_count += 1;
        self.total = total;
        Ok(())
    }

    fn add_reserved(&mut self, region: Region) -> Result<(), Error> {
        let slot = self
            .reserved
            .get_mut(self.reserved_count)
            .ok_or(Error::Capacity)?;
        *slot = region;
        self.reserved_count += 1;
        Ok(())
    }
}

fn bytes(data: &[u8], start: usize, length: usize) -> Result<&[u8], Error> {
    let end = start.checked_add(length).ok_or(Error::Bounds)?;
    data.get(start..end).ok_or(Error::Bounds)
}

fn word(data: &[u8], start: usize) -> Result<u32, Error> {
    let value: [u8; 4] = bytes(data, start, 4)?
        .try_into()
        .map_err(|_| Error::Bounds)?;
    Ok(u32::from_be_bytes(value))
}

fn doubleword(data: &[u8], start: usize) -> Result<u64, Error> {
    let value: [u8; 8] = bytes(data, start, 8)?
        .try_into()
        .map_err(|_| Error::Bounds)?;
    Ok(u64::from_be_bytes(value))
}

fn c_string(data: &[u8]) -> Result<&[u8], Error> {
    let length = data
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Error::Structure)?;
    bytes(data, 0, length)
}

struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let result = bytes(self.data, self.position, length)?;
        self.position = self.position.checked_add(length).ok_or(Error::Bounds)?;
        Ok(result)
    }

    fn word(&mut self) -> Result<u32, Error> {
        word(self.take(4)?, 0)
    }

    fn align(&mut self, require_zero: bool) -> Result<(), Error> {
        let padding = (4 - self.position % 4) % 4;
        let bytes = self.take(padding)?;
        if require_zero && bytes.iter().any(|byte| *byte != 0) {
            return Err(Error::Structure);
        }
        Ok(())
    }

    fn name(&mut self) -> Result<&'a [u8], Error> {
        let suffix = self.data.get(self.position..).ok_or(Error::Bounds)?;
        let name = c_string(suffix)?;
        self.take(name.len().checked_add(1).ok_or(Error::Bounds)?)?;
        self.align(true)?;
        if name.contains(&b'/') {
            return Err(Error::Structure);
        }
        Ok(name)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Root,
    Memory,
    Reserved,
    Reservation,
    Other,
}

#[derive(Clone, Copy)]
struct Node<'a> {
    kind: Kind,
    children: bool,
    seen: u8,
    reg: Option<&'a [u8]>,
}

const ADDRESS: u8 = 1;
const SIZE: u8 = 2;
const RANGES: u8 = 4;
const REG: u8 = 8;
const TYPE: u8 = 16;
const STATUS: u8 = 32;

impl<'a> Node<'a> {
    const EMPTY: Self = Self {
        kind: Kind::Other,
        children: false,
        seen: 0,
        reg: None,
    };

    fn once(&mut self, flag: u8) -> Result<(), Error> {
        if self.seen & flag != 0 {
            return Err(Error::Duplicate);
        }
        self.seen |= flag;
        Ok(())
    }

    fn property(&mut self, name: &[u8], value: &'a [u8]) -> Result<(), Error> {
        if self.children {
            return Err(Error::Structure);
        }
        if name == b"device_type" {
            self.once(TYPE)?;
            if (self.kind == Kind::Memory && value != b"memory\0")
                || (self.kind != Kind::Memory && value == b"memory\0")
            {
                return Err(Error::Unsupported);
            }
        }
        match self.kind {
            Kind::Root | Kind::Reserved => match name {
                b"#address-cells" | b"#size-cells" => {
                    self.once(if name == b"#address-cells" {
                        ADDRESS
                    } else {
                        SIZE
                    })?;
                    if value.len() != 4 || word(value, 0)? != 2 {
                        return Err(Error::Unsupported);
                    }
                }
                b"ranges" if self.kind == Kind::Reserved => {
                    self.once(RANGES)?;
                    if !value.is_empty() {
                        return Err(Error::Unsupported);
                    }
                }
                _ => {}
            },
            Kind::Memory | Kind::Reservation => {
                if name == b"reg" {
                    self.once(REG)?;
                    if value.is_empty() || !value.len().is_multiple_of(16) {
                        return Err(Error::Structure);
                    }
                    self.reg = Some(value);
                }
            }
            Kind::Other => {}
        }
        if self.kind != Kind::Other && name == b"status" {
            self.once(STATUS)?;
            if value != b"okay\0" && value != b"ok\0" {
                return Err(Error::Unsupported);
            }
        }
        if matches!(self.kind, Kind::Reserved | Kind::Reservation)
            && matches!(
                name,
                b"size" | b"alignment" | b"alloc-ranges" | b"no-map" | b"reusable"
            )
        {
            return Err(Error::Unsupported);
        }
        Ok(())
    }

    fn finish(self, map: &mut MemoryMap) -> Result<(), Error> {
        let required = match self.kind {
            Kind::Root => ADDRESS | SIZE,
            Kind::Memory => TYPE | REG,
            Kind::Reserved => ADDRESS | SIZE | RANGES,
            Kind::Reservation => REG,
            Kind::Other => 0,
        };
        if self.seen & required != required {
            return Err(Error::Structure);
        }
        if let Some(reg) = self.reg {
            for tuple in reg.as_chunks::<16>().0 {
                let region = Region::new(doubleword(tuple, 0)?, doubleword(tuple, 8)?)?;
                if self.kind == Kind::Memory {
                    map.add_ram(region)?;
                } else {
                    map.add_reserved(region)?;
                }
            }
        }
        Ok(())
    }
}

/// Decode only the reviewed static 64-bit root-memory FDT profile.
/// Invalid or unsupported input returns no partially usable map.
pub fn parse(input: &[u8]) -> Result<MemoryMap, Error> {
    if input.len() < 40 || word(input, 0)? != 0xd00d_feed {
        return Err(Error::Header);
    }
    let size = word(input, 4)? as usize;
    if !(40..=MAX_DTB_SIZE).contains(&size) || word(input, 20)? != 17 || word(input, 24)? != 16 {
        return Err(Error::Header);
    }
    let blob = bytes(input, 0, size)?;
    let structure_start = word(blob, 8)? as usize;
    let strings_start = word(blob, 12)? as usize;
    let reserve_start = word(blob, 16)? as usize;
    let strings_size = word(blob, 32)? as usize;
    let structure_size = word(blob, 36)? as usize;
    let structure_end = structure_start
        .checked_add(structure_size)
        .ok_or(Error::Bounds)?;
    if reserve_start < 40
        || !reserve_start.is_multiple_of(8)
        || !structure_start.is_multiple_of(4)
        || !structure_size.is_multiple_of(4)
        || reserve_start >= structure_start
        || structure_end > strings_start
    {
        return Err(Error::Header);
    }
    let structure = bytes(blob, structure_start, structure_size)?;
    let strings = bytes(blob, strings_start, strings_size)?;
    let reservations = blob
        .get(reserve_start..structure_start)
        .ok_or(Error::Bounds)?;
    let mut map = MemoryMap {
        ram: [Region::EMPTY; RAM_CAPACITY],
        ram_count: 0,
        reserved: [Region::EMPTY; RESERVED_CAPACITY],
        reserved_count: 0,
        total: 0,
        dtb_size: size,
    };
    let mut reservation_cursor = Cursor {
        data: reservations,
        position: 0,
    };
    loop {
        let tuple = reservation_cursor.take(16)?;
        let start = doubleword(tuple, 0)?;
        let length = doubleword(tuple, 8)?;
        if start == 0 && length == 0 {
            break;
        }
        let region = Region::new(start, length)?;
        if map.reservations().iter().any(|old| old.overlaps(region)) {
            return Err(Error::Overlap);
        }
        map.add_reserved(region)?;
    }
    let mut cursor = Cursor {
        data: structure,
        position: 0,
    };
    let mut stack = [Node::EMPTY; MAX_DEPTH];
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut reserved_seen = false;
    loop {
        match cursor.word()? {
            1 => {
                let name = cursor.name()?;
                let kind = if depth == 0 {
                    if root_seen || !name.is_empty() {
                        return Err(Error::Structure);
                    }
                    root_seen = true;
                    Kind::Root
                } else {
                    if name.is_empty() {
                        return Err(Error::Structure);
                    }
                    let parent = stack.get_mut(depth - 1).ok_or(Error::Structure)?;
                    parent.children = true;
                    match parent.kind {
                        Kind::Memory | Kind::Reservation => return Err(Error::Unsupported),
                        Kind::Reserved => Kind::Reservation,
                        Kind::Root if name == b"reserved-memory" => {
                            if reserved_seen {
                                return Err(Error::Duplicate);
                            }
                            reserved_seen = true;
                            Kind::Reserved
                        }
                        Kind::Root if name == b"memory" || name.starts_with(b"memory@") => {
                            Kind::Memory
                        }
                        _ => Kind::Other,
                    }
                };
                let slot = stack.get_mut(depth).ok_or(Error::Capacity)?;
                *slot = Node {
                    kind,
                    ..Node::EMPTY
                };
                depth += 1;
            }
            2 => {
                depth = depth.checked_sub(1).ok_or(Error::Structure)?;
                stack.get(depth).ok_or(Error::Structure)?.finish(&mut map)?;
            }
            3 => {
                let length = cursor.word()? as usize;
                let offset = cursor.word()? as usize;
                let name = c_string(strings.get(offset..).ok_or(Error::Bounds)?)?;
                if name.is_empty() {
                    return Err(Error::Structure);
                }
                let value = cursor.take(length)?;
                // Pinned QEMU emits nonzero property padding. Length bounds
                // define the value; ignore only the following 0–3 pad bytes.
                // See design 0004 for the recorded format accommodation.
                cursor.align(false)?;
                let index = depth.checked_sub(1).ok_or(Error::Structure)?;
                stack
                    .get_mut(index)
                    .ok_or(Error::Structure)?
                    .property(name, value)?;
            }
            4 => {}
            9 => {
                if depth != 0 || !root_seen || cursor.position != structure.len() {
                    return Err(Error::Structure);
                }
                if map.ram_count == 0 {
                    return Err(Error::MissingMemory);
                }
                return Ok(map);
            }
            _ => return Err(Error::Structure),
        }
    }
}
