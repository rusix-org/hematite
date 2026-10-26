#![forbid(unsafe_code)]
use super::*;

struct Memory {
    input: [u8; MAX_WRITE],
    output: [u8; BYTE_LIMIT],
    used: usize,
    copies: usize,
    writes: usize,
    fail_copy: bool,
}
impl Memory {
    fn new() -> Self {
        Self {
            input: core::array::from_fn(|i| i as u8),
            output: [0; BYTE_LIMIT],
            used: 0,
            copies: 0,
            writes: 0,
            fail_copy: false,
        }
    }
}
impl Host for Memory {
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), CopyError> {
        self.copies += 1;
        if self.fail_copy {
            output.fill(0xaa); // Even a failed adapter copy must never reach output.
            return Err(CopyError);
        }
        let start = address.checked_sub(0x1000).ok_or(CopyError)?;
        let end = start.checked_add(output.len() as u64).ok_or(CopyError)?;
        let start = usize::try_from(start).map_err(|_| CopyError)?;
        let end = usize::try_from(end).map_err(|_| CopyError)?;
        output.copy_from_slice(self.input.get(start..end).ok_or(CopyError)?);
        Ok(())
    }
    fn write_console(&mut self, bytes: &[u8]) {
        self.writes += 1;
        self.output[self.used..self.used + bytes.len()].copy_from_slice(bytes);
        self.used += bytes.len();
    }
}
fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn write(session: &Session, length: u64) -> Request {
    Request {
        version: VERSION,
        operation: WRITE,
        argument: word(&session.startup(), 24),
        address: 0x1000,
        length,
    }
}
fn exit(code: u64) -> Request {
    Request {
        version: VERSION,
        operation: EXIT,
        argument: code,
        address: 0,
        length: 0,
    }
}
fn response(status: Status, written: u64) -> Outcome {
    Outcome::Reply(Response { status, written })
}

