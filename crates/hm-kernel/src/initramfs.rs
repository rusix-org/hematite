//! Complete, bounded admission of the restricted native newc boot archive.
#![forbid(unsafe_code)]

pub const MAX_ARCHIVE_SIZE: usize = 4 * 1024 * 1024;
pub const MAX_FILE_SIZE: usize = 1024 * 1024;
pub const MAX_PATH_SIZE: usize = 128;
pub const MAX_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Header,
    Unsupported,
    Bounds,
    Capacity,
    Path,
    Duplicate,
    Hierarchy,
    Padding,
    Trailer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
}

/// Immutable slices borrow the admitted archive input, not the plan object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry<'a> {
    path: &'a str,
    bytes: &'a [u8],
    kind: Kind,
    permissions: u16,
    identity: [u32; 3],
}
impl<'a> Entry<'a> {
    pub fn path(self) -> &'a str {
        self.path
    }
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn kind(self) -> Kind {
        self.kind
    }
    pub fn permissions(self) -> u16 {
        self.permissions
    }
}

#[derive(Debug)]
pub struct Archive<'a> {
    entries: [Option<Entry<'a>>; MAX_ENTRIES],
    count: usize,
}
impl<'a> Archive<'a> {
    /// No entry is published until the trailer and complete suffix validate.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_ARCHIVE_SIZE {
            return Err(Error::Capacity);
        }
        let mut archive = Self {
            entries: [None; MAX_ENTRIES],
            count: 0,
        };
        let mut cursor = 0usize;
        loop {
            if cursor == bytes.len() {
                return Err(Error::Trailer);
            }
            let name_start = cursor.checked_add(110).ok_or(Error::Bounds)?;
            let header = bytes.get(cursor..name_start).ok_or(Error::Bounds)?;
            if header.get(..6) != Some(b"070701".as_slice()) {
                return Err(Error::Header);
            }
            let mut fields = [0u32; 13];
            for (index, value) in fields.iter_mut().enumerate() {
                let start = 6 + index * 8;
                *value = hexadecimal(header.get(start..start + 8).ok_or(Error::Header)?)?;
            }
            let namesize = usize::try_from(fields[11]).map_err(|_| Error::Bounds)?;
            if !(2..=MAX_PATH_SIZE + 1).contains(&namesize) {
                return Err(Error::Path);
            }
            let name_end = name_start.checked_add(namesize).ok_or(Error::Bounds)?;
            let name = bytes.get(name_start..name_end).ok_or(Error::Bounds)?;
            let (terminator, name) = name.split_last().ok_or(Error::Path)?;
            if *terminator != 0 || name.contains(&0) {
                return Err(Error::Path);
            }
            let data_start = align4(name_end)?;
            zero_padding(bytes, name_end, data_start)?;
            let size = usize::try_from(fields[6]).map_err(|_| Error::Bounds)?;
            if size > MAX_FILE_SIZE {
                return Err(Error::Capacity);
            }
            let data_end = data_start.checked_add(size).ok_or(Error::Bounds)?;
            let data = bytes.get(data_start..data_end).ok_or(Error::Bounds)?;
            let next = align4(data_end)?;
            zero_padding(bytes, data_end, next)?;
            if name == b"TRAILER!!!" {
                for (index, value) in fields.iter().enumerate() {
                    let expected = match index {
                        4 => 1,
                        11 => 11,
                        _ => 0,
                    };
                    if *value != expected {
                        return Err(Error::Trailer);
                    }
                }
                let tail = bytes.get(next..).ok_or(Error::Bounds)?;
                if tail.len() > 511 || tail.iter().any(|b| *b != 0) {
                    return Err(Error::Trailer);
                }
                return Ok(archive);
            }
            if archive.count == MAX_ENTRIES {
                return Err(Error::Capacity);
            }
            let path = path(name)?;
            let kind = match fields[1] & !0o777 {
                0o100000 if fields[4] == 1 => Kind::File,
                0o040000 if fields[4] >= 1 && size == 0 => Kind::Directory,
                _ => return Err(Error::Unsupported),
            };
            if fields[9] != 0 || fields[10] != 0 || fields[12] != 0 {
                return Err(Error::Unsupported);
            }
            let identity = [fields[7], fields[8], fields[0]];
            if archive
                .entries()
                .any(|e| e.path == path || e.identity == identity)
            {
                return Err(Error::Duplicate);
            }
            for (end, byte) in name.iter().enumerate() {
                if *byte == b'/' {
                    let parent = path.get(..end).ok_or(Error::Path)?;
                    if !archive
                        .get(parent)
                        .is_some_and(|e| e.kind == Kind::Directory)
                    {
                        return Err(Error::Hierarchy);
                    }
                }
            }
            archive.entries[archive.count] = Some(Entry {
                path,
                bytes: data,
                kind,
                permissions: (fields[1] & 0o777) as u16,
                identity,
            });
            archive.count += 1;
            cursor = next;
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn entries(&self) -> impl Iterator<Item = Entry<'a>> + '_ {
        self.entries[..self.count].iter().filter_map(|entry| *entry)
    }
    /// Exact lookup only: no normalization, prefix matching or implicit links.
    pub fn get(&self, path: &str) -> Option<Entry<'a>> {
        self.entries().find(|entry| entry.path == path)
    }
}

fn hexadecimal(bytes: &[u8]) -> Result<u32, Error> {
    let mut value = 0u32;
    for byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return Err(Error::Header),
        };
        value = value
            .checked_mul(16)
            .and_then(|v| v.checked_add(u32::from(digit)))
            .ok_or(Error::Header)?;
    }
    Ok(value)
}
fn align4(value: usize) -> Result<usize, Error> {
    value.checked_add(3).map(|n| n & !3).ok_or(Error::Bounds)
}
fn zero_padding(bytes: &[u8], start: usize, end: usize) -> Result<(), Error> {
    if bytes
        .get(start..end)
        .ok_or(Error::Bounds)?
        .iter()
        .any(|b| *b != 0)
    {
        return Err(Error::Padding);
    }
    Ok(())
}
fn path(bytes: &[u8]) -> Result<&str, Error> {
    for component in bytes.split(|b| *b == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component
                .iter()
                .any(|b| !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
        {
            return Err(Error::Path);
        }
    }
    core::str::from_utf8(bytes).map_err(|_| Error::Path)
}
