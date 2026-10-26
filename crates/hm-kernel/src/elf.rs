//! Bounded native AArch64 ELF admission; design 0021, stage A.
#![forbid(unsafe_code)]

pub const USER_IMAGE_START: u64 = 0x1_0000_0000;
pub const USER_IMAGE_END: u64 = 0x1_0010_0000;
pub const MAX_FILE_SIZE: usize = 1024 * 1024;
pub const MAX_PROGRAM_HEADERS: usize = 16;
pub const MAX_LOAD_SEGMENTS: usize = 8;
pub const MAX_IMAGE_PAGES: usize = 256;
pub const PAGE_SIZE: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Header,
    Unsupported,
    Bounds,
    Capacity,
    Layout,
    Permissions,
    Entry,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Permissions {
    ReadOnly,
    ReadWrite,
    ReadExecute,
}
impl Permissions {
    pub const fn readable(self) -> bool {
        true
    }
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
    pub const fn executable(self) -> bool {
        matches!(self, Self::ReadExecute)
    }
}

/// Validated metadata, not authority to access these virtual addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Segment<'a> {
    virtual_address: u64,
    memory_size: u64,
    file_bytes: &'a [u8],
    permissions: Permissions,
    page_start: u64,
    page_end: u64,
}
impl<'a> Segment<'a> {
    const EMPTY: Self = Self {
        virtual_address: 0,
        memory_size: 0,
        file_bytes: &[],
        permissions: Permissions::ReadOnly,
        page_start: 0,
        page_end: 0,
    };
    pub const fn virtual_address(&self) -> u64 {
        self.virtual_address
    }
    pub const fn memory_size(&self) -> u64 {
        self.memory_size
    }
    pub const fn file_bytes(&self) -> &'a [u8] {
        self.file_bytes
    }
    pub const fn permissions(&self) -> Permissions {
        self.permissions
    }
    pub const fn page_start(&self) -> u64 {
        self.page_start
    }
    pub const fn page_end(&self) -> u64 {
        self.page_end
    }
}

