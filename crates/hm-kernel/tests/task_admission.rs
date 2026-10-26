#![forbid(unsafe_code)]
use hm_kernel::native::{
    Admission, CopyError, Host, Issuer, Outcome, Request, Response, Session, Status, Stop,
    TaskAdmission, TaskOperation,
};

#[derive(Default)]
struct RecordingHost {
    copies: usize,
    writes: usize,
    bytes: Vec<u8>,
    reject_copy: bool,
}
impl Host for RecordingHost {
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), CopyError> {
        self.copies += 1;
        if self.reject_copy || address != 0x1000 {
            return Err(CopyError);
        }
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = index as u8;
        }
        Ok(())
    }
    fn write_console(&mut self, bytes: &[u8]) {
        self.writes += 1;
        self.bytes.extend_from_slice(bytes);
    }
}
fn session() -> Session {
    Issuer::new().issue(true).unwrap()
}
fn handle(session: &Session) -> u64 {
    u64::from_le_bytes(session.startup()[24..32].try_into().unwrap())
}
fn request(operation: u64) -> Request {
    Request {
        version: 1,
        operation,
        argument: 0,
        address: 0,
        length: 0,
    }
}
fn write(token: u64, length: u64) -> Request {
    Request {
        argument: token,
        address: 0x1000,
        length,
        ..request(1)
    }
}
fn response(status: Status, written: u64) -> Response {
    Response { status, written }
}
fn complete(admission: Admission) -> (Session, Outcome) {
    match admission {
        Admission::Complete { session, outcome } => (session, outcome),
        Admission::Task(_) => panic!("expected completed legacy/terminal admission"),
    }
}
fn task(admission: Admission) -> TaskAdmission {
    match admission {
        Admission::Task(guard) => guard,
        Admission::Complete { .. } => panic!("expected owned task guard"),
    }
}
fn consume_attempts(session: &mut Session, count: usize) {
    let mut host = RecordingHost::default();
    for _ in 0..count {
        assert_eq!(
            session.dispatch(write(handle(session), 0), &mut host),
            Outcome::Reply(response(Status::Ok, 0))
        );
    }
    assert_eq!((host.copies, host.writes), (0, 0));
}

#[test]
fn legacy_dispatch_parity() {
    let mut direct = session();
    let mut owned = session();
    let mut first = RecordingHost::default();
    let mut second = RecordingHost::default();
    let token = handle(&direct);
    let requests = [
        (write(token, 3), Outcome::Reply(response(Status::Ok, 3))),
        (
            write(token ^ 1, 1),
            Outcome::Reply(response(Status::Handle, 0)),
        ),
        (
            write(token, 257),
            Outcome::Reply(response(Status::Length, 0)),
        ),
        (
            Request {
                address: u64::MAX,
                ..write(token, 1)
            },
            Outcome::Reply(response(Status::Address, 0)),
        ),
        (
            Request {
                version: 2,
                ..write(token, 1)
            },
            Outcome::Reply(response(Status::Version, 0)),
        ),
        (request(255), Outcome::Reply(response(Status::Operation, 0))),
        (
            Request {
                argument: 256,
                ..request(2)
            },
            Outcome::Reply(response(Status::Argument, 0)),
        ),
        (
            Request {
                argument: 17,
                ..request(2)
            },
            Outcome::Stop(Stop::Exit(17)),
        ),
    ];
    for (request, expected) in requests {
        assert_eq!(direct.dispatch(request, &mut first), expected);
        let (returned, outcome) = complete(owned.admit_task(request, &mut second));
        owned = returned;
        assert_eq!(outcome, expected);
        assert_eq!(
            (
                owned.calls(),
                owned.written(),
                owned.termination(),
                owned.startup()
            ),
            (
                direct.calls(),
                direct.written(),
                direct.termination(),
                direct.startup()
            )
        );
        assert_eq!(
            (&second.bytes, second.copies, second.writes),
            (&first.bytes, first.copies, first.writes)
        );
    }
}

