#![forbid(unsafe_code)]
use hm_kernel::interactive_wire::{self as wire, Error, Grant};
use hm_kernel::startup_input::{EXTENSION_SIZE, Input, decode};

// Independent literal: generation 0x12345678, input slot5, fixed LE limits.
const TOKEN: u64 = 0x1234_5678_0000_0005;
const CANONICAL: [u8; 64] = [
    b'H', b'M', b'I', b'N', b'P', b'U', b'T', 0, 1, 0, 0, 0, 64, 0, 0, 0, 5, 0, 0, 0, 0x78, 0x56,
    0x34, 0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn wire_constants() {
    assert_eq!((wire::READ, wire::INPUT_SLOT), (6, 5));
    assert_eq!(
        (
            wire::MAX_READ,
            wire::CALL_LIMIT,
            wire::BYTE_LIMIT,
            wire::GRANT_SIZE
        ),
        (256, 4096, 65536, 64)
    );
}

#[test]
fn grant_literal() {
    let grant = Grant::decode(&CANONICAL).unwrap();
    assert_eq!(grant.input_token(), TOKEN);
    assert_eq!(grant, Grant::new(TOKEN).unwrap());
    let mut output = [0xff; 64];
    grant.encode_into(&mut output).unwrap();
    assert_eq!(output, CANONICAL);
}

#[test]
fn token_shape_boundaries() {
    for token in [0, 5, 1, 0xffff_ffff, u64::MAX] {
        assert_eq!(Grant::new(token), Err(Error::Handle));
    }
    for slot in [0, 1, 2, 3, 4, 6, 7, u32::MAX] {
        for generation in [1u32, u32::MAX] {
            assert_eq!(
                Grant::new((u64::from(generation) << 32) | u64::from(slot)),
                Err(Error::Handle)
            );
        }
    }
    for bit in 0..32 {
        assert_eq!(Grant::new(TOKEN ^ (1u64 << bit)), Err(Error::Handle));
    }
    let mut no_generation = CANONICAL;
    no_generation[20..24].fill(0);
    assert_eq!(Grant::decode(&no_generation), Err(Error::Handle));
}

#[test]
fn generation_roundtrips() {
    for generation in [
        1u32,
        2,
        255,
        256,
        65535,
        65536,
        0x12345678,
        u32::MAX - 1,
        u32::MAX,
    ] {
        let token = (u64::from(generation) << 32) | 5;
        let grant = Grant::new(token).unwrap();
        let mut bytes = CANONICAL;
        bytes[20..24].copy_from_slice(&generation.to_le_bytes());
        assert_eq!(Grant::decode(&bytes), Ok(grant));
        let mut encoded = [0; 64];
        grant.encode_into(&mut encoded).unwrap();
        assert_eq!(encoded, bytes);
    }
}

#[test]
fn decode_size_boundaries() {
    for length in 0..64 {
        assert_eq!(Grant::decode(&CANONICAL[..length]), Err(Error::Length));
    }
    for length in 65..=129 {
        let mut bytes = vec![0; length];
        bytes[..64].copy_from_slice(&CANONICAL);
        assert_eq!(Grant::decode(&bytes), Err(Error::Length));
    }
    assert_eq!(Grant::decode(&[0; 4096]), Err(Error::Length));
}

#[test]
fn encode_transactional_size() {
    let grant = Grant::new(TOKEN).unwrap();
    for length in (0..=129).filter(|length| *length != 64) {
        let mut storage = vec![0xa5; length + 2];
        let before = storage.clone();
        assert_eq!(
            grant.encode_into(&mut storage[1..length + 1]),
            Err(Error::Length)
        );
        assert_eq!(storage, before);
    }
    let mut guarded = [0xa5; 66];
    grant.encode_into(&mut guarded[1..65]).unwrap();
    assert_eq!(guarded[0], 0xa5);
    assert_eq!(guarded[65], 0xa5);
    assert_eq!(&guarded[1..65], CANONICAL);
}

#[test]
fn field_rejections() {
    for (start, length, error) in [
        (0, 8, Error::Magic),
        (8, 4, Error::Version),
        (12, 4, Error::Size),
        (24, 8, Error::Flags),
        (32, 8, Error::Limits),
        (40, 8, Error::Limits),
        (48, 8, Error::Limits),
        (56, 8, Error::Reserved),
    ] {
        for bit in 0..length * 8 {
            let mut bytes = CANONICAL;
            bytes[start + bit / 8] ^= 1 << (bit % 8);
            assert_eq!(
                Grant::decode(&bytes),
                Err(error),
                "offset {start}, bit {bit}"
            );
        }
    }
    for bit in 0..32 {
        let mut bytes = CANONICAL;
        bytes[16 + bit / 8] ^= 1 << (bit % 8);
        assert_eq!(Grant::decode(&bytes), Err(Error::Handle));
    }
}

#[test]
fn decode_precedence() {
    let defects = [
        (0, Error::Magic),
        (8, Error::Version),
        (12, Error::Size),
        (24, Error::Flags),
        (16, Error::Handle),
        (32, Error::Limits),
        (56, Error::Reserved),
    ];
    let mut bytes = CANONICAL;
    for (offset, _) in defects {
        bytes[offset] ^= 0x80;
    }
    assert_eq!(Grant::decode(&bytes[..63]), Err(Error::Length));
    for (offset, error) in defects {
        assert_eq!(Grant::decode(&bytes), Err(error));
        bytes[offset] ^= 0x80;
    }
    assert_eq!(Grant::decode(&bytes).unwrap().input_token(), TOKEN);
}

#[test]
fn canonical_full_overwrite() {
    let grant = Grant::new(TOKEN).unwrap();
    for fill in [0u8, 1, 0x55, 0xaa, 0xff] {
        let mut bytes = [fill; 64];
        grant.encode_into(&mut bytes).unwrap();
        assert_eq!(bytes, CANONICAL);
    }
}

#[test]
fn nested_startup_input() {
    let input = Input::from_bytes(&CANONICAL).unwrap();
    let mut tail = [0xa5; EXTENSION_SIZE];
    input.encode_into(&mut tail).unwrap();
    let outer = decode(&tail).unwrap();
    assert!(outer.is_present());
    assert_eq!(outer.bytes(), CANONICAL.as_slice());
    assert_eq!(&tail[..8], &CANONICAL[..8]);
    assert_eq!(Grant::decode(outer.bytes()).unwrap().input_token(), TOKEN);
    // A valid outer payload remains merely bytes; inner validation is separate.
    let mut malformed = CANONICAL;
    malformed[24] = 1;
    Input::from_bytes(&malformed)
        .unwrap()
        .encode_into(&mut tail)
        .unwrap();
    assert_eq!(
        Grant::decode(decode(&tail).unwrap().bytes()),
        Err(Error::Flags)
    );
}

#[test]
fn layout_confusion_rejected() {
    let mut tail = [0; EXTENSION_SIZE];
    Input::from_bytes(&CANONICAL)
        .unwrap()
        .encode_into(&mut tail)
        .unwrap();
    assert_eq!(Grant::decode(&tail), Err(Error::Length));
    assert_eq!(Grant::decode(&tail[..64]), Err(Error::Size));
    assert!(decode(&CANONICAL).is_err());
    let task_grant = hm_kernel::task_wire::Grant::new((1u64 << 32) | 2, true).unwrap();
    let mut other = [0; 64];
    task_grant.encode_into(&mut other).unwrap();
    assert_eq!(Grant::decode(&other), Err(Error::Magic));
}

#[test]
fn arbitrary_inputs() {
    let mut state = 0x6a09_e667_f3bc_c909u64;
    for length in 0..=160 {
        let mut bytes = vec![0; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        assert!(Grant::decode(&bytes).is_err());
    }
    // Every accepted single-byte mutation must still be a canonical encoding.
    for offset in 0..64 {
        for value in 0..=255 {
            let mut bytes = CANONICAL;
            bytes[offset] = value;
            if let Ok(grant) = Grant::decode(&bytes) {
                let mut encoded = [0xff; 64];
                grant.encode_into(&mut encoded).unwrap();
                assert_eq!(encoded, bytes);
            }
        }
    }
}

#[test]
fn owned_metadata() {
    let grant = {
        let mut source = CANONICAL.to_vec();
        let grant = Grant::decode(&source).unwrap();
        source.fill(0);
        grant
    };
    let copied = grant;
    assert_eq!(copied.input_token(), TOKEN);
    assert_eq!(grant, copied);
    // Construction/decoding do not consult an issuer or mint a Session. Even
    // MAX generation and repeated copies are valid data, not proof of a lease.
    let unissued = Grant::new(0xffff_ffff_0000_0005).unwrap();
    let mut bytes = [0; 64];
    unissued.encode_into(&mut bytes).unwrap();
    assert_eq!(Grant::decode(&bytes), Ok(unissued));
}
