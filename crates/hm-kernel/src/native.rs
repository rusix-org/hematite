//! Experimental synchronous native policy; design 0023-native-wire defines the wire.
#![forbid(unsafe_code)]

pub const VERSION: u64 = 1;
pub const WRITE: u64 = 1;
pub const EXIT: u64 = 2;
pub const MAX_WRITE: usize = 256;
pub const BYTE_LIMIT: usize = 4096;
pub const CALL_LIMIT: usize = 64;
pub const STARTUP_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub version: u64,
    pub operation: u64,
    pub argument: u64,
    pub address: u64,
    pub length: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum Status {
    Ok = 0,
    Version = 1,
    Operation = 2,
    Handle = 3,
    Address = 4,
    Length = 5,
    Budget = 6,
    Argument = 7,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub status: Status,
    pub written: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    Exit(u8),
    CallLimit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Reply(Response),
    Stop(Stop),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyError;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationExhausted;

/// Safe policy effects, not hardware authority. The executor supplies a private
/// adapter with a stopped process and boot mappings; it accepts no user callback.
pub trait Host {
    /// Copy the complete readable user range, or fail without console effects.
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), CopyError>;
    /// Returning guarantees complete synchronous output. The concrete UART
    /// adapter must bound polling and terminate on hardware failure, not return OK.
    fn write_console(&mut self, bytes: &[u8]);
}

/// One namespace, retained across runs by the real executor. Creating this safe
/// metadata value does not itself grant a console or any hardware capability.
pub struct Issuer {
    next: u32,
}
impl Default for Issuer {
    fn default() -> Self {
        Self::new()
    }
}
impl Issuer {
    pub const fn new() -> Self {
        Self { next: 1 }
    }
    pub fn issue(&mut self, console: bool) -> Result<Session, GenerationExhausted> {
        if self.next == 0 {
            return Err(GenerationExhausted);
        }
        let handle = (u64::from(self.next) << 32) | 1;
        self.next = self.next.checked_add(1).unwrap_or(0);
        Ok(Session {
            console: console.then_some(handle),
            calls: 0,
            written: 0,
            stopped: None,
        })
    }
}

pub struct Session {
    console: Option<u64>,
    calls: usize,
    written: usize,
    stopped: Option<Stop>,
}
impl Session {
    pub fn calls(&self) -> usize {
        self.calls
    }
    pub fn written(&self) -> usize {
        self.written
    }
    pub fn termination(&self) -> Option<Stop> {
        self.stopped
    }
    pub fn may_resume(&self) -> bool {
        self.stopped.is_none() && self.calls < CALL_LIMIT
    }
    pub fn startup(&self) -> [u8; STARTUP_SIZE] {
        let mut bytes = [0; STARTUP_SIZE];
        bytes[..8].copy_from_slice(b"HEMATITE");
        bytes[8..12].copy_from_slice(&(VERSION as u32).to_le_bytes());
        bytes[12..16].copy_from_slice(&(STARTUP_SIZE as u32).to_le_bytes());
        for (offset, value) in [
            (16, u64::from(self.console.is_some())),
            (24, self.console.unwrap_or(0)),
            (32, MAX_WRITE as u64),
            (40, BYTE_LIMIT as u64),
            (48, CALL_LIMIT as u64),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    pub fn dispatch(&mut self, request: Request, host: &mut impl Host) -> Outcome {
        if let Some(stop) = self.stopped {
            return Outcome::Stop(stop);
        }
        if self.calls >= CALL_LIMIT {
            self.console = None;
            self.stopped = Some(Stop::CallLimit);
            return Outcome::Stop(Stop::CallLimit);
        }
        let Some(calls) = self.calls.checked_add(1) else {
            self.console = None;
            self.stopped = Some(Stop::CallLimit);
            return Outcome::Stop(Stop::CallLimit);
        };
        self.calls = calls;
        let outcome = self.evaluate(request, host);
        if self.calls == CALL_LIMIT && matches!(outcome, Outcome::Reply(_)) {
            self.console = None;
            self.stopped = Some(Stop::CallLimit);
        }
        outcome
    }
    fn evaluate(&mut self, request: Request, host: &mut impl Host) -> Outcome {
        if request.version != VERSION {
            return reply(Status::Version);
        }
        match request.operation {
            WRITE => self.write(request, host),
            EXIT => {
                if request.argument > u8::MAX as u64 || request.address != 0 || request.length != 0
                {
                    return reply(Status::Argument);
                }
                let stop = Stop::Exit(request.argument as u8);
                self.console = None;
                self.stopped = Some(stop);
                Outcome::Stop(stop)
            }
            _ => reply(Status::Operation),
        }
    }
    fn write(&mut self, request: Request, host: &mut impl Host) -> Outcome {
        if self.console != Some(request.argument) {
            return reply(Status::Handle);
        }
        if request.length > MAX_WRITE as u64 {
            return reply(Status::Length);
        }
        let length = request.length as usize;
        let Some(total) = self
            .written
            .checked_add(length)
            .filter(|n| *n <= BYTE_LIMIT)
        else {
            return reply(Status::Budget);
        };
        if length == 0 {
            return reply(Status::Ok);
        }
        let mut bytes = [0; MAX_WRITE];
        if host
            .copy_from_user(request.address, &mut bytes[..length])
            .is_err()
        {
            return reply(Status::Address);
        }
        host.write_console(&bytes[..length]);
        self.written = total;
        Outcome::Reply(Response {
            status: Status::Ok,
            written: length as u64,
        })
    }
}
fn reply(status: Status) -> Outcome {
    Outcome::Reply(Response { status, written: 0 })
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
