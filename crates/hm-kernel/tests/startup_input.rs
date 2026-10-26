#![forbid(unsafe_code)]
use hm_kernel::startup_input::{
    EXTENSION_OFFSET, EXTENSION_SIZE, Error, HEADER_SIZE, Input, MAX_INPUT_SIZE, PAYLOAD_OFFSET,
    decode,
};

fn encoded(bytes: &[u8]) -> [u8; EXTENSION_SIZE] {
    let mut out = [0xaa; EXTENSION_SIZE];
    Input::from_bytes(bytes)
        .unwrap()
        .encode_into(&mut out)
        .unwrap();
    out
}
#[test]
fn absent_canonical() {
    let input = Input::absent();
    assert_eq!(input.as_bytes(), None);
    let mut out = [0xff; EXTENSION_SIZE];
    input.encode_into(&mut out).unwrap();
    assert_eq!(out, [0; EXTENSION_SIZE]);
    let view = decode(&out).unwrap();
    assert!(!view.is_present());
    assert!(view.bytes().is_empty());
}
#[test]
fn present_empty_distinct() {
    let input = Input::from_bytes(&[]).unwrap();
    assert_eq!(input.as_bytes(), Some([].as_slice()));
    let out = encoded(&[]);
    // Literal independent wire expectation, not a copy of an encoder helper.
    assert_eq!(&out[..24], b"HMINPUT\0\x01\0\0\0\x18\0\0\0\0\0\0\0\0\0\0\0");
    assert!(out[24..].iter().all(|b| *b == 0));
    let view = decode(&out).unwrap();
    assert!(view.is_present());
    assert!(view.bytes().is_empty());
}
#[test]
fn opaque_round_trip() {
    let bytes = core::array::from_fn::<_, 256, _>(|i| i as u8);
    let out = encoded(&bytes);
    assert_eq!(&out[16..20], &[0, 1, 0, 0]);
    assert_eq!(&out[24..280], &bytes);
    let view = decode(&out).unwrap();
    assert!(view.is_present());
    assert_eq!(view.bytes(), bytes);
    assert_eq!(view.bytes().as_ptr(), out[24..].as_ptr());
}
#[test]
fn all_input_lengths() {
    let bytes = core::array::from_fn::<_, 256, _>(|i| (i as u8).wrapping_mul(73));
    for length in 0..=256 {
        let out = encoded(&bytes[..length]);
        assert_eq!(
            u32::from_le_bytes(out[16..20].try_into().unwrap()),
            length as u32
        );
        assert_eq!(decode(&out).unwrap().bytes(), &bytes[..length]);
        assert!(out[24 + length..].iter().all(|b| *b == 0));
    }
}
#[test]
fn maximum_and_over_limit() {
    assert_eq!(MAX_INPUT_SIZE, 256);
    assert_eq!(
        Input::from_bytes(&[7; 256]).unwrap().as_bytes(),
        Some([7; 256].as_slice())
    );
    for length in [257, 258, 4096] {
        assert_eq!(
            Input::from_bytes(&vec![0; length]).unwrap_err(),
            Error::Length
        );
    }
}
#[test]
fn owns_input_copy() {
    let mut out = [0; EXTENSION_SIZE];
    {
        let input = {
            let mut source = vec![0, 255, 10, 128, 42];
            let input = Input::from_bytes(&source).unwrap();
            source.fill(1);
            assert_eq!(input.as_bytes(), Some([0, 255, 10, 128, 42].as_slice()));
            input
        };
        input.encode_into(&mut out).unwrap();
    }
    assert_eq!(decode(&out).unwrap().bytes(), &[0, 255, 10, 128, 42]);
}

