//! Non-authoritative owned wire values for experimental task creation; design 0029.
#![forbid(unsafe_code)]

pub const SPAWN: u64 = 3;
pub const WAIT: u64 = 4;
pub const CANCEL: u64 = 5;
pub const SPAWN_SIZE: usize = 64;
pub const GRANT_SIZE: usize = 64;
pub const MAX_INPUT: u64 = 256;
pub const MAX_CHILDREN: u32 = 2;
pub const MAX_CHILD_FRAMES: u64 = 128;
pub const GROUP_FRAMES: u32 = 512;
pub const WORKER_PROGRAM: u32 = 1;
pub const LOOP_PROGRAM: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Length,
    Header,
    Flags,
    Reserved,
    Rights,
    Frames,
    Input,
    Address,
    Handle,
    Limits,
    Completion,
    Arguments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rights {
    Wait,
    WaitCancel,
}
impl Rights {
    pub const fn bits(self) -> u32 {
        match self {
            Self::Wait => 1,
            Self::WaitCancel => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Spawn {
    program: u32,
    input: Option<(u64, u64)>,
    console: bool,
    max_frames: u64,
    rights: Rights,
}
impl Spawn {
    pub fn new(
        program: u32,
        input: Option<(u64, u64)>,
        console: bool,
        max_frames: u64,
        rights: Rights,
    ) -> Result<Self, Error> {
        frames(max_frames)?;
        if let Some((address, length)) = input {
            canonical_input(true, address, length)?;
        }
        Ok(Self {
            program,
            input,
            console,
            max_frames,
            rights,
        })
    }
    pub fn program(self) -> u32 {
        self.program
    }
    pub fn input(self) -> Option<(u64, u64)> {
        self.input
    }
    pub fn console(self) -> bool {
        self.console
    }
    pub fn max_frames(self) -> u64 {
        self.max_frames
    }
    pub fn rights(self) -> Rights {
        self.rights
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != SPAWN_SIZE {
            return Err(Error::Length);
        }
        if word(bytes, 0) != 1 || word(bytes, 4) != SPAWN_SIZE as u32 {
            return Err(Error::Header);
        }
        let flags = word(bytes, 12);
        if flags & !3 != 0 {
            return Err(Error::Flags);
        }
        if bytes[44..].iter().any(|byte| *byte != 0) {
            return Err(Error::Reserved);
        }
        let rights = match word(bytes, 40) {
            1 => Rights::Wait,
            3 => Rights::WaitCancel,
            _ => return Err(Error::Rights),
        };
        let max_frames = wide(bytes, 32);
        frames(max_frames)?;
        let (address, length) = (wide(bytes, 16), wide(bytes, 24));
        let present = flags & 1 != 0;
        canonical_input(present, address, length)?;
        Ok(Self {
            program: word(bytes, 8),
            input: present.then_some((address, length)),
            console: flags & 2 != 0,
            max_frames,
            rights,
        })
    }
    pub fn encode_into(self, output: &mut [u8]) -> Result<(), Error> {
        if output.len() != SPAWN_SIZE {
            return Err(Error::Length);
        }
        output.fill(0);
        put_word(output, 0, 1);
        put_word(output, 4, SPAWN_SIZE as u32);
        put_word(output, 8, self.program);
        put_word(
            output,
            12,
            u32::from(self.input.is_some()) | (u32::from(self.console) << 1),
        );
        if let Some((address, length)) = self.input {
            put_wide(output, 16, address);
            put_wide(output, 24, length);
        }
        put_wide(output, 32, self.max_frames);
        put_word(output, 40, self.rights.bits());
        Ok(())
    }
}

fn frames(value: u64) -> Result<(), Error> {
    if (1..=MAX_CHILD_FRAMES).contains(&value) {
        Ok(())
    } else {
        Err(Error::Frames)
    }
}
fn canonical_input(present: bool, address: u64, length: u64) -> Result<(), Error> {
    if length > MAX_INPUT {
        return Err(Error::Length);
    }
    if (!present && (address != 0 || length != 0)) || (length == 0 && address != 0) {
        return Err(Error::Input);
    }
    if length != 0 && address.checked_add(length).is_none() {
        return Err(Error::Address);
    }
    Ok(())
}

/// An envelope description, never an issued launch capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Grant {
    handle: u64,
    console_allowed: bool,
}
impl Grant {
    pub fn new(handle: u64, console_allowed: bool) -> Result<Self, Error> {
        if handle >> 32 == 0 || handle as u32 != 2 {
            return Err(Error::Handle);
        }
        Ok(Self {
            handle,
            console_allowed,
        })
    }
    pub fn handle(self) -> u64 {
        self.handle
    }
    pub fn console_allowed(self) -> bool {
        self.console_allowed
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != GRANT_SIZE {
            return Err(Error::Length);
        }
        if &bytes[..8] != b"HMTASK\0\0"
            || word(bytes, 8) != 1
            || word(bytes, 12) != GRANT_SIZE as u32
        {
            return Err(Error::Header);
        }
        if wide(bytes, 56) != 0 {
            return Err(Error::Reserved);
        }
        let flags = wide(bytes, 48);
        if flags & !1 != 0 {
            return Err(Error::Flags);
        }
        let grant = Self::new(wide(bytes, 16), flags != 0)?;
        if word(bytes, 24) != MAX_INPUT as u32
            || word(bytes, 28) != MAX_CHILDREN
            || word(bytes, 32) != MAX_CHILD_FRAMES as u32
            || word(bytes, 36) != GROUP_FRAMES
            || word(bytes, 40) != WORKER_PROGRAM
            || word(bytes, 44) != LOOP_PROGRAM
        {
            return Err(Error::Limits);
        }
        Ok(grant)
    }
    pub fn encode_into(self, output: &mut [u8]) -> Result<(), Error> {
        if output.len() != GRANT_SIZE {
            return Err(Error::Length);
        }
        output.fill(0);
        output[..8].copy_from_slice(b"HMTASK\0\0");
        put_word(output, 8, 1);
        put_word(output, 12, GRANT_SIZE as u32);
        put_wide(output, 16, self.handle);
        for (offset, value) in [
            (24, MAX_INPUT as u32),
            (28, MAX_CHILDREN),
            (32, MAX_CHILD_FRAMES as u32),
            (36, GROUP_FRAMES),
            (40, WORKER_PROGRAM),
            (44, LOOP_PROGRAM),
        ] {
            put_word(output, offset, value);
        }
        put_wide(output, 48, u64::from(self.console_allowed));
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Exited(u8),
    Fault,
    Budget,
    CallLimit,
    TurnLimit,
    Cancelled,
    Unavailable,
}
impl Completion {
    pub fn encode(self) -> u64 {
        match self {
            Self::Exited(status) => u64::from(status) << 8,
            Self::Fault => 1,
            Self::Budget => 2,
            Self::CallLimit => 3,
            Self::TurnLimit => 4,
            Self::Cancelled => 5,
            Self::Unavailable => 6,
        }
    }
    pub fn decode(word: u64) -> Result<Self, Error> {
        if word >> 16 != 0 {
            return Err(Error::Completion);
        }
        if word as u8 == 0 {
            return Ok(Self::Exited((word >> 8) as u8));
        }
        match word {
            1 => Ok(Self::Fault),
            2 => Ok(Self::Budget),
            3 => Ok(Self::CallLimit),
            4 => Ok(Self::TurnLimit),
            5 => Ok(Self::Cancelled),
            6 => Ok(Self::Unavailable),
            _ => Err(Error::Completion),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitMode {
    Blocking,
    Poll,
}
impl WaitMode {
    pub fn decode(mode: u64, reserved: u64) -> Result<Self, Error> {
        match (mode, reserved) {
            (0, 0) => Ok(Self::Blocking),
            (1, 0) => Ok(Self::Poll),
            _ => Err(Error::Arguments),
        }
    }
}
pub fn cancel_arguments(address: u64, length: u64) -> Result<(), Error> {
    if address == 0 && length == 0 {
        Ok(())
    } else {
        Err(Error::Arguments)
    }
}

// All callers first check an exact 64-byte extent and use fixed in-range offsets.
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
fn wide(bytes: &[u8], offset: usize) -> u64 {
    u64::from(word(bytes, offset)) | (u64::from(word(bytes, offset + 4)) << 32)
}
fn put_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_wide(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
