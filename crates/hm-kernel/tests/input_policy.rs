#![forbid(unsafe_code)]
use hm_kernel::interactive_wire as wire;
use hm_kernel::native::{
    Admission, CopyError, Host, InputAdmission, Issuer, Outcome, ParkedRead, ReadAdmission,
    ReadError, Request, Response, Session, Status, Stop,
};

#[derive(Default)]
struct RecordingHost {
    copies: usize,
    writes: usize,
    bytes: Vec<u8>,
}
impl Host for RecordingHost {
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), CopyError> {
        self.copies += 1;
        if address != 0x1000 {
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
fn interactive() -> Session {
    Issuer::new().issue_interactive().unwrap()
}
fn word(session: &Session, offset: usize) -> u64 {
    u64::from_le_bytes(session.startup()[offset..offset + 8].try_into().unwrap())
}
fn handle(session: &Session) -> u64 {
    word(session, 24)
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
fn read(capacity: u64) -> Request {
    Request {
        argument: 0x1234_5678_0000_0005,
        address: 0x2000,
        length: capacity,
        ..request(wire::READ)
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
fn reply(status: Status, written: u64) -> Outcome {
    Outcome::Reply(Response { status, written })
}
fn complete(admission: InputAdmission) -> (Session, Outcome) {
    match admission {
        InputAdmission::Complete { session, outcome } => (session, outcome),
        InputAdmission::Read(_) => panic!("unexpected read guard"),
    }
}
fn guard(admission: InputAdmission) -> ReadAdmission {
    match admission {
        InputAdmission::Read(guard) => guard,
        InputAdmission::Complete { .. } => panic!("missing read guard"),
    }
}
fn park(guard: ReadAdmission) -> ParkedRead {
    match guard.park() {
        Ok(owner) => owner,
        Err(_) => panic!("valid park rejected"),
    }
}
fn finish(owner: ParkedRead, count: usize) -> (Session, Outcome) {
    match owner.finish(count) {
        Ok(result) => result,
        Err(_) => panic!("valid finish rejected"),
    }
}
fn attempts(session: &mut Session, count: usize) {
    let mut host = RecordingHost::default();
    for _ in 0..count {
        assert_eq!(
            session.dispatch(write(handle(session), 0), &mut host),
            reply(Status::Ok, 0)
        );
    }
    assert_eq!((host.copies, host.writes), (0, 0));
}
fn assert_stopped(session: &Session, stop: Stop) {
    assert_eq!(session.termination(), Some(stop));
    assert!(!session.may_resume());
    assert_eq!((word(session, 16), word(session, 24)), (0, 0));
    assert_eq!((word(session, 40), word(session, 48)), (65536, 4096));
}

#[test]
fn legacy_input_entry_matches_dispatch() {
    let mut direct = Issuer::new().issue(true).unwrap();
    let mut owned = Issuer::new().issue(true).unwrap();
    let mut a = RecordingHost::default();
    let mut b = RecordingHost::default();
    let token = handle(&direct);
    assert_eq!(
        (word(&direct, 16), word(&direct, 40), word(&direct, 48)),
        (1, 4096, 64)
    );
    let operations = [
        write(token, 3),
        read(1),
        Request {
            version: 2,
            ..read(1)
        },
        request(3),
        write(token, 257),
        write(token ^ 1, 1),
        Request {
            address: u64::MAX,
            ..write(token, 1)
        },
    ];
    for index in 0..70 {
        let operation = operations[index % operations.len()];
        let expected = direct.dispatch(operation, &mut a);
        let (next, actual) = complete(owned.admit_input(operation, &mut b));
        if index == 1 {
            assert_eq!(actual, reply(Status::Operation, 0));
        }
        owned = next;
        assert_eq!(actual, expected);
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
            (&b.bytes, b.copies, b.writes),
            (&a.bytes, a.copies, a.writes)
        );
    }
    assert_eq!(owned.calls(), 64);
    assert_eq!(owned.termination(), Some(Stop::CallLimit));
}

#[test]
fn interactive_startup_limits_and_terminal_flags() {
    let mut session = interactive();
    let bytes = session.startup();
    assert_eq!(&bytes[..16], b"HEMATITE\x01\0\0\0\x40\0\0\0");
    assert_eq!(
        (word(&session, 16), word(&session, 24), word(&session, 32)),
        (3, 0x100000001, 256)
    );
    assert_eq!(
        (word(&session, 40), word(&session, 48), word(&session, 56)),
        (65536, 4096, 0)
    );
    assert_eq!(
        session.dispatch(
            Request {
                argument: 19,
                ..request(2)
            },
            &mut RecordingHost::default()
        ),
        Outcome::Stop(Stop::Exit(19))
    );
    assert_stopped(&session, Stop::Exit(19));
    assert_eq!(session.calls(), 1);
}

#[test]
fn shared_issuer_generations_are_monotonic() {
    let mut issuer = Issuer::new();
    let first = issuer.issue(true).unwrap();
    let second = issuer.issue_interactive().unwrap();
    let absent = issuer.issue(false).unwrap();
    let fourth = issuer.issue_interactive().unwrap();
    assert_eq!(
        (
            handle(&first),
            handle(&second),
            handle(&absent),
            handle(&fourth)
        ),
        (0x100000001, 0x200000001, 0, 0x400000001)
    );
    assert_eq!(
        (
            word(&first, 16),
            word(&second, 16),
            word(&absent, 16),
            word(&fourth, 16)
        ),
        (1, 3, 0, 3)
    );
}

#[test]
fn read_admission_preserves_counters_without_host_effects() {
    let mut session = interactive();
    let mut host = RecordingHost::default();
    assert_eq!(
        session.dispatch(write(handle(&session), 7), &mut host),
        reply(Status::Ok, 7)
    );
    let operation = Request {
        argument: u64::MAX,
        address: u64::MAX,
        ..read(256)
    };
    let guard = guard(session.admit_input(operation, &mut host));
    assert_eq!(
        (guard.request(), guard.calls(), guard.written()),
        (operation, 2, 7)
    );
    // Metadata admission does not claim that the intentionally invalid token or
    // pointer is usable. Actual validation remains the core's responsibility.
    assert_eq!((host.copies, host.writes, host.bytes.len()), (1, 1, 7));
}

#[test]
fn rejection_returns_original_session() {
    let mut session = interactive();
    let mut host = RecordingHost::default();
    let token = handle(&session);
    for (index, (error, status)) in [
        (ReadError::Handle, Status::Handle),
        (ReadError::Length, Status::Length),
        (ReadError::Address, Status::Address),
    ]
    .into_iter()
    .enumerate()
    {
        let admission = guard(session.admit_input(read(256), &mut host));
        let (next, outcome) = admission.reject(error);
        assert_eq!(outcome, reply(status, 0));
        assert_eq!(
            (next.calls(), next.written(), handle(&next)),
            (index + 1, 0, token)
        );
        assert!(next.may_resume());
        session = next;
    }
    assert_eq!((host.copies, host.writes), (0, 0));
}

#[test]
fn invalid_park_returns_guard() {
    for capacity in [0, 257, u64::MAX] {
        let mut host = RecordingHost::default();
        let operation = read(capacity);
        let admitted = guard(interactive().admit_input(operation, &mut host));
        let returned = match admitted.park() {
            Err(owner) => owner,
            Ok(_) => panic!("invalid capacity parked"),
        };
        assert_eq!(
            (returned.request(), returned.calls(), returned.written()),
            (operation, 1, 0)
        );
        let (session, outcome) = returned.reject(ReadError::Length);
        assert_eq!(outcome, reply(Status::Length, 0));
        assert_eq!(session.calls(), 1);
        assert!(session.may_resume());
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn park_finish_preserves_shared_allowances() {
    let mut session = interactive();
    let token = handle(&session);
    let mut host = RecordingHost::default();
    for cycle in 0..100 {
        assert_eq!(
            session.dispatch(write(token, 1), &mut host),
            reply(Status::Ok, 1)
        );
        let operation = read(256);
        let parked = park(guard(session.admit_input(operation, &mut host)));
        assert_eq!(
            (parked.request(), parked.calls(), parked.written()),
            (operation, (cycle + 1) * 2, cycle + 1)
        );
        let (next, outcome) = finish(parked, 256);
        assert_eq!(outcome, reply(Status::Ok, 256));
        assert_eq!(
            (next.calls(), next.written(), handle(&next)),
            ((cycle + 1) * 2, cycle + 1, token)
        );
        assert!(next.may_resume());
        session = next;
    }
    assert_eq!(
        (host.copies, host.writes, host.bytes.len()),
        (100, 100, 100)
    );
}

#[test]
fn invalid_finish_returns_original_parked_owner() {
    let mut session = interactive();
    let mut host = RecordingHost::default();
    assert_eq!(
        session.dispatch(write(handle(&session), 3), &mut host),
        reply(Status::Ok, 3)
    );
    let operation = read(7);
    let mut parked = park(guard(session.admit_input(operation, &mut host)));
    for count in [0, 8, 256, 257, usize::MAX] {
        parked = match parked.finish(count) {
            Err(owner) => owner,
            Ok(_) => panic!("invalid finish accepted"),
        };
        assert_eq!(
            (parked.request(), parked.calls(), parked.written()),
            (operation, 2, 3)
        );
    }
    let (session, outcome) = finish(parked, 7);
    assert_eq!(outcome, reply(Status::Ok, 7));
    assert_eq!((session.calls(), session.written()), (2, 3));
    assert_eq!((host.copies, host.writes), (1, 1));
}

#[test]
fn host_borrow_ends_at_admission() {
    let admitted = {
        let mut host = RecordingHost::default();
        let admitted = guard(interactive().admit_input(read(1), &mut host));
        host.bytes.push(0xa5);
        assert_eq!((host.copies, host.writes), (0, 0));
        admitted
    };
    let (session, outcome) = finish(park(admitted), 1);
    assert_eq!(outcome, reply(Status::Ok, 1));
    assert_eq!(session.calls(), 1);
}

#[test]
fn guard_drop_is_session_local() {
    let mut issuer = Issuer::new();
    let abandoned = issuer.issue_interactive().unwrap();
    let mut survivor = issuer.issue_interactive().unwrap();
    let token = handle(&survivor);
    let mut host = RecordingHost::default();
    {
        let _guard = guard(abandoned.admit_input(read(1), &mut host));
    }
    assert_eq!(
        (survivor.calls(), survivor.written(), handle(&survivor)),
        (0, 0, token)
    );
    assert_eq!(
        survivor.dispatch(write(token, 2), &mut host),
        reply(Status::Ok, 2)
    );
    assert_eq!(handle(&issuer.issue_interactive().unwrap()), 0x300000001);
}

#[test]
fn parked_drop_is_session_local() {
    let mut issuer = Issuer::new();
    let abandoned = issuer.issue_interactive().unwrap();
    let survivor = issuer.issue_interactive().unwrap();
    let mut host = RecordingHost::default();
    {
        let _parked = park(guard(abandoned.admit_input(read(1), &mut host)));
    }
    let (survivor, outcome) = finish(park(guard(survivor.admit_input(read(1), &mut host))), 1);
    assert_eq!(outcome, reply(Status::Ok, 1));
    assert_eq!(
        (survivor.calls(), survivor.written(), handle(&survivor)),
        (1, 0, 0x200000001)
    );
    assert_eq!((host.copies, host.writes), (0, 0));
    assert_eq!(handle(&issuer.issue_interactive().unwrap()), 0x300000001);
}

#[test]
fn recognized_final_read_has_no_effect_or_guard() {
    for capacity in [0, 1, 256, 257, u64::MAX] {
        let mut session = interactive();
        attempts(&mut session, 4094);
        let mut host = RecordingHost::default();
        let pending = park(guard(session.admit_input(read(1), &mut host)));
        assert_eq!((pending.calls(), pending.written()), (4095, 0));
        let (session, delivered) = finish(pending, 1);
        assert_eq!(delivered, reply(Status::Ok, 1));
        assert!(session.may_resume());
        let (session, outcome) = complete(session.admit_input(
            Request {
                argument: 0,
                address: u64::MAX,
                ..read(capacity)
            },
            &mut host,
        ));
        assert_eq!(outcome, Outcome::Stop(Stop::CallLimit));
        assert_eq!(session.calls(), 4096);
        assert_stopped(&session, Stop::CallLimit);
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn malformed_final_read_preserves_reply_then_stop() {
    for (operation, status) in [
        (
            Request {
                version: 2,
                ..read(0)
            },
            Status::Version,
        ),
        (
            Request {
                operation: u64::MAX,
                ..read(0)
            },
            Status::Operation,
        ),
        (
            Request {
                operation: 3,
                ..read(0)
            },
            Status::Operation,
        ),
    ] {
        let mut session = interactive();
        attempts(&mut session, 4095);
        let mut host = RecordingHost::default();
        let (session, outcome) = complete(session.admit_input(operation, &mut host));
        assert_eq!(outcome, reply(status, 0));
        assert_eq!(session.calls(), 4096);
        assert_stopped(&session, Stop::CallLimit);
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn interactive_cross_entrypoints_never_grant_task_authority() {
    for operation in [3, 4, 5, wire::READ] {
        let mut host = RecordingHost::default();
        let mut direct = interactive();
        assert_eq!(
            direct.dispatch(request(operation), &mut host),
            reply(Status::Operation, 0)
        );
        let (task_session, task_outcome) =
            match interactive().admit_task(request(operation), &mut host) {
                Admission::Complete { session, outcome } => (session, outcome),
                Admission::Task(_) => panic!("interactive profile minted task admission"),
            };
        assert_eq!(task_outcome, reply(Status::Operation, 0));
        assert_eq!(task_session.calls(), 1);
        if operation != wire::READ {
            let (input_session, outcome) =
                complete(interactive().admit_input(request(operation), &mut host));
            assert_eq!(outcome, reply(Status::Operation, 0));
            assert_eq!(input_session.calls(), 1);
        }
        assert_eq!((host.copies, host.writes), (0, 0));
    }
    // Legacy task entry still recognizes its existing WAIT capability metadata.
    assert!(matches!(
        Issuer::new()
            .issue(true)
            .unwrap()
            .admit_task(request(4), &mut RecordingHost::default()),
        Admission::Task(_)
    ));
}

#[test]
fn stopped_session_is_sticky() {
    for exited in [false, true] {
        let mut session = interactive();
        let mut host = RecordingHost::default();
        let stop = if exited {
            assert_eq!(
                session.dispatch(
                    Request {
                        argument: 23,
                        ..request(2)
                    },
                    &mut host
                ),
                Outcome::Stop(Stop::Exit(23))
            );
            Stop::Exit(23)
        } else {
            attempts(&mut session, 4096);
            Stop::CallLimit
        };
        let calls = session.calls();
        for _ in 0..3 {
            let (next, outcome) = complete(session.admit_input(read(1), &mut host));
            assert_eq!(outcome, Outcome::Stop(stop));
            session = next;
            assert_eq!(
                session.dispatch(write(0, 1), &mut host),
                Outcome::Stop(stop)
            );
            let admitted = session.admit_task(request(3), &mut host);
            session = match admitted {
                Admission::Complete { session, outcome } => {
                    assert_eq!(outcome, Outcome::Stop(stop));
                    session
                }
                Admission::Task(_) => panic!("stopped owner revived"),
            };
            assert_eq!((session.calls(), session.written()), (calls, 0));
            assert_stopped(&session, stop);
        }
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}

#[test]
fn interactive_output_budget_survives_read_cycles() {
    let mut session = interactive();
    let token = handle(&session);
    let mut host = RecordingHost::default();
    for _ in 0..256 {
        assert_eq!(
            session.dispatch(write(token, 256), &mut host),
            reply(Status::Ok, 256)
        );
        (session, _) = finish(park(guard(session.admit_input(read(256), &mut host))), 256);
    }
    assert_eq!((session.calls(), session.written()), (512, 65536));
    assert_eq!(
        session.dispatch(write(token, 1), &mut host),
        reply(Status::Budget, 0)
    );
    assert_eq!(
        (host.copies, host.writes, host.bytes.len()),
        (256, 256, 65536)
    );
    (session, _) = finish(park(guard(session.admit_input(read(1), &mut host))), 1);
    assert_eq!(session.written(), 65536);
    assert_eq!(
        session.dispatch(write(token, 0), &mut host),
        reply(Status::Ok, 0)
    );
    assert!(session.may_resume());
}

#[test]
fn interactive_last_write_and_exit_rules() {
    for final_operation in 0..4 {
        let mut session = interactive();
        attempts(&mut session, 4095);
        let mut host = RecordingHost::default();
        let (operation, expected, stop, copies) = match final_operation {
            0 => (
                write(handle(&session), 1),
                reply(Status::Ok, 1),
                Stop::CallLimit,
                1,
            ),
            1 => (write(0, 1), reply(Status::Handle, 0), Stop::CallLimit, 0),
            2 => (
                Request {
                    argument: 41,
                    ..request(2)
                },
                Outcome::Stop(Stop::Exit(41)),
                Stop::Exit(41),
                0,
            ),
            _ => (
                Request {
                    argument: 256,
                    ..request(2)
                },
                reply(Status::Argument, 0),
                Stop::CallLimit,
                0,
            ),
        };
        let (session, outcome) = complete(session.admit_input(operation, &mut host));
        assert_eq!(outcome, expected);
        assert_eq!(session.calls(), 4096);
        assert_stopped(&session, stop);
        assert_eq!(
            (host.copies, host.writes, session.written()),
            (copies, copies, copies)
        );
    }
}

#[test]
fn read_finish_requires_original_capacity() {
    for capacity in [1, 2, 255, 256] {
        let mut host = RecordingHost::default();
        let operation = read(capacity);
        let owner = park(guard(interactive().admit_input(operation, &mut host)));
        let owner = match owner.finish(capacity as usize + 1) {
            Err(owner) => owner,
            Ok(_) => panic!("finish exceeded original capacity"),
        };
        assert_eq!(owner.request(), operation);
        let (session, outcome) = finish(owner, capacity as usize);
        assert_eq!(outcome, reply(Status::Ok, capacity));
        assert_eq!((session.calls(), session.written()), (1, 0));
        assert_eq!((host.copies, host.writes), (0, 0));
    }
}