#[test]
fn legacy_limit_parity() {
    for variant in 0..5 {
        let mut direct = session();
        let mut owned = session();
        consume_attempts(&mut direct, 63);
        consume_attempts(&mut owned, 63);
        let token = handle(&direct);
        let (request, expected, stopped) = match variant {
            0 => (
                write(token, 1),
                Outcome::Reply(response(Status::Ok, 1)),
                Stop::CallLimit,
            ),
            1 => (
                Request {
                    argument: 7,
                    ..request(2)
                },
                Outcome::Stop(Stop::Exit(7)),
                Stop::Exit(7),
            ),
            2 => (
                Request {
                    argument: 256,
                    ..request(2)
                },
                Outcome::Reply(response(Status::Argument, 0)),
                Stop::CallLimit,
            ),
            3 => (
                request(255),
                Outcome::Reply(response(Status::Operation, 0)),
                Stop::CallLimit,
            ),
            _ => (
                Request {
                    version: 2,
                    ..request(1)
                },
                Outcome::Reply(response(Status::Version, 0)),
                Stop::CallLimit,
            ),
        };
        let mut first = RecordingHost::default();
        let mut second = RecordingHost::default();
        assert_eq!(direct.dispatch(request, &mut first), expected);
        let (mut returned, outcome) = complete(owned.admit_task(request, &mut second));
        assert_eq!(outcome, expected);
        assert_eq!(
            (
                returned.calls(),
                returned.termination(),
                returned.may_resume()
            ),
            (64, Some(stopped), false)
        );
        assert_eq!(
            returned.dispatch(write(token, 1), &mut second),
            Outcome::Stop(stopped)
        );
        assert_eq!(
            (second.copies, second.writes, second.bytes),
            (first.copies, first.writes, first.bytes)
        );
    }
}

#[test]
fn all_task_operations() {
    for (operation, expected) in [
        (3, TaskOperation::Spawn),
        (4, TaskOperation::Wait),
        (5, TaskOperation::Cancel),
    ] {
        let mut host = RecordingHost::default();
        let request = request(operation);
        let guard = task(session().admit_task(request, &mut host));
        assert_eq!(guard.operation(), expected);
        assert_eq!(guard.request(), request);
        assert_eq!((guard.calls(), guard.written()), (1, 0));
        assert_eq!((host.copies, host.writes), (0, 0));
        let (session, outcome) = guard.reply(response(Status::Argument, 0));
        assert_eq!(outcome, Outcome::Reply(response(Status::Argument, 0)));
        assert!(session.may_resume());
        // Legacy entry remains closed to task operations.
        let mut legacy = Issuer::new().issue(true).unwrap();
        assert_eq!(
            legacy.dispatch(request, &mut host),
            Outcome::Reply(response(Status::Operation, 0))
        );
    }
}