#[test]
fn startup_bytes_and_scope() {
    let mut issuer = Issuer::new();
    let first = issuer.issue(true).unwrap();
    let absent = issuer.issue(false).unwrap();
    let third = issuer.issue(true).unwrap();
    let bytes = first.startup();
    assert_eq!(&bytes[..16], b"HEMATITE\x01\0\0\0\x40\0\0\0");
    assert_eq!((word(&bytes, 16), word(&bytes, 24)), (1, 0x1_0000_0001));
    assert_eq!(
        (
            word(&bytes, 32),
            word(&bytes, 40),
            word(&bytes, 48),
            word(&bytes, 56)
        ),
        (256, 4096, 64, 0)
    );
    assert_eq!(
        (word(&absent.startup(), 16), word(&absent.startup(), 24)),
        (0, 0)
    );
    assert_eq!(word(&third.startup(), 24), 0x3_0000_0001);
    assert_eq!(
        (first.calls(), first.written(), first.may_resume()),
        (0, 0, true)
    );
}
#[test]
fn generations_retire_without_wrap() {
    let mut issuer = Issuer { next: u32::MAX - 1 };
    assert_eq!(
        word(&issuer.issue(true).unwrap().startup(), 24),
        0xffff_fffe_0000_0001
    );
    assert_eq!(
        word(&issuer.issue(true).unwrap().startup(), 24),
        0xffff_ffff_0000_0001
    );
    for _ in 0..3 {
        assert!(matches!(issuer.issue(true), Err(GenerationExhausted)));
    }
}
#[test]
fn stale_foreign_absent_and_wrong_slot_handles() {
    let mut issuer = Issuer::new();
    let first = issuer.issue(true).unwrap();
    let mut second = issuer.issue(true).unwrap();
    let mut absent = issuer.issue(false).unwrap();
    let mut host = Memory::new();
    let base = write(&second, 1);
    for handle in [
        0,
        1,
        word(&first.startup(), 24),
        base.argument ^ 3,
        u64::MAX,
    ] {
        assert_eq!(
            second.dispatch(
                Request {
                    argument: handle,
                    ..base
                },
                &mut host
            ),
            response(Status::Handle, 0)
        );
    }
    assert_eq!(
        absent.dispatch(base, &mut host),
        response(Status::Handle, 0)
    );
    assert_eq!((host.copies, host.writes, second.written()), (0, 0, 0));
}
#[test]
fn validation_precedence_has_no_effects() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    let mut request = Request {
        version: 0,
        operation: u64::MAX,
        argument: 0,
        address: u64::MAX,
        length: u64::MAX,
    };
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Version, 0)
    );
    request.version = VERSION;
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Operation, 0)
    );
    request.operation = WRITE;
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Handle, 0)
    );
    request.argument = word(&session.startup(), 24);
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Length, 0)
    );
    request.length = 1;
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Address, 0)
    );
    assert_eq!(
        (session.calls(), session.written(), host.copies, host.writes),
        (5, 0, 1, 0)
    );
}
#[test]
fn zero_and_maximum_write_are_exact() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    let request = Request {
        address: u64::MAX,
        ..write(&session, 0)
    };
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Ok, 0)
    );
    assert_eq!((host.copies, host.writes), (0, 0));
    let request = write(&session, 256);
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Ok, 256)
    );
    assert_eq!(&host.output[..256], &host.input);
    assert_eq!((session.written(), host.used), (256, 256));
    assert_eq!(
        session.dispatch(
            Request {
                length: 257,
                ..request
            },
            &mut host
        ),
        response(Status::Length, 0)
    );
    assert_eq!((host.copies, host.writes, session.written()), (1, 1, 256));
}
#[test]
fn failed_copy_never_emits_or_charges() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    host.fail_copy = true;
    let request = write(&session, 256);
    assert_eq!(
        session.dispatch(request, &mut host),
        response(Status::Address, 0)
    );
    assert_eq!(
        (host.copies, host.writes, host.used, session.written()),
        (1, 0, 0, 0)
    );
    host.fail_copy = false;
    for address in [0, 0x10ff, u64::MAX] {
        assert_eq!(
            session.dispatch(Request { address, ..request }, &mut host),
            response(Status::Address, 0)
        );
    }
    assert_eq!((host.writes, session.written()), (0, 0));
}
#[test]
fn byte_budget_exhaustion_is_independent_of_calls() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    let request = write(&session, 256);
    for _ in 0..16 {
        assert_eq!(
            session.dispatch(request, &mut host),
            response(Status::Ok, 256)
        );
    }
    assert_eq!((session.written(), host.used), (4096, 4096));
    assert!(session.may_resume());
    let calls = host.copies;
    assert_eq!(
        session.dispatch(
            Request {
                address: u64::MAX,
                length: 1,
                ..request
            },
            &mut host
        ),
        response(Status::Budget, 0)
    );
    assert_eq!(host.copies, calls);
    assert_eq!(
        session.dispatch(
            Request {
                address: u64::MAX,
                length: 0,
                ..request
            },
            &mut host
        ),
        response(Status::Ok, 0)
    );
    assert_eq!(
        (session.calls(), session.written(), host.used),
        (18, 4096, 4096)
    );
}
#[test]
fn invalid_storm_exhausts_calls_and_stops_effects() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    let request = Request {
        version: u64::MAX,
        ..write(&session, 1)
    };
    for _ in 0..64 {
        assert_eq!(
            session.dispatch(request, &mut host),
            response(Status::Version, 0)
        );
    }
    assert!(!session.may_resume());
    assert!(session.termination().is_some());
    assert_eq!(word(&session.startup(), 24), 0);
    assert_eq!(session.termination(), Some(Stop::CallLimit));
    for _ in 0..3 {
        assert_eq!(
            session.dispatch(exit(0), &mut host),
            Outcome::Stop(Stop::CallLimit)
        );
    }
    assert_eq!(
        (session.calls(), session.written(), host.copies, host.writes),
        (64, 0, 0, 0)
    );
}
#[test]
fn last_call_can_exit_and_terminal_result_is_stable() {
    let mut session = Issuer::new().issue(true).unwrap();
    let mut host = Memory::new();
    let request = write(&session, 0);
    for _ in 0..63 {
        assert_eq!(
            session.dispatch(request, &mut host),
            response(Status::Ok, 0)
        );
    }
    assert_eq!(
        session.dispatch(exit(255), &mut host),
        Outcome::Stop(Stop::Exit(255))
    );
    assert!(!session.may_resume());
    assert!(session.termination().is_some());
    assert_eq!(word(&session.startup(), 24), 0);
    assert_eq!(
        session.dispatch(write(&session, 1), &mut host),
        Outcome::Stop(Stop::Exit(255))
    );
    assert_eq!((session.calls(), host.copies, host.writes), (64, 0, 0));
}
#[test]
fn invalid_exit_is_recoverable_but_success_revokes_execution() {
    let mut session = Issuer::new().issue(false).unwrap();
    let mut host = Memory::new();
    for request in [
        exit(256),
        exit(u64::MAX),
        Request {
            address: 1,
            ..exit(0)
        },
        Request {
            length: 1,
            ..exit(0)
        },
    ] {
        assert_eq!(
            session.dispatch(request, &mut host),
            response(Status::Argument, 0)
        );
        assert!(session.may_resume());
    }
    assert_eq!(
        session.dispatch(exit(0), &mut host),
        Outcome::Stop(Stop::Exit(0))
    );
    assert_eq!(
        session.dispatch(exit(200), &mut host),
        Outcome::Stop(Stop::Exit(0))
    );
    assert_eq!(
        (session.calls(), session.written(), host.copies, host.writes),
        (5, 0, 0, 0)
    );
}