/// Borrows immutable input until the eventual loader finishes copying it.
#[derive(Debug, Eq, PartialEq)]
pub struct Image<'a> {
    entry: u64,
    segments: [Segment<'a>; MAX_LOAD_SEGMENTS],
    count: usize,
    pages: usize,
}
impl<'a> Image<'a> {
    pub const fn entry(&self) -> u64 {
        self.entry
    }
    pub fn segments(&self) -> &[Segment<'a>] {
        &self.segments[..self.count]
    }
    pub const fn page_count(&self) -> usize {
        self.pages
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_FILE_SIZE {
            return Err(Error::Capacity);
        }
        if bytes.len() < 64 {
            return Err(Error::Bounds);
        }
        if &bytes[..4] != b"\x7fELF" {
            return Err(Error::Header);
        }
        if bytes[4] != 2
            || bytes[5] != 1
            || bytes[6] != 1
            || bytes[7..16].iter().any(|b| *b != 0)
            || u16_at(bytes, 16)? != 2
            || u16_at(bytes, 18)? != 183
            || u32_at(bytes, 20)? != 1
            || u32_at(bytes, 48)? != 0
        {
            return Err(Error::Unsupported);
        }
        if u16_at(bytes, 52)? != 64 || u16_at(bytes, 54)? != 56 {
            return Err(Error::Header);
        }
        let count = usize::from(u16_at(bytes, 56)?);
        if count == 0 || count > MAX_PROGRAM_HEADERS {
            return Err(Error::Capacity);
        }
        let table = usize::try_from(u64_at(bytes, 32)?).map_err(|_| Error::Bounds)?;
        if table < 64 {
            return Err(Error::Layout);
        }
        let table_size = count.checked_mul(56).ok_or(Error::Bounds)?;
        let end = table.checked_add(table_size).ok_or(Error::Bounds)?;
        bytes.get(table..end).ok_or(Error::Bounds)?;
        let mut image = Self {
            entry: u64_at(bytes, 24)?,
            segments: [Segment::EMPTY; MAX_LOAD_SEGMENTS],
            count: 0,
            pages: 0,
        };
        let mut stack_seen = false;
        for index in 0..count {
            // Table containment above bounds both addition and all fixed field reads.
            let header = table + index * 56;
            let kind = u32_at(bytes, header)?;
            if kind == 0 {
                continue;
            }
            let flags = u32_at(bytes, header + 4)?;
            let offset = u64_at(bytes, header + 8)?;
            let address = u64_at(bytes, header + 16)?;
            // p_paddr and section tables grant no load authority and are ignored.
            let file_size = u64_at(bytes, header + 32)?;
            let memory_size = u64_at(bytes, header + 40)?;
            let align = u64_at(bytes, header + 48)?;
            if kind == 0x6474_e551 {
                if stack_seen
                    || flags != 6
                    || file_size != 0
                    || memory_size != 0
                    || (align > 1 && !align.is_power_of_two())
                {
                    return Err(Error::Unsupported);
                }
                stack_seen = true;
                continue;
            }
            if kind != 1 {
                return Err(Error::Unsupported);
            }
            if image.count == MAX_LOAD_SEGMENTS {
                return Err(Error::Capacity);
            }
            if memory_size == 0 || file_size > memory_size {
                return Err(Error::Layout);
            }
            if (align > 1 && (!align.is_power_of_two() || address % align != offset % align))
                || address % PAGE_SIZE as u64 != offset % PAGE_SIZE as u64
            {
                return Err(Error::Layout);
            }
            let permissions = match flags {
                4 => Permissions::ReadOnly,
                6 => Permissions::ReadWrite,
                5 => Permissions::ReadExecute,
                _ => return Err(Error::Permissions),
            };
            let file_end = offset.checked_add(file_size).ok_or(Error::Bounds)?;
            let file_start = usize::try_from(offset).map_err(|_| Error::Bounds)?;
            let file_end = usize::try_from(file_end).map_err(|_| Error::Bounds)?;
            let file_bytes = bytes.get(file_start..file_end).ok_or(Error::Bounds)?;
            let memory_end = address.checked_add(memory_size).ok_or(Error::Bounds)?;
            let mask = PAGE_SIZE as u64 - 1;
            let page_start = address & !mask;
            let page_end = memory_end.checked_add(mask).ok_or(Error::Bounds)? & !mask;
            if address < USER_IMAGE_START
                || memory_end > USER_IMAGE_END
                || page_start < USER_IMAGE_START
                || page_end > USER_IMAGE_END
            {
                return Err(Error::Bounds);
            }
            if image.segments().iter().any(|prior| {
                prior.virtual_address > address
                    || (page_start < prior.page_end && prior.page_start < page_end)
            }) {
                return Err(Error::Layout);
            }
            let pages = usize::try_from((page_end - page_start) / PAGE_SIZE as u64)
                .map_err(|_| Error::Capacity)?;
            image.pages = image.pages.checked_add(pages).ok_or(Error::Capacity)?;
            if image.pages > MAX_IMAGE_PAGES {
                return Err(Error::Capacity);
            }
            image.segments[image.count] = Segment {
                virtual_address: address,
                memory_size,
                file_bytes,
                permissions,
                page_start,
                page_end,
            };
            image.count += 1;
        }
        if image.count == 0 {
            return Err(Error::Layout);
        }
        let end = image.entry.checked_add(4).ok_or(Error::Entry)?;
        if !image.entry.is_multiple_of(4)
            || !image.segments().iter().any(|segment| {
                segment.permissions.executable()
                    && image.entry >= segment.virtual_address
                    && segment
                        .virtual_address
                        .checked_add(segment.file_bytes.len() as u64)
                        .is_some_and(|file_end| end <= file_end)
            })
        {
            return Err(Error::Entry);
        }
        Ok(image)
    }
}
fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Error> {
    bytes
        .get(offset..offset.checked_add(N).ok_or(Error::Bounds)?)
        .ok_or(Error::Bounds)?
        .try_into()
        .map_err(|_| Error::Bounds)
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(field(bytes, offset)?))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(field(bytes, offset)?))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(field(bytes, offset)?))
}
