//! Owned opaque startup input; canonical experimental wire format in design 0027.
#![forbid(unsafe_code)]

pub const MAX_INPUT_SIZE: usize = 256;
pub const EXTENSION_OFFSET: usize = 64;
pub const EXTENSION_SIZE: usize = 4032;
pub const HEADER_SIZE: usize = 24;
pub const PAYLOAD_OFFSET: usize = HEADER_SIZE;
const MAGIC: &[u8; 8] = b"HMINPUT\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Size,
    Magic,
    Version,
    Header,
    Reserved,
    Padding,
}

#[derive(Debug)]
pub struct Input {
    bytes: [u8; MAX_INPUT_SIZE],
    length: usize,
    present: bool,
}
impl Input {
    pub const fn absent() -> Self {
        Self {
            bytes: [0; MAX_INPUT_SIZE],
            length: 0,
            present: false,
        }
    }
    /// Copy before returning; no reference to the source is retained.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_INPUT_SIZE {
            return Err(Error::Length);
        }
        let mut input = Self::absent();
        input.bytes[..bytes.len()].copy_from_slice(bytes);
        input.length = bytes.len();
        input.present = true;
        Ok(input)
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.present.then_some(&self.bytes[..self.length])
    }
    /// Wrong-size destinations remain untouched; valid destinations are fully
    /// overwritten, including padding. No page-sized temporary is constructed.
    pub fn encode_into(&self, extension: &mut [u8]) -> Result<(), Error> {
        if extension.len() != EXTENSION_SIZE {
            return Err(Error::Size);
        }
        extension.fill(0);
        if self.present {
            extension[..8].copy_from_slice(MAGIC);
            extension[8..12].copy_from_slice(&1u32.to_le_bytes());
            extension[12..16].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
            extension[16..20].copy_from_slice(&(self.length as u32).to_le_bytes());
            // Private construction bounds length by MAX_INPUT_SIZE; both end
            // and the fixed header are within the validated destination extent.
            extension[PAYLOAD_OFFSET..PAYLOAD_OFFSET + self.length]
                .copy_from_slice(&self.bytes[..self.length]);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct View<'a> {
    bytes: &'a [u8],
    present: bool,
}
impl<'a> View<'a> {
    pub fn is_present(self) -> bool {
        self.present
    }
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
}

/// Validate the entire extension before exposing an immutable payload borrow.
/// Error precedence for inputs with multiple defects is unspecified.
pub fn decode(extension: &[u8]) -> Result<View<'_>, Error> {
    if extension.len() != EXTENSION_SIZE {
        return Err(Error::Size);
    }
    if extension.iter().all(|byte| *byte == 0) {
        return Ok(View {
            bytes: &extension[..0],
            present: false,
        });
    }
    if extension.get(..8) != Some(MAGIC.as_slice()) {
        return Err(Error::Magic);
    }
    if word(extension, 8)? != 1 {
        return Err(Error::Version);
    }
    if word(extension, 12)? != HEADER_SIZE as u32 {
        return Err(Error::Header);
    }
    if word(extension, 20)? != 0 {
        return Err(Error::Reserved);
    }
    let length = usize::try_from(word(extension, 16)?).map_err(|_| Error::Length)?;
    if length > MAX_INPUT_SIZE {
        return Err(Error::Length);
    }
    let end = PAYLOAD_OFFSET.checked_add(length).ok_or(Error::Length)?;
    let bytes = extension.get(PAYLOAD_OFFSET..end).ok_or(Error::Length)?;
    let tail = extension.get(end..).ok_or(Error::Length)?;
    if tail.iter().any(|byte| *byte != 0) {
        return Err(Error::Padding);
    }
    Ok(View {
        bytes,
        present: true,
    })
}
fn word(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::Size)?;
    let word = bytes.get(offset..end).ok_or(Error::Size)?;
    Ok(u32::from_le_bytes(
        word.try_into().map_err(|_| Error::Size)?,
    ))
}
