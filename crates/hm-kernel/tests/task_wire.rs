#![forbid(unsafe_code)]
use hm_kernel::task_wire::{self, Completion, Error, Grant, Rights, Spawn, WaitMode};

// Independent wire examples, including non-palindromic multibyte values.
const SPAWN: [u8; 64] = [
    1, 0, 0, 0, 64, 0, 0, 0, 0xef, 0xcd, 0xab, 0x89, 3, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1, 0, 1, 0,
    0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0,
];
const GRANT: [u8; 64] = [
    b'H', b'M', b'T', b'A', b'S', b'K', 0, 0, 1, 0, 0, 0, 64, 0, 0, 0, 2, 0, 0, 0, 0x78, 0x56,
    0x34, 0x12, 0, 1, 0, 0, 2, 0, 0, 0, 128, 0, 0, 0, 0, 2, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn wire_constants() {
    assert_eq!(
        (task_wire::SPAWN, task_wire::WAIT, task_wire::CANCEL),
        (3, 4, 5)
    );
    assert_eq!((task_wire::SPAWN_SIZE, task_wire::GRANT_SIZE), (64, 64));
    assert_eq!((task_wire::MAX_INPUT, task_wire::MAX_CHILDREN), (256, 2));
    assert_eq!(
        (task_wire::MAX_CHILD_FRAMES, task_wire::GROUP_FRAMES),
        (128, 512)
    );
    assert_eq!((task_wire::WORKER_PROGRAM, task_wire::LOOP_PROGRAM), (1, 2));
    assert_eq!((Rights::Wait.bits(), Rights::WaitCancel.bits()), (1, 3));
}

#[test]
fn spawn_literal() {
    let spawn = Spawn::decode(&SPAWN).unwrap();
    assert_eq!(spawn.program(), 0x89ab_cdef);
    assert_eq!(spawn.input(), Some((0x0102_0304_0506_0708, 256)));
    assert!(spawn.console());
    assert_eq!(spawn.max_frames(), 128);
    assert_eq!(spawn.rights(), Rights::WaitCancel);
    let constructed = Spawn::new(
        0x89ab_cdef,
        Some((0x0102_0304_0506_0708, 256)),
        true,
        128,
        Rights::WaitCancel,
    )
    .unwrap();
    assert_eq!(spawn, constructed);
    let mut encoded = [0xa5; 64];
    constructed.encode_into(&mut encoded).unwrap();
    assert_eq!(encoded, SPAWN);
}

#[test]
fn spawn_input_lengths_and_presence() {
    for length in 0..=256 {
        let address = if length == 0 { 0 } else { 0x1234 };
        let spawn = Spawn::new(1, Some((address, length)), false, 1, Rights::Wait).unwrap();
        let mut wire = SPAWN;
        spawn.encode_into(&mut wire).unwrap();
        assert_eq!(wire[12], 1);
        assert_eq!(&wire[16..24], &address.to_le_bytes());
        assert_eq!(&wire[24..32], &length.to_le_bytes());
        assert_eq!(
            Spawn::decode(&wire).unwrap().input(),
            Some((address, length))
        );
    }
    let absent = Spawn::new(1, None, false, 1, Rights::Wait).unwrap();
    let empty = Spawn::new(1, Some((0, 0)), false, 1, Rights::Wait).unwrap();
    let mut a = [0xff; 64];
    let mut b = [0xff; 64];
    absent.encode_into(&mut a).unwrap();
    empty.encode_into(&mut b).unwrap();
    assert_eq!(a[12], 0);
    assert_eq!(b[12], 1);
    assert_eq!(&a[16..32], &[0; 16]);
    assert_eq!(&b[16..32], &[0; 16]);
    assert_eq!(Spawn::decode(&a).unwrap().input(), None);
    assert_eq!(Spawn::decode(&b).unwrap().input(), Some((0, 0)));
    let mut console_only = [0; 64];
    Spawn::new(1, None, true, 1, Rights::Wait)
        .unwrap()
        .encode_into(&mut console_only)
        .unwrap();
    assert_eq!(console_only[12], 2);
    assert!(Spawn::decode(&console_only).unwrap().console());
    assert_eq!(Spawn::decode(&console_only).unwrap().input(), None);
    a[16] = 1;
    assert_eq!(Spawn::decode(&a), Err(Error::Input));
    a[16] = 0;
    a[24] = 1;
    assert_eq!(Spawn::decode(&a), Err(Error::Input));
    assert_eq!(
        Spawn::new(1, Some((1, 0)), false, 1, Rights::Wait),
        Err(Error::Input)
    );
}

#[test]
fn spawn_frames_and_range_bounds() {
    for frames in [1, 128] {
        assert!(Spawn::new(0, Some((0, 1)), false, frames, Rights::Wait).is_ok());
    }
    for frames in [0, 129, u64::MAX] {
        assert_eq!(
            Spawn::new(1, None, false, frames, Rights::Wait),
            Err(Error::Frames)
        );
        let mut wire = SPAWN;
        wire[32..40].copy_from_slice(&frames.to_le_bytes());
        assert_eq!(Spawn::decode(&wire), Err(Error::Frames));
    }
    assert!(Spawn::new(1, Some((u64::MAX - 256, 256)), false, 1, Rights::Wait).is_ok());
    for (address, length, error) in [
        (u64::MAX - 255, 256, Error::Address),
        (u64::MAX, 1, Error::Address),
        (0, 257, Error::Length),
        (0, u64::MAX, Error::Length),
    ] {
        assert_eq!(
            Spawn::new(1, Some((address, length)), false, 1, Rights::Wait),
            Err(error)
        );
        let mut wire = SPAWN;
        wire[16..24].copy_from_slice(&address.to_le_bytes());
        wire[24..32].copy_from_slice(&length.to_le_bytes());
        assert_eq!(Spawn::decode(&wire), Err(error));
    }
}

#[test]
fn spawn_flags_reserved_and_rights() {
    for offset in 0..8 {
        let mut wire = SPAWN;
        wire[offset] ^= 1;
        assert_eq!(Spawn::decode(&wire), Err(Error::Header));
    }
    for bit in 2..32 {
        let mut wire = SPAWN;
        wire[12..16].copy_from_slice(&(3 | 1u32 << bit).to_le_bytes());
        assert_eq!(Spawn::decode(&wire), Err(Error::Flags));
    }
    for offset in 44..64 {
        let mut wire = SPAWN;
        wire[offset] = 1;
        assert_eq!(Spawn::decode(&wire), Err(Error::Reserved));
    }
    for rights in [0u32, 2, 4, 7, u32::MAX] {
        let mut wire = SPAWN;
        wire[40..44].copy_from_slice(&rights.to_le_bytes());
        assert_eq!(Spawn::decode(&wire), Err(Error::Rights));
    }
    let mut wait = SPAWN;
    wait[40] = 1;
    assert_eq!(Spawn::decode(&wait).unwrap().rights(), Rights::Wait);
}

#[test]
fn spawn_error_precedence() {
    let mut wire = SPAWN;
    wire[0] = 2;
    wire[12] = 4;
    wire[63] = 1;
    wire[40] = 0;
    wire[32] = 0;
    wire[24..32].copy_from_slice(&257u64.to_le_bytes());
    assert_eq!(Spawn::decode(&wire[..63]), Err(Error::Length));
    assert_eq!(Spawn::decode(&wire), Err(Error::Header));
    wire[0] = 1;
    assert_eq!(Spawn::decode(&wire), Err(Error::Flags));
    wire[12] = 0;
    assert_eq!(Spawn::decode(&wire), Err(Error::Reserved));
    wire[63] = 0;
    assert_eq!(Spawn::decode(&wire), Err(Error::Rights));
    wire[40] = 1;
    assert_eq!(Spawn::decode(&wire), Err(Error::Frames));
    wire[32] = 1;
    assert_eq!(Spawn::decode(&wire), Err(Error::Length));
    wire[24..32].copy_from_slice(&1u64.to_le_bytes());
    wire[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(Spawn::decode(&wire), Err(Error::Input));
    wire[12] = 1;
    assert_eq!(Spawn::decode(&wire), Err(Error::Address));
    assert_eq!(
        Spawn::new(1, Some((u64::MAX, 257)), false, 0, Rights::Wait),
        Err(Error::Frames)
    );
}

#[test]
fn unknown_program_is_metadata() {
    for program in [0, 3, 0x1234_5678, u32::MAX] {
        let value = Spawn::new(program, None, false, 1, Rights::Wait).unwrap();
        let mut wire = [0; 64];
        value.encode_into(&mut wire).unwrap();
        assert_eq!(Spawn::decode(&wire).unwrap().program(), program);
    }
}

#[test]
fn grant_literal() {
    let grant = Grant::decode(&GRANT).unwrap();
    assert_eq!(grant.handle(), 0x1234_5678_0000_0002);
    assert!(grant.console_allowed());
    let value = Grant::new(0x1234_5678_0000_0002, true).unwrap();
    assert_eq!(grant, value);
    let mut wire = [0xa5; 64];
    value.encode_into(&mut wire).unwrap();
    assert_eq!(wire, GRANT);
    Grant::new(value.handle(), false)
        .unwrap()
        .encode_into(&mut wire)
        .unwrap();
    assert_eq!(&wire[48..56], &[0; 8]);
    assert!(!Grant::decode(&wire).unwrap().console_allowed());
}

#[test]
fn grant_handle_canonicality() {
    for token in [
        0,
        1,
        2,
        0x1_0000_0000,
        0x1_0000_0001,
        0x1_0000_0003,
        u64::MAX,
    ] {
        assert_eq!(Grant::new(token, false), Err(Error::Handle));
        let mut wire = GRANT;
        wire[16..24].copy_from_slice(&token.to_le_bytes());
        assert_eq!(Grant::decode(&wire), Err(Error::Handle));
    }
    for token in [0x1_0000_0002, 0xffff_ffff_0000_0002] {
        assert_eq!(Grant::new(token, false).unwrap().handle(), token);
    }
}

#[test]
fn grant_limits_and_features() {
    for offset in 24..48 {
        let mut wire = GRANT;
        wire[offset] ^= 1;
        assert_eq!(Grant::decode(&wire), Err(Error::Limits));
    }
    for bit in 1..64 {
        let mut wire = GRANT;
        wire[48..56].copy_from_slice(&(1u64 << bit).to_le_bytes());
        assert_eq!(Grant::decode(&wire), Err(Error::Flags));
    }
    for offset in 56..64 {
        let mut wire = GRANT;
        wire[offset] = 1;
        assert_eq!(Grant::decode(&wire), Err(Error::Reserved));
    }
    for offset in 0..16 {
        let mut wire = GRANT;
        wire[offset] ^= 1;
        assert_eq!(Grant::decode(&wire), Err(Error::Header));
    }
}

#[test]
fn grant_error_precedence() {
    let mut wire = GRANT;
    wire[0] = 0;
    wire[56] = 1;
    wire[48] = 2;
    wire[16] = 1;
    wire[24] = 1;
    assert_eq!(Grant::decode(&wire[..63]), Err(Error::Length));
    assert_eq!(Grant::decode(&wire), Err(Error::Header));
    wire[0] = b'H';
    assert_eq!(Grant::decode(&wire), Err(Error::Reserved));
    wire[56] = 0;
    assert_eq!(Grant::decode(&wire), Err(Error::Flags));
    wire[48] = 1;
    assert_eq!(Grant::decode(&wire), Err(Error::Handle));
    wire[16] = 2;
    assert_eq!(Grant::decode(&wire), Err(Error::Limits));
}

#[test]
fn encoders_are_transactional() {
    let spawn = Spawn::decode(&SPAWN).unwrap();
    let grant = Grant::decode(&GRANT).unwrap();
    for size in 0..=128 {
        let mut out = vec![0xa5; size];
        if size == 64 {
            continue;
        }
        assert_eq!(spawn.encode_into(&mut out), Err(Error::Length));
        assert!(out.iter().all(|byte| *byte == 0xa5));
        assert_eq!(grant.encode_into(&mut out), Err(Error::Length));
        assert!(out.iter().all(|byte| *byte == 0xa5));
        assert_eq!(Spawn::decode(&out), Err(Error::Length));
        assert_eq!(Grant::decode(&out), Err(Error::Length));
    }
    let mut surrounding = [0xa5; 66];
    spawn.encode_into(&mut surrounding[1..65]).unwrap();
    assert_eq!(&surrounding[1..65], &SPAWN);
    assert_eq!((surrounding[0], surrounding[65]), (0xa5, 0xa5));
    grant.encode_into(&mut surrounding[1..65]).unwrap();
    assert_eq!(&surrounding[1..65], &GRANT);
    assert_eq!((surrounding[0], surrounding[65]), (0xa5, 0xa5));
}

#[test]
fn unaligned_and_owned_values() {
    let (spawn, grant) = {
        let mut bytes = [0; 130];
        bytes[1..65].copy_from_slice(&SPAWN);
        bytes[65..129].copy_from_slice(&GRANT);
        let spawn = Spawn::decode(&bytes[1..65]).unwrap();
        let grant = Grant::decode(&bytes[65..129]).unwrap();
        bytes.fill(0xff);
        assert_eq!(spawn.input(), Some((0x0102_0304_0506_0708, 256)));
        assert_eq!(grant.handle(), 0x1234_5678_0000_0002);
        (spawn, grant)
    };
    let mut encoded = [0; 64];
    spawn.encode_into(&mut encoded).unwrap();
    assert_eq!(encoded, SPAWN);
    grant.encode_into(&mut encoded).unwrap();
    assert_eq!(encoded, GRANT);
}

#[test]
fn grant_through_startup_input() {
    use hm_kernel::{
        native::Issuer,
        startup_input::{self, Input},
    };
    let mut issuer = Issuer::new();
    let base = issuer.issue(true).unwrap().startup();
    let mut page = [0xa5; 4096];
    page[..64].copy_from_slice(&base);
    let input = Input::from_bytes(&GRANT).unwrap();
    input.encode_into(&mut page[64..]).unwrap();
    assert_eq!(&page[..64], &base);
    assert_eq!(
        &page[64..88],
        b"HMINPUT\0\x01\0\0\0\x18\0\0\0\x40\0\0\0\0\0\0\0"
    );
    assert_eq!(&page[88..152], &GRANT);
    assert!(page[152..].iter().all(|byte| *byte == 0));
    let view = startup_input::decode(&page[64..]).unwrap();
    assert!(view.is_present());
    assert_eq!(
        Grant::decode(view.bytes()).unwrap().handle(),
        0x1234_5678_0000_0002
    );
    page[4095] = 1;
    assert_eq!(
        startup_input::decode(&page[64..]).unwrap_err(),
        startup_input::Error::Padding
    );
}

#[test]
fn all_completion_values() {
    for status in 0..=255 {
        let value = Completion::Exited(status);
        assert_eq!(value.encode(), u64::from(status) * 256);
        assert_eq!(Completion::decode(u64::from(status) * 256), Ok(value));
    }
    for (word, value) in [
        (1, Completion::Fault),
        (2, Completion::Budget),
        (3, Completion::CallLimit),
        (4, Completion::TurnLimit),
        (5, Completion::Cancelled),
        (6, Completion::Unavailable),
    ] {
        assert_eq!(value.encode(), word);
        assert_eq!(Completion::decode(word), Ok(value));
    }
}

#[test]
fn malformed_completion_words() {
    for kind in 7..=255 {
        assert_eq!(Completion::decode(kind), Err(Error::Completion));
    }
    for kind in 1..=6 {
        for status in 1..=255 {
            assert_eq!(
                Completion::decode(kind | status << 8),
                Err(Error::Completion)
            );
        }
    }
    for bit in 16..64 {
        for low in [0, 1, 0xff00] {
            assert_eq!(
                Completion::decode(1u64 << bit | low),
                Err(Error::Completion)
            );
        }
    }
    assert_eq!(Completion::decode(u64::MAX), Err(Error::Completion));
}

#[test]
fn wait_and_cancel_arguments() {
    assert_eq!(WaitMode::decode(0, 0), Ok(WaitMode::Blocking));
    assert_eq!(WaitMode::decode(1, 0), Ok(WaitMode::Poll));
    assert_eq!(task_wire::cancel_arguments(0, 0), Ok(()));
    for mode in [2, 3, u64::MAX] {
        assert_eq!(WaitMode::decode(mode, 0), Err(Error::Arguments));
    }
    for bit in 0..64 {
        let value = 1u64 << bit;
        assert_eq!(WaitMode::decode(0, value), Err(Error::Arguments));
        assert_eq!(WaitMode::decode(1, value), Err(Error::Arguments));
        assert_eq!(task_wire::cancel_arguments(value, 0), Err(Error::Arguments));
        assert_eq!(task_wire::cancel_arguments(0, value), Err(Error::Arguments));
    }
}

#[test]
fn bounded_mutations_are_total() {
    for offset in 0..64 {
        for bit in 0..8 {
            let mut spawn = SPAWN;
            spawn[offset] ^= 1 << bit;
            if let Ok(value) = Spawn::decode(&spawn) {
                let mut encoded = [0xa5; 64];
                value.encode_into(&mut encoded).unwrap();
                assert_eq!(encoded, spawn);
            }
            let mut grant = GRANT;
            grant[offset] ^= 1 << bit;
            if let Ok(value) = Grant::decode(&grant) {
                let mut encoded = [0xa5; 64];
                value.encode_into(&mut encoded).unwrap();
                assert_eq!(encoded, grant);
            }
        }
    }
}
