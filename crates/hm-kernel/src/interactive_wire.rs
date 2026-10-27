//! Canonical input-grant metadata; it cannot create terminal authority.
#![forbid(unsafe_code)]

pub const READ: u64 = 6;
pub const INPUT_SLOT: u32 = 5;
pub const MAX_READ: usize = 256;
pub const CALL_LIMIT: usize = 4096;
pub const BYTE_LIMIT: usize = 65536;
pub const GRANT_SIZE: usize = 64;
const MAGIC: &[u8; 8] = b"HMINPUT\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Magic,
    Version,
    Size,
    Flags,
    Handle,
    Limits,
    Reserved,
}

/// Copyable wire data only. A matching live, principal-scoped core table entry
/// is required before this token can authorize any operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Grant {
    input_token: u64,
}
impl Grant {
    pub fn new(input_token: u64) -> Result<Self, Error> {
        if input_token as u32 != INPUT_SLOT || input_token >> 32 == 0 {
            return Err(Error::Handle);
        }
        Ok(Self { input_token })
    }
    pub fn input_token(self) -> u64 {
        self.input_token
    }
    /// Wrong size leaves the entire destination unchanged.
    pub fn encode_into(self, output: &mut [u8]) -> Result<(), Error> {
        if output.len() != GRANT_SIZE {
            return Err(Error::Length);
        }
        output.fill(0);
        output[..8].copy_from_slice(MAGIC);
        output[8..12].copy_from_slice(&1u32.to_le_bytes());
        output[12..16].copy_from_slice(&(GRANT_SIZE as u32).to_le_bytes());
        output[16..24].copy_from_slice(&self.input_token.to_le_bytes());
        output[32..40].copy_from_slice(&(MAX_READ as u64).to_le_bytes());
        output[40..48].copy_from_slice(&(CALL_LIMIT as u64).to_le_bytes());
        output[48..56].copy_from_slice(&(BYTE_LIMIT as u64).to_le_bytes());
        Ok(())
    }
    pub fn decode(input: &[u8]) -> Result<Self, Error> {
        let bytes: &[u8; GRANT_SIZE] = input.try_into().map_err(|_| Error::Length)?;
        if &bytes[..8] != MAGIC {
            return Err(Error::Magic);
        }
        if word(bytes, 8) != 1 {
            return Err(Error::Version);
        }
        if word(bytes, 12) != GRANT_SIZE as u32 {
            return Err(Error::Size);
        }
        if doubleword(bytes, 24) != 0 {
            return Err(Error::Flags);
        }
        let grant = Self::new(doubleword(bytes, 16))?;
        if doubleword(bytes, 32) != MAX_READ as u64
            || doubleword(bytes, 40) != CALL_LIMIT as u64
            || doubleword(bytes, 48) != BYTE_LIMIT as u64
        {
            return Err(Error::Limits);
        }
        if doubleword(bytes, 56) != 0 {
            return Err(Error::Reserved);
        }
        Ok(grant)
    }
}
// Called only with the fixed, in-bounds field offsets above after exact sizing.
fn word(bytes: &[u8; GRANT_SIZE], offset: usize) -> u32 {
    let mut value = [0; 4];
    value.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(value)
}
fn doubleword(bytes: &[u8; GRANT_SIZE], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(value)
}