#[test]
fn malformed_task_arguments_are_deferred() {
    for operation in 3..=5 {
        let request = Request {
            argument: u64::MAX,
            address: u64::MAX,
            length: u64::MAX,
            ..request(operation)
        };
        let mut host = RecordingHost {
            reject_copy: true,
            ..RecordingHost::default()
        };
        let guard = task(session().admit_task(request, &mut host));
        assert_eq!(guard.request(), request);
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn shared_attempt_counter() {
    let mut session = session();
    let mut host = RecordingHost::default();
    for round in 0..21 {
        assert_eq!(
            session.dispatch(write(handle(&session), 0), &mut host),
            Outcome::Reply(response(Status::Ok, 0))
        );
        let guard = task(session.admit_task(request(4), &mut host));
        let parked = match guard.park_wait() {
            Ok(parked) => parked,
            Err(_) => panic!("WAIT did not park"),
        };
        let (returned, _) = parked.wake(response(Status::Ok, 0));
        let guard = task(returned.admit_task(request(5), &mut host));
        let (returned, _) = guard.reply(response(Status::Argument, 0));
        session = returned;
        assert_eq!(session.calls(), (round + 1) * 3);
    }
    let (session, outcome) = complete(session.admit_task(request(3), &mut host));
    assert_eq!(
        (session.calls(), session.termination(), outcome),
        (64, Some(Stop::CallLimit), Outcome::Stop(Stop::CallLimit))
    );
    assert_eq!((host.copies, host.writes), (0, 0));
}

#[test]
fn valid_task_limit_has_no_host_effect() {
    for operation in 3..=5 {
        let mut session = session();
        consume_attempts(&mut session, 63);
        let mut host = RecordingHost::default();
        let bad = Request {
            argument: u64::MAX,
            address: u64::MAX,
            length: u64::MAX,
            ..request(operation)
        };
        let (session, outcome) = complete(session.admit_task(bad, &mut host));
        assert_eq!(outcome, Outcome::Stop(Stop::CallLimit));
        assert_eq!(
            (
                session.calls(),
                session.written(),
                session.termination(),
                session.may_resume()
            ),
            (64, 0, Some(Stop::CallLimit), false)
        );
        assert_eq!((host.copies, host.writes), (0, 0));
        assert!(host.bytes.is_empty());
    }
}

#[test]
fn malformed_limit_preserves_legacy_reply() {
    for (bad, status) in [
        (
            Request {
                version: 0,
                ..request(3)
            },
            Status::Version,
        ),
        (
            Request {
                version: 0,
                ..request(255)
            },
            Status::Version,
        ),
        (request(255), Status::Operation),
    ] {
        let mut session = session();
        consume_attempts(&mut session, 63);
        let mut host = RecordingHost::default();
        let (session, outcome) = complete(session.admit_task(bad, &mut host));
        assert_eq!(outcome, Outcome::Reply(response(status, 0)));
        assert_eq!(
            (session.calls(), session.termination(), session.may_resume()),
            (64, Some(Stop::CallLimit), false)
        );
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn guard_at_attempt_63() {
    for operation in 3..=5 {
        let mut session = session();
        consume_attempts(&mut session, 62);
        let mut host = RecordingHost::default();
        let guard = task(session.admit_task(request(operation), &mut host));
        assert_eq!(guard.calls(), 63);
        let (session, _) = if operation == 4 {
            let parked = match guard.park_wait() {
                Ok(value) => value,
                Err(_) => panic!("WAIT rejected"),
            };
            assert_eq!(parked.calls(), 63);
            parked.wake(response(Status::Ok, 0))
        } else {
            guard.reply(response(Status::Argument, 0))
        };
        assert_eq!(session.calls(), 63);
        assert!(session.may_resume());
        let (_, outcome) = complete(session.admit_task(request(operation), &mut host));
        assert_eq!(outcome, Outcome::Stop(Stop::CallLimit));
    }
}

#[test]
fn reply_preserves_session() {
    let mut session = session();
    let before = session.startup();
    let mut host = RecordingHost::default();
    session.dispatch(write(handle(&session), 7), &mut host);
    let guard = task(session.admit_task(request(3), &mut host));
    assert_eq!((guard.calls(), guard.written()), (2, 7));
    let reply = response(Status::Ok, 0x1234_0000_0003);
    let (mut session, outcome) = guard.reply(reply);
    assert_eq!(outcome, Outcome::Reply(reply));
    assert_eq!(
        (session.calls(), session.written(), session.startup()),
        (2, 7, before)
    );
    assert_eq!((host.copies, host.writes, host.bytes.len()), (1, 1, 7));
    assert_eq!(
        session.dispatch(write(handle(&session), 1), &mut host),
        Outcome::Reply(response(Status::Ok, 1))
    );
    assert_eq!((session.calls(), session.written()), (3, 8));
}

#[test]
fn wait_park_wake_preserves_session() {
    let mut session = session();
    let before = session.startup();
    let mut host = RecordingHost::default();
    session.dispatch(write(handle(&session), 9), &mut host);
    let guard = task(session.admit_task(request(4), &mut host));
    let parked = match guard.park_wait() {
        Ok(value) => value,
        Err(_) => panic!("WAIT rejected"),
    };
    assert_eq!((parked.calls(), parked.written()), (2, 9));
    let reply = response(Status::Ok, 0xff00);
    let (session, outcome) = parked.wake(reply);
    assert_eq!(outcome, Outcome::Reply(reply));
    assert_eq!(
        (
            session.calls(),
            session.written(),
            session.startup(),
            session.termination()
        ),
        (2, 9, before, None)
    );
    assert_eq!((host.copies, host.writes, host.bytes.len()), (1, 1, 9));
}

#[test]
fn byte_budget_survives_owned_transitions() {
    let mut session = session();
    let mut host = RecordingHost::default();
    for _ in 0..16 {
        assert_eq!(
            session.dispatch(write(handle(&session), 256), &mut host),
            Outcome::Reply(response(Status::Ok, 256))
        );
    }
    let guard = task(session.admit_task(request(3), &mut host));
    let (session, _) = guard.reply(response(Status::Argument, 123));
    let guard = task(session.admit_task(request(4), &mut host));
    let parked = match guard.park_wait() {
        Ok(value) => value,
        Err(_) => panic!("WAIT rejected"),
    };
    assert_eq!((parked.calls(), parked.written()), (18, 4096));
    let (mut session, _) = parked.wake(response(Status::Ok, 123));
    assert_eq!(
        session.dispatch(write(handle(&session), 1), &mut host),
        Outcome::Reply(response(Status::Budget, 0))
    );
    assert_eq!(
        session.dispatch(write(handle(&session), 0), &mut host),
        Outcome::Reply(response(Status::Ok, 0))
    );
    assert_eq!((session.calls(), session.written()), (20, 4096));
    assert_eq!((host.copies, host.writes, host.bytes.len()), (16, 16, 4096));
}

#[test]
fn non_wait_park_returns_guard() {
    for operation in [3, 5] {
        let mut session = session();
        let before = session.startup();
        let mut host = RecordingHost::default();
        session.dispatch(write(handle(&session), 5), &mut host);
        let request = Request {
            argument: 123,
            address: 456,
            length: 789,
            ..request(operation)
        };
        let guard = task(session.admit_task(request, &mut host));
        let guard = match guard.park_wait() {
            Err(guard) => guard,
            Ok(_) => panic!("non-WAIT parked"),
        };
        assert_eq!(
            (guard.request(), guard.calls(), guard.written()),
            (request, 2, 5)
        );
        let (session, _) = guard.reply(response(Status::Argument, 0));
        assert_eq!(
            (session.startup(), session.calls(), session.written()),
            (before, 2, 5)
        );
    }
}

#[test]
fn host_borrow_ends_at_admission() {
    let guard = {
        let mut local_host = RecordingHost::default();
        let guard = task(session().admit_task(request(4), &mut local_host));
        // This mutable use and the end of scope are incompatible with a retained Host borrow.
        local_host.bytes.push(99);
        assert_eq!((local_host.copies, local_host.writes), (0, 0));
        guard
    };
    let parked = match guard.park_wait() {
        Ok(value) => value,
        Err(_) => panic!("WAIT rejected"),
    };
    let (mut session, _) = parked.wake(response(Status::Ok, 0));
    let mut other_host = RecordingHost::default();
    assert_eq!(
        session.dispatch(write(handle(&session), 1), &mut other_host),
        Outcome::Reply(response(Status::Ok, 1))
    );
    assert_eq!((other_host.copies, other_host.writes), (1, 1));
}

#[test]
fn stopped_session_is_sticky() {
    for stopped in [Stop::Exit(19), Stop::CallLimit] {
        let mut session = session();
        let mut host = RecordingHost::default();
        if stopped == Stop::CallLimit {
            consume_attempts(&mut session, 64);
        } else {
            assert_eq!(
                session.dispatch(
                    Request {
                        argument: 19,
                        ..request(2)
                    },
                    &mut host
                ),
                Outcome::Stop(stopped)
            );
        }
        let calls = session.calls();
        for operation in [1, 2, 3, 4, 5, 255] {
            let (returned, outcome) = complete(session.admit_task(request(operation), &mut host));
            session = returned;
            assert_eq!(
                (outcome, session.calls(), session.termination()),
                (Outcome::Stop(stopped), calls, Some(stopped))
            );
            assert!(!session.may_resume());
        }
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn dropped_guard_is_session_local() {
    let mut issuer = Issuer::new();
    let first = issuer.issue(true).unwrap();
    let mut second = issuer.issue(true).unwrap();
    let before = second.startup();
    let mut host = RecordingHost::default();
    {
        let guard = task(first.admit_task(request(3), &mut host));
        assert_eq!(guard.calls(), 1);
    }
    assert_eq!(
        (second.calls(), second.written(), second.startup()),
        (0, 0, before)
    );
    assert_eq!(
        second.dispatch(write(handle(&second), 2), &mut host),
        Outcome::Reply(response(Status::Ok, 2))
    );
    assert_eq!((second.calls(), second.written()), (1, 2));
    assert_eq!(handle(&issuer.issue(true).unwrap()), 0x3_0000_0001);
}

#[test]
fn parked_drop_is_session_local() {
    let mut issuer = Issuer::new();
    let first = issuer.issue(true).unwrap();
    let second = issuer.issue(true).unwrap();
    let before = second.startup();
    let mut host = RecordingHost::default();
    {
        let guard = task(first.admit_task(request(4), &mut host));
        let parked = match guard.park_wait() {
            Ok(value) => value,
            Err(_) => panic!("WAIT rejected"),
        };
        assert_eq!((parked.calls(), parked.written()), (1, 0));
    }
    let guard = task(second.admit_task(request(5), &mut host));
    let (session, _) = guard.reply(response(Status::Argument, 0));
    assert_eq!(
        (session.startup(), session.calls(), session.written()),
        (before, 1, 0)
    );
    assert_eq!((host.copies, host.writes), (0, 0));
    assert_eq!(handle(&issuer.issue(true).unwrap()), 0x3_0000_0001);
}
