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
    Denied = 8,
    Resources = 9,
    Pending = 10,
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
        self.issue_profile(console, SessionProfile::Legacy)
    }
    /// Fixed safe policy metadata; actual input authority is issued by the core.
    pub fn issue_interactive(&mut self) -> Result<Session, GenerationExhausted> {
        self.issue_profile(true, SessionProfile::Interactive)
    }
    fn issue_profile(
        &mut self,
        console: bool,
        profile: SessionProfile,
    ) -> Result<Session, GenerationExhausted> {
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
            profile,
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SessionProfile {
    Legacy,
    Interactive,
}

pub struct Session {
    console: Option<u64>,
    calls: usize,
    written: usize,
    stopped: Option<Stop>,
    profile: SessionProfile,
}

/// Input policy has its own admission enum so legacy task callers retain their
/// existing exhaustive interface. No real input handle or range is checked here.
pub enum InputAdmission {
    Complete { session: Session, outcome: Outcome },
    Read(ReadAdmission),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadError {
    Handle,
    Length,
    Address,
}

/// Owns the original accounted session, with no user/host borrow or wake right.
pub struct ReadAdmission {
    session: Session,
    request: Request,
}
impl ReadAdmission {
    pub fn request(&self) -> Request {
        self.request
    }
    pub fn calls(&self) -> usize {
        self.session.calls()
    }
    pub fn written(&self) -> usize {
        self.session.written()
    }
    pub fn reject(self, error: ReadError) -> (Session, Outcome) {
        let status = match error {
            ReadError::Handle => Status::Handle,
            ReadError::Length => Status::Length,
            ReadError::Address => Status::Address,
        };
        (self.session, reply(status))
    }
    pub fn park(self) -> Result<ParkedRead, Self> {
        if !valid_read_capacity(self.request.length) || !self.session.may_resume() {
            return Err(self);
        }
        Ok(ParkedRead {
            session: self.session,
            request: self.request,
        })
    }
}

/// Core owns actual identity, destination validation, charged copying and wake
/// authority. Finishing this safe value alone causes no device or memory effect.
pub struct ParkedRead {
    session: Session,
    request: Request,
}
impl ParkedRead {
    pub fn request(&self) -> Request {
        self.request
    }
    pub fn calls(&self) -> usize {
        self.session.calls()
    }
    pub fn written(&self) -> usize {
        self.session.written()
    }
    pub fn finish(self, count: usize) -> Result<(Session, Outcome), Self> {
        if count == 0
            || count > crate::interactive_wire::MAX_READ
            || !valid_read_capacity(self.request.length)
            || count as u64 > self.request.length
            || !self.session.may_resume()
        {
            return Err(self);
        }
        Ok((
            self.session,
            Outcome::Reply(Response {
                status: Status::Ok,
                written: count as u64,
            }),
        ))
    }
}

fn valid_read_capacity(length: u64) -> bool {
    length > 0 && length <= crate::interactive_wire::MAX_READ as u64
}

/// Safe policy metadata, not permission to operate on a real task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskOperation {
    Spawn,
    Wait,
    Cancel,
}

pub enum Admission {
    Complete { session: Session, outcome: Outcome },
    Task(TaskAdmission),
}

/// Owns the original session while a task request is evaluated. Dropping this
/// value abandons that session; it cannot restore or resume a process.
pub struct TaskAdmission {
    session: Session,
    request: Request,
    operation: TaskOperation,
}
impl TaskAdmission {
    pub fn operation(&self) -> TaskOperation {
        self.operation
    }
    pub fn request(&self) -> Request {
        self.request
    }
    pub fn calls(&self) -> usize {
        self.session.calls()
    }
    pub fn written(&self) -> usize {
        self.session.written()
    }
    pub fn reply(self, response: Response) -> (Session, Outcome) {
        (self.session, Outcome::Reply(response))
    }
    pub fn park_wait(self) -> Result<ParkedSession, Self> {
        if self.operation == TaskOperation::Wait {
            Ok(ParkedSession {
                session: self.session,
            })
        } else {
            Err(self)
        }
    }
}

/// Already-accounted WAIT state. Real task identity and wake authority remain
/// the responsibility of the sealed core; this value retains no user borrow.
pub struct ParkedSession {
    session: Session,
}
impl ParkedSession {
    pub fn calls(&self) -> usize {
        self.session.calls()
    }
    pub fn written(&self) -> usize {
        self.session.written()
    }
    pub fn wake(self, response: Response) -> (Session, Outcome) {
        (self.session, Outcome::Reply(response))
    }
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
        self.stopped.is_none() && self.calls < self.call_limit()
    }
    fn call_limit(&self) -> usize {
        match self.profile {
            SessionProfile::Legacy => CALL_LIMIT,
            SessionProfile::Interactive => crate::interactive_wire::CALL_LIMIT,
        }
    }
    fn byte_limit(&self) -> usize {
        match self.profile {
            SessionProfile::Legacy => BYTE_LIMIT,
            SessionProfile::Interactive => crate::interactive_wire::BYTE_LIMIT,
        }
    }
    pub fn startup(&self) -> [u8; STARTUP_SIZE] {
        let mut bytes = [0; STARTUP_SIZE];
        bytes[..8].copy_from_slice(b"HEMATITE");
        bytes[8..12].copy_from_slice(&(VERSION as u32).to_le_bytes());
        bytes[12..16].copy_from_slice(&(STARTUP_SIZE as u32).to_le_bytes());
        for (offset, value) in [
            (
                16,
                u64::from(self.console.is_some())
                    | if self.profile == SessionProfile::Interactive && self.stopped.is_none() {
                        2
                    } else {
                        0
                    },
            ),
            (24, self.console.unwrap_or(0)),
            (32, MAX_WRITE as u64),
            (40, self.byte_limit() as u64),
            (48, self.call_limit() as u64),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    pub fn dispatch(&mut self, request: Request, host: &mut impl Host) -> Outcome {
        if let Err(stop) = self.begin_call() {
            return Outcome::Stop(stop);
        }
        let outcome = self.evaluate(request, host);
        self.finish_call(outcome)
    }
    /// Opt-in policy entry. Legacy dispatch continues to reject task opcodes.
    /// Task guards own the original counters and grant, not a new allowance.
    pub fn admit_task(mut self, request: Request, host: &mut impl Host) -> Admission {
        if let Err(stop) = self.begin_call() {
            return Admission::Complete {
                session: self,
                outcome: Outcome::Stop(stop),
            };
        }
        let operation = if self.profile == SessionProfile::Legacy && request.version == VERSION {
            match request.operation {
                crate::task_wire::SPAWN => Some(TaskOperation::Spawn),
                crate::task_wire::WAIT => Some(TaskOperation::Wait),
                crate::task_wire::CANCEL => Some(TaskOperation::Cancel),
                _ => None,
            }
        } else {
            None
        };
        if let Some(operation) = operation {
            if self.calls == self.call_limit() {
                self.stop_calls();
                return Admission::Complete {
                    session: self,
                    outcome: Outcome::Stop(Stop::CallLimit),
                };
            }
            return Admission::Task(TaskAdmission {
                session: self,
                request,
                operation,
            });
        }
        let outcome = self.evaluate(request, host);
        let outcome = self.finish_call(outcome);
        Admission::Complete {
            session: self,
            outcome,
        }
    }
    /// Only the fixed interactive profile recognizes READ. Its guard owns the
    /// original allowance; address and actual authority checks remain in core.
    pub fn admit_input(mut self, request: Request, host: &mut impl Host) -> InputAdmission {
        if let Err(stop) = self.begin_call() {
            return InputAdmission::Complete {
                session: self,
                outcome: Outcome::Stop(stop),
            };
        }
        if self.profile == SessionProfile::Interactive
            && request.version == VERSION
            && request.operation == crate::interactive_wire::READ
        {
            if self.calls == self.call_limit() {
                self.stop_calls();
                return InputAdmission::Complete {
                    session: self,
                    outcome: Outcome::Stop(Stop::CallLimit),
                };
            }
            return InputAdmission::Read(ReadAdmission {
                session: self,
                request,
            });
        }
        let outcome = self.evaluate(request, host);
        let outcome = self.finish_call(outcome);
        InputAdmission::Complete {
            session: self,
            outcome,
        }
    }
    fn stop_calls(&mut self) {
        self.console = None;
        self.stopped = Some(Stop::CallLimit);
    }
    fn begin_call(&mut self) -> Result<(), Stop> {
        if let Some(stop) = self.stopped {
            return Err(stop);
        }
        if self.calls >= self.call_limit() {
            self.stop_calls();
            return Err(Stop::CallLimit);
        }
        let Some(calls) = self.calls.checked_add(1) else {
            self.stop_calls();
            return Err(Stop::CallLimit);
        };
        self.calls = calls;
        Ok(())
    }
    fn finish_call(&mut self, outcome: Outcome) -> Outcome {
        if self.calls == self.call_limit() && matches!(outcome, Outcome::Reply(_)) {
            self.stop_calls();
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
            .filter(|n| *n <= self.byte_limit())
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