#[test]
fn wrong_destination_size_unchanged() {
    for input in [Input::absent(), Input::from_bytes(b"payload").unwrap()] {
        for length in (0..EXTENSION_SIZE).chain([EXTENSION_SIZE + 1]) {
            let mut out = vec![0xa5; length];
            assert_eq!(input.encode_into(&mut out), Err(Error::Size));
            assert!(out.iter().all(|b| *b == 0xa5));
        }
    }
}
#[test]
fn exact_decode_extent() {
    let out = encoded(b"data");
    for end in 0..EXTENSION_SIZE {
        assert_eq!(decode(&out[..end]), Err(Error::Size));
    }
    assert_eq!(decode(&[0; EXTENSION_SIZE + 1]), Err(Error::Size));
}
#[test]
fn malformed_header() {
    let canonical = encoded(b"abc");
    for (range, error) in [
        (0..8, Error::Magic),
        (8..12, Error::Version),
        (12..16, Error::Header),
        (20..24, Error::Reserved),
    ] {
        for i in range {
            for bit in 0..8 {
                let mut bytes = canonical;
                bytes[i] ^= 1 << bit;
                assert_eq!(decode(&bytes), Err(error), "byte {i} bit {bit}");
            }
        }
    }
    // A nonzero suffix is not an absent extension, even with an all-zero header.
    let mut bytes = [0; EXTENSION_SIZE];
    bytes[EXTENSION_SIZE - 1] = 1;
    assert_eq!(decode(&bytes), Err(Error::Magic));
}
#[test]
fn malformed_lengths() {
    for length in [257u32, 4032, 4096, u32::MAX] {
        let mut bytes = encoded(&[]);
        bytes[16..20].copy_from_slice(&length.to_le_bytes());
        assert_eq!(decode(&bytes), Err(Error::Length));
    }
    let mut bytes = encoded(b"abc");
    bytes[16..20].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(decode(&bytes), Err(Error::Padding));
}
#[test]
fn malformed_tail() {
    for payload in [&[][..], &b"abc"[..], &[0xff; 256][..]] {
        let canonical = encoded(payload);
        for i in 24 + payload.len()..EXTENSION_SIZE {
            let mut bytes = canonical;
            bytes[i] = 1;
            assert_eq!(decode(&bytes), Err(Error::Padding), "tail byte {i}");
        }
    }
}
#[test]
fn reencoding_clears_stale_bytes() {
    let mut out = encoded(&[0xff; 256]);
    Input::from_bytes(b"a")
        .unwrap()
        .encode_into(&mut out)
        .unwrap();
    assert_eq!(out, encoded(b"a"));
    Input::from_bytes(&[])
        .unwrap()
        .encode_into(&mut out)
        .unwrap();
    assert_eq!(out, encoded(&[]));
    Input::absent().encode_into(&mut out).unwrap();
    assert_eq!(out, [0; EXTENSION_SIZE]);
}
#[test]
fn legacy_prefix_unchanged() {
    assert_eq!(
        (
            EXTENSION_OFFSET,
            EXTENSION_SIZE,
            HEADER_SIZE,
            PAYLOAD_OFFSET
        ),
        (64, 4032, 24, 24)
    );
    let prefix = hm_kernel::native::Issuer::new()
        .issue(true)
        .unwrap()
        .startup();
    let mut page = [0x55; 4096];
    page[..64].copy_from_slice(&prefix);
    Input::from_bytes(b"input")
        .unwrap()
        .encode_into(&mut page[EXTENSION_OFFSET..])
        .unwrap();
    assert_eq!(&page[..64], &prefix);
    assert_eq!(decode(&page[64..]).unwrap().bytes(), b"input");
    Input::absent().encode_into(&mut page[64..]).unwrap();
    assert_eq!(&page[..64], &prefix);
    assert!(page[64..].iter().all(|b| *b == 0));
}
#[test]
fn deterministic_mutations() {
    let seed = encoded(&[0x33; 256]);
    for index in 0..EXTENSION_SIZE {
        for mask in [1, 0x80, 0xff] {
            let mut bytes = seed;
            bytes[index] ^= mask;
            if let Ok(view) = decode(&bytes) {
                // Every accepted representation must reproduce identically;
                // this rejects unnoticed noncanonical fields and trailing bytes.
                let owned = if view.is_present() {
                    Input::from_bytes(view.bytes()).unwrap()
                } else {
                    Input::absent()
                };
                let mut canonical = [0; EXTENSION_SIZE];
                owned.encode_into(&mut canonical).unwrap();
                assert_eq!(canonical, bytes);
            }
        }
    }
}
