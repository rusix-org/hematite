//! Bounded CPU/PSCI descriptions, not authority to start another CPU.
//! The admitted QEMU profile is specified by design 0014.
#![forbid(unsafe_code)]

const MAX_SIZE: usize = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_PROPERTIES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Header,
    Bounds,
    Structure,
    Unsupported,
    Duplicate,
    Capacity,
    MissingCpus,
    MissingPsci,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Conduit {
    Hvc,
}

/// Compatibility advertised by firmware, not the result of PSCI_VERSION.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Psci {
    pub conduit: Conduit,
    pub advertised_1_0: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Topology {
    affinities: [u64; 2],
    count: usize,
    psci: Option<Psci>,
}
impl Topology {
    pub fn cpu_count(&self) -> usize {
        self.count
    }
    pub fn affinities(&self) -> &[u64] {
        &self.affinities[..self.count]
    }
    pub fn psci(&self) -> Option<Psci> {
        self.psci
    }
}

fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8], Error> {
    data.get(offset..offset.checked_add(length).ok_or(Error::Bounds)?)
        .ok_or(Error::Bounds)
}
fn word(data: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_be_bytes(
        bytes(data, offset, 4)?
            .try_into()
            .map_err(|_| Error::Bounds)?,
    ))
}
fn doubleword(data: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_be_bytes(
        bytes(data, offset, 8)?
            .try_into()
            .map_err(|_| Error::Bounds)?,
    ))
}
fn string(data: &[u8]) -> Result<&[u8], Error> {
    let length = data.iter().position(|b| *b == 0).ok_or(Error::Structure)?;
    Ok(&data[..length])
}
fn exact_string(value: &[u8]) -> Result<&[u8], Error> {
    let text = string(value)?;
    if text.len() + 1 != value.len() {
        return Err(Error::Structure);
    }
    Ok(text)
}
fn compatible(value: &[u8]) -> Result<(bool, bool), Error> {
    if value.is_empty() || value.last() != Some(&0) {
        return Err(Error::Structure);
    }
    let mut modern = false;
    let mut v1 = false;
    for item in value[..value.len() - 1].split(|b| *b == 0) {
        if item.is_empty() {
            return Err(Error::Structure);
        }
        modern |= item == b"arm,psci-0.2" || item == b"arm,psci-1.0";
        v1 |= item == b"arm,psci-1.0";
    }
    Ok((modern, v1))
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let value = bytes(self.data, self.offset, length)?;
        self.offset = self.offset.checked_add(length).ok_or(Error::Bounds)?;
        Ok(value)
    }
    fn word(&mut self) -> Result<u32, Error> {
        word(self.take(4)?, 0)
    }
    fn align(&mut self, zero: bool) -> Result<(), Error> {
        let count = (4 - self.offset % 4) % 4;
        if self.take(count)?.iter().any(|b| zero && *b != 0) {
            return Err(Error::Structure);
        }
        Ok(())
    }
    fn name(&mut self) -> Result<&'a [u8], Error> {
        let value = string(self.data.get(self.offset..).ok_or(Error::Bounds)?)?;
        self.take(value.len().checked_add(1).ok_or(Error::Bounds)?)?;
        self.align(true)?;
        if value.contains(&b'/') {
            return Err(Error::Structure);
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Root,
    Cpus,
    Cpu,
    Psci,
    Other,
}
#[derive(Clone, Copy)]
struct Node<'a> {
    kind: Kind,
    child: bool,
    properties: [u32; MAX_PROPERTIES],
    count: usize,
    address_cells: Option<u32>,
    size_cells: Option<u32>,
    unit: Option<u64>,
    reg: Option<&'a [u8]>,
    cpu_type: bool,
    enabled: bool,
    method: bool,
    modern: bool,
    v1: bool,
}
impl<'a> Node<'a> {
    const EMPTY: Self = Self {
        kind: Kind::Other,
        child: false,
        properties: [0; MAX_PROPERTIES],
        count: 0,
        address_cells: None,
        size_cells: None,
        unit: None,
        reg: None,
        cpu_type: false,
        enabled: false,
        method: false,
        modern: false,
        v1: false,
    };
    fn property(&mut self, names: &[u8], offset: u32, value: &'a [u8]) -> Result<(), Error> {
        if self.child {
            return Err(Error::Structure);
        }
        let name = string(names.get(offset as usize..).ok_or(Error::Bounds)?)?;
        if name.is_empty() {
            return Err(Error::Structure);
        }
        for old in &self.properties[..self.count] {
            if string(names.get(*old as usize..).ok_or(Error::Bounds)?)? == name {
                return Err(Error::Duplicate);
            }
        }
        *self.properties.get_mut(self.count).ok_or(Error::Capacity)? = offset;
        self.count += 1;
        if name == b"device_type" {
            let text = exact_string(value)?;
            if (self.kind == Kind::Cpu) != (text == b"cpu") {
                return Err(Error::Unsupported);
            }
            self.cpu_type = text == b"cpu";
        }
        if name == b"compatible" {
            let (modern, v1) = compatible(value)?;
            let psci = modern
                || value
                    .split(|byte| *byte == 0)
                    .any(|item| item == b"arm,psci");
            if psci && self.kind != Kind::Psci {
                return Err(Error::Unsupported);
            }
            self.modern = modern;
            self.v1 = v1;
        }
        if self.kind != Kind::Other
            && name == b"status"
            && !matches!(exact_string(value)?, b"okay" | b"ok")
        {
            return Err(Error::Unsupported);
        }
        match self.kind {
            Kind::Cpus => match name {
                b"#address-cells" | b"#size-cells" => {
                    if value.len() != 4 {
                        return Err(Error::Structure);
                    }
                    let count = word(value, 0)?;
                    if name == b"#address-cells" {
                        if count != 1 && count != 2 {
                            return Err(Error::Unsupported);
                        }
                        self.address_cells = Some(count);
                    } else {
                        if count != 0 {
                            return Err(Error::Unsupported);
                        }
                        self.size_cells = Some(count);
                    }
                }
                b"enable-method" => return Err(Error::Unsupported),
                _ => {}
            },
            Kind::Cpu => match name {
                b"reg" => self.reg = Some(value),
                b"enable-method" => {
                    if exact_string(value)? != b"psci" {
                        return Err(Error::Unsupported);
                    }
                    self.enabled = true;
                }
                _ => {}
            },
            Kind::Psci => match name {
                b"method" => {
                    if exact_string(value)? != b"hvc" {
                        return Err(Error::Unsupported);
                    }
                    self.method = true;
                }
                b"cpu_on" | b"cpu_off" | b"cpu_suspend" | b"migrate" => {
                    let expected = match name {
                        b"cpu_on" => 0xc400_0003,
                        b"cpu_off" => 0x8400_0002,
                        b"cpu_suspend" => 0xc400_0001,
                        _ => 0xc400_0005,
                    };
                    if value.len() != 4 {
                        return Err(Error::Structure);
                    }
                    if word(value, 0)? != expected {
                        return Err(Error::Unsupported);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
}

fn unit_address(name: &[u8]) -> Result<u64, Error> {
    let digits = name.strip_prefix(b"cpu@").ok_or(Error::Unsupported)?;
    if digits.is_empty() || digits.len() > 16 {
        return Err(Error::Structure);
    }
    let mut value = 0u64;
    for digit in digits {
        let nibble = match digit {
            b'0'..=b'9' => digit - b'0',
            b'a'..=b'f' => digit - b'a' + 10,
            _ => return Err(Error::Structure),
        };
        value = value
            .checked_mul(16)
            .and_then(|v| v.checked_add(u64::from(nibble)))
            .ok_or(Error::Unsupported)?;
    }
    Ok(value)
}

/// Parse an owned description of the narrow CPU0 or CPU0/CPU1 PSCI profile.
/// Root memory/device semantics are deliberately left to their own parsers.
pub fn parse(input: &[u8]) -> Result<Topology, Error> {
    if input.len() < 40 || word(input, 0)? != 0xd00d_feed {
        return Err(Error::Header);
    }
    let size = word(input, 4)? as usize;
    if !(40..=MAX_SIZE).contains(&size) || word(input, 20)? != 17 || word(input, 24)? != 16 {
        return Err(Error::Header);
    }
    if word(input, 28)? != 0 {
        return Err(Error::Unsupported);
    }
    let blob = bytes(input, 0, size)?;
    let structure_start = word(blob, 8)? as usize;
    let strings_start = word(blob, 12)? as usize;
    let reserve_start = word(blob, 16)? as usize;
    let strings_size = word(blob, 32)? as usize;
    let structure_size = word(blob, 36)? as usize;
    if reserve_start < 40
        || !reserve_start.is_multiple_of(8)
        || !structure_start.is_multiple_of(4)
        || !structure_size.is_multiple_of(4)
        || reserve_start >= structure_start
        || structure_start
            .checked_add(structure_size)
            .ok_or(Error::Bounds)?
            > strings_start
    {
        return Err(Error::Header);
    }
    let structure = bytes(blob, structure_start, structure_size)?;
    let names = bytes(blob, strings_start, strings_size)?;
    let mut reserves = Cursor {
        data: blob
            .get(reserve_start..structure_start)
            .ok_or(Error::Bounds)?,
        offset: 0,
    };
    loop {
        let tuple = reserves.take(16)?;
        let address = doubleword(tuple, 0)?;
        let length = doubleword(tuple, 8)?;
        if address == 0 && length == 0 {
            break;
        }
        if length == 0 || address.checked_add(length).is_none() {
            return Err(Error::Structure);
        }
    }
    let mut cursor = Cursor {
        data: structure,
        offset: 0,
    };
    let mut stack = [Node::EMPTY; MAX_DEPTH];
    let mut depth = 0usize;
    let mut root = false;
    let mut cpus = false;
    let mut psci_seen = false;
    let mut psci = None;
    let mut present = [false; 2];
    let mut methods = [false; 2];
    loop {
        match cursor.word()? {
            1 => {
                let name = cursor.name()?;
                let kind = if depth == 0 {
                    if root || !name.is_empty() {
                        return Err(Error::Structure);
                    }
                    root = true;
                    Kind::Root
                } else {
                    if name.is_empty() {
                        return Err(Error::Structure);
                    }
                    let parent = stack.get_mut(depth - 1).ok_or(Error::Structure)?;
                    parent.child = true;
                    match parent.kind {
                        Kind::Cpu | Kind::Psci => return Err(Error::Unsupported),
                        Kind::Root if name == b"cpus" => {
                            if cpus {
                                return Err(Error::Duplicate);
                            }
                            cpus = true;
                            Kind::Cpus
                        }
                        Kind::Root if name == b"psci" => {
                            if psci_seen {
                                return Err(Error::Duplicate);
                            }
                            psci_seen = true;
                            Kind::Psci
                        }
                        Kind::Cpus if name.starts_with(b"cpu@") => Kind::Cpu,
                        _ if name == b"cpus"
                            || name == b"psci"
                            || name.starts_with(b"cpus@")
                            || name.starts_with(b"psci@")
                            || name.starts_with(b"cpu@")
                            || name == b"cpu" =>
                        {
                            return Err(Error::Unsupported);
                        }
                        _ => Kind::Other,
                    }
                };
                let unit = if kind == Kind::Cpu {
                    Some(unit_address(name)?)
                } else {
                    None
                };
                *stack.get_mut(depth).ok_or(Error::Capacity)? = Node {
                    kind,
                    unit,
                    ..Node::EMPTY
                };
                depth += 1;
            }
            2 => {
                depth = depth.checked_sub(1).ok_or(Error::Structure)?;
                let node = *stack.get(depth).ok_or(Error::Structure)?;
                match node.kind {
                    Kind::Cpus if node.address_cells.is_none() || node.size_cells != Some(0) => {
                        return Err(Error::Unsupported);
                    }
                    Kind::Cpu => {
                        let parent = stack
                            .get(depth.checked_sub(1).ok_or(Error::Structure)?)
                            .ok_or(Error::Structure)?;
                        let cells = parent.address_cells.ok_or(Error::Unsupported)?;
                        if parent.size_cells != Some(0) || !node.cpu_type {
                            return Err(Error::Unsupported);
                        }
                        let reg = node.reg.ok_or(Error::Structure)?;
                        if reg.len() != cells as usize * 4 {
                            return Err(Error::Structure);
                        }
                        let affinity = if cells == 1 {
                            u64::from(word(reg, 0)?)
                        } else {
                            doubleword(reg, 0)?
                        };
                        if node.unit != Some(affinity) || affinity > 1 {
                            return Err(Error::Unsupported);
                        }
                        let slot = affinity as usize;
                        if present[slot] {
                            return Err(Error::Duplicate);
                        }
                        present[slot] = true;
                        methods[slot] = node.enabled;
                    }
                    Kind::Psci => {
                        if !node.method || !node.modern {
                            return Err(Error::Unsupported);
                        }
                        psci = Some(Psci {
                            conduit: Conduit::Hvc,
                            advertised_1_0: node.v1,
                        });
                    }
                    _ => {}
                }
            }
            3 => {
                let length = cursor.word()? as usize;
                let offset = cursor.word()?;
                let value = cursor.take(length)?;
                // Preserve the existing QEMU accommodation for property padding.
                cursor.align(false)?;
                let node = stack
                    .get_mut(depth.checked_sub(1).ok_or(Error::Structure)?)
                    .ok_or(Error::Structure)?;
                node.property(names, offset, value)?;
            }
            4 => {}
            9 => {
                if !root || depth != 0 || cursor.offset != structure.len() {
                    return Err(Error::Structure);
                }
                if !cpus || !present[0] {
                    return Err(Error::MissingCpus);
                }
                if present[1] && (psci.is_none() || methods != [true; 2]) {
                    return Err(Error::MissingPsci);
                }
                if methods[0] && psci.is_none() {
                    return Err(Error::MissingPsci);
                }
                return Ok(Topology {
                    affinities: [0, 1],
                    count: if present[1] { 2 } else { 1 },
                    psci,
                });
            }
            _ => return Err(Error::Structure),
        }
    }
}
