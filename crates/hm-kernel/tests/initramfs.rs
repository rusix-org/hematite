#![forbid(unsafe_code)]
use hm_kernel::initramfs::{
    Archive, Kind, MAX_ARCHIVE_SIZE, MAX_ENTRIES, MAX_FILE_SIZE, MAX_PATH_SIZE,
};

const HELLO: &[u8] = include_bytes!("../../hm-core/fixtures/process/hello.elf");
const CANONICAL: &[u8] = include_bytes!("../../hm-core/fixtures/initramfs.cpio");
const BANNER: &[u8] = b"hematite native initramfs\n";

fn field(bytes: &mut [u8], start: usize, index: usize, value: u32) {
    let offset = start + 6 + index * 8;
    bytes[offset..offset + 8].copy_from_slice(format!("{value:08x}").as_bytes());
}
fn record(out: &mut Vec<u8>, name: &[u8], mode: u32, inode: u32, data: &[u8]) -> (usize, usize) {
    let start = out.len();
    let values = [
        inode,
        mode,
        0,
        0,
        if mode & 0o170000 == 0o040000 { 2 } else { 1 },
        0,
        u32::try_from(data.len()).unwrap(),
        0,
        0,
        0,
        0,
        u32::try_from(name.len() + 1).unwrap(),
        0,
    ];
    out.extend_from_slice(b"070701");
    for value in values {
        out.extend_from_slice(format!("{value:08x}").as_bytes());
    }
    out.extend_from_slice(name);
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    let payload = out.len();
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    (start, payload)
}
fn finish(out: &mut Vec<u8>) -> usize {
    record(out, b"TRAILER!!!", 0, 0, &[]).0
}
fn one(name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    record(&mut bytes, name, 0o100644, 1, data);
    finish(&mut bytes);
    bytes
}
fn rejected(bytes: &[u8]) {
    assert!(Archive::parse(bytes).is_err());
}
fn reference(bytes: &[u8]) {
    let archive = Archive::parse(bytes).unwrap();
    assert_eq!(archive.len(), 4);
    assert_eq!(
        archive.entries().map(|e| e.path()).collect::<Vec<_>>(),
        ["bin", "bin/hello", "etc", "etc/banner"]
    );
    assert_eq!(archive.get("bin").unwrap().kind(), Kind::Directory);
    assert_eq!(archive.get("bin/hello").unwrap().kind(), Kind::File);
    assert_eq!(archive.get("bin/hello").unwrap().permissions(), 0o755);
    assert_eq!(archive.get("bin/hello").unwrap().bytes(), HELLO);
    assert_eq!(archive.get("etc/banner").unwrap().bytes(), BANNER);
    assert!(hm_kernel::elf::Image::parse(archive.get("bin/hello").unwrap().bytes()).is_ok());
    assert!(archive.get("./bin/hello").is_none());
    assert!(archive.get("bin/missing").is_none());
}
#[test]
fn canonical_payload_and_elf() {
    reference(CANONICAL);
}
#[test]
fn gnu_reference_payload_and_elf() {
    let path = std::env::var_os("HM_INITRAMFS_REFERENCE")
        .expect("independent GNU cpio reference is required");
    reference(&std::fs::read(path).unwrap());
}
#[test]
fn empty_and_zero_length_files() {
    let mut empty = Vec::new();
    finish(&mut empty);
    let archive = Archive::parse(&empty).unwrap();
    assert!(archive.is_empty());
    assert_eq!(archive.entries().count(), 0);
    assert!(archive.get("TRAILER!!!").is_none());
    let bytes = one(b"empty", &[]);
    assert!(
        Archive::parse(&bytes)
            .unwrap()
            .get("empty")
            .unwrap()
            .bytes()
            .is_empty()
    );
    rejected(&[]);
}
#[test]
fn alignment_and_hex_fields() {
    for name_len in 1..=8 {
        for data_len in 0..=8 {
            let name = vec![b'a'; name_len];
            let data = vec![0xab; data_len];
            let mut bytes = one(&name, &data);
            field(&mut bytes, 0, 2, 0xabcd_efab);
            for index in 0..13 {
                bytes[6 + index * 8..14 + index * 8].make_ascii_uppercase();
            }
            let archive = Archive::parse(&bytes).unwrap();
            assert_eq!(archive.entries().next().unwrap().bytes(), data);
        }
    }
}
#[test]
fn descriptive_metadata_not_authority() {
    let mut bytes = one(b"plain", b"abc");
    for index in [0, 2, 3, 5, 7, 8] {
        field(&mut bytes, 0, index, u32::MAX);
    }
    let entry = Archive::parse(&bytes).unwrap().get("plain").unwrap();
    assert_eq!(entry.permissions(), 0o644);
    assert_eq!(entry.kind(), Kind::File);
    assert!(hm_kernel::elf::Image::parse(entry.bytes()).is_err());
    let mut dirs = Vec::new();
    record(&mut dirs, b"dir", 0o040000, 1, &[]);
    field(&mut dirs, 0, 4, u32::MAX);
    finish(&mut dirs);
    assert!(Archive::parse(&dirs).is_ok());
    field(&mut dirs, 0, 4, 1);
    assert!(Archive::parse(&dirs).is_ok());
}
#[test]
fn entry_capacity() {
    let mut mixed = Vec::new();
    record(&mut mixed, b"dir", 0o040755, 1, &[]);
    for i in 1..MAX_ENTRIES {
        record(
            &mut mixed,
            format!("dir/f{i}").as_bytes(),
            0o100644,
            (i + 1) as u32,
            &[],
        );
    }
    let mut overflow = mixed.clone();
    record(&mut overflow, b"extra", 0o040755, 65, &[]);
    finish(&mut mixed);
    finish(&mut overflow);
    assert_eq!(Archive::parse(&mixed).unwrap().len(), MAX_ENTRIES);
    rejected(&overflow);
    for count in [MAX_ENTRIES, MAX_ENTRIES + 1] {
        let mut bytes = Vec::new();
        for i in 0..count {
            record(
                &mut bytes,
                format!("f{i}").as_bytes(),
                0o100000,
                (i + 1) as u32,
                &[],
            );
        }
        finish(&mut bytes);
        if count == MAX_ENTRIES {
            assert_eq!(Archive::parse(&bytes).unwrap().len(), count);
        } else {
            rejected(&bytes);
        }
    }
}
#[test]
fn file_and_archive_capacity() {
    assert!(Archive::parse(&one(b"max", &vec![0; MAX_FILE_SIZE])).is_ok());
    rejected(&one(b"large", &vec![0; MAX_FILE_SIZE + 1]));
    let build = |last: usize| {
        let mut bytes = Vec::new();
        for i in 0..4 {
            record(
                &mut bytes,
                format!("f{i}").as_bytes(),
                0o100644,
                i + 1,
                &vec![0; if i == 3 { last } else { MAX_FILE_SIZE }],
            );
        }
        finish(&mut bytes);
        bytes
    };
    let excess = build(MAX_FILE_SIZE).len() - MAX_ARCHIVE_SIZE;
    let mut bytes = build(MAX_FILE_SIZE - excess);
    assert_eq!(bytes.len(), MAX_ARCHIVE_SIZE);
    assert_eq!(Archive::parse(&bytes).unwrap().len(), 4);
    bytes.push(0);
    rejected(&bytes);
}
#[test]
fn path_capacity_and_alphabet() {
    assert!(Archive::parse(&one(&[b'x'; MAX_PATH_SIZE], &[])).is_ok());
    rejected(&one(&[b'x'; MAX_PATH_SIZE + 1], &[]));
    assert!(Archive::parse(&one(b"Az09-_.x", &[])).is_ok());
    for path in [
        b"".as_slice(),
        b".",
        b"..",
        b"/a",
        b"a/",
        b"a//b",
        b"a/./b",
        b"a/../b",
        b"a\\b",
        b"a b",
        b"a\nb",
        b"a\0b",
        b"\xff",
        b"TRAILER!!!",
    ] {
        rejected(&one(path, &[]));
    }
}
#[test]
fn hierarchy_and_duplicates() {
    for (first_mode, second_mode) in [(0o040755, 0o100644), (0o100644, 0o040755)] {
        for (name, ino) in [(b"a".as_slice(), 2), (b"b".as_slice(), 1)] {
            let mut bytes = Vec::new();
            record(&mut bytes, b"a", first_mode, 1, &[]);
            record(&mut bytes, name, second_mode, ino, &[]);
            finish(&mut bytes);
            rejected(&bytes);
        }
    }
    let mut late_parent = Vec::new();
    record(&mut late_parent, b"a/b", 0o100644, 1, &[]);
    record(&mut late_parent, b"a", 0o040755, 2, &[]);
    finish(&mut late_parent);
    rejected(&late_parent);
    let mut good = Vec::new();
    record(&mut good, b"a", 0o040755, 1, &[]);
    record(&mut good, b"a/b", 0o040755, 2, &[]);
    record(&mut good, b"a/b/c", 0o100644, 3, b"ok");
    finish(&mut good);
    assert_eq!(Archive::parse(&good).unwrap().len(), 3);
    rejected(&one(b"a/b", &[]));
    for (first, second, first_mode, same_inode) in [
        ("a", "a", 0o100644, false),
        ("a", "a/b", 0o100644, false),
        ("a/b", "a", 0o100644, false),
        ("a", "b", 0o100644, true),
    ] {
        let mut bytes = Vec::new();
        record(&mut bytes, first.as_bytes(), first_mode, 1, &[]);
        record(
            &mut bytes,
            second.as_bytes(),
            0o100644,
            if same_inode { 1 } else { 2 },
            &[],
        );
        finish(&mut bytes);
        rejected(&bytes);
    }
    let mut bytes = Vec::new();
    record(&mut bytes, b"a", 0o100644, 1, &[]);
    let (s, _) = record(&mut bytes, b"b", 0o100644, 1, &[]);
    field(&mut bytes, s, 8, 1);
    finish(&mut bytes);
    assert!(Archive::parse(&bytes).is_ok());
}
#[test]
fn metadata_and_types() {
    for mode in [
        0,
        0o120777,
        0o060644,
        0o020644,
        0o010644,
        0o140644,
        0o104644,
        0o102644,
        0o101644,
        u32::MAX,
    ] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, 1, mode);
        rejected(&bytes);
    }
    for index in [9, 10, 12] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, index, 1);
        rejected(&bytes);
    }
    for links in [0, 2, u32::MAX] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, 4, links);
        rejected(&bytes);
    }
    {
        let mut bytes = Vec::new();
        record(&mut bytes, b"a", 0o040755, 1, &[]);
        field(&mut bytes, 0, 4, 0);
        finish(&mut bytes);
        rejected(&bytes);
    }
    let mut bytes = Vec::new();
    record(&mut bytes, b"a", 0o040755, 1, b"not empty");
    finish(&mut bytes);
    rejected(&bytes);
    for mode in [0o100000, 0o100777] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, 1, mode);
        assert!(Archive::parse(&bytes).is_ok());
    }
}
#[test]
fn header_field_rejection() {
    for index in 0..13 {
        for offset in 0..8 {
            for invalid in [b'g', b' ', b'+', 0] {
                let mut bytes = one(b"a", b"data");
                bytes[6 + index * 8 + offset] = invalid;
                rejected(&bytes);
            }
        }
    }
    for magic in [b"070702", b"070707", b"000000"] {
        let mut bytes = one(b"a", &[]);
        bytes[..6].copy_from_slice(magic);
        rejected(&bytes);
    }
    for index in [6, 11] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, index, u32::MAX);
        rejected(&bytes);
    }
    for size in [0, 1] {
        let mut bytes = one(b"a", &[]);
        field(&mut bytes, 0, 11, size);
        rejected(&bytes);
    }
    let mut bytes = one(b"a", &[]);
    bytes[111] = b'x';
    rejected(&bytes);
}
#[test]
fn truncation_all_prefixes() {
    let bytes = one(b"sample", b"some data\0TRAILER!!!070701");
    for length in 0..bytes.len() {
        rejected(&bytes[..length]);
    }
    assert!(Archive::parse(&bytes).is_ok());
}
#[test]
fn padding_and_trailer() {
    let mut bytes = Vec::new();
    let (_, data) = record(&mut bytes, b"ab", 0o100644, 1, b"x");
    let trailer = finish(&mut bytes);
    for position in (113..data).chain(data + 1..trailer) {
        let mut bad = bytes.clone();
        bad[position] = 1;
        rejected(&bad);
    }
    for index in 0..13 {
        let mut bad = bytes.clone();
        field(&mut bad, trailer, index, if index == 4 { 0 } else { 1 });
        rejected(&bad);
    }
    for position in trailer + 121..bytes.len() {
        let mut bad = bytes.clone();
        bad[position] = 1;
        rejected(&bad);
    }
    rejected(&bytes[..trailer]);
    let mut duplicate = bytes.clone();
    finish(&mut duplicate);
    rejected(&duplicate);
    for count in [0, 1, 511] {
        let mut padded = bytes.clone();
        padded.resize(padded.len() + count, 0);
        assert!(Archive::parse(&padded).is_ok());
    }
    let mut too_much = bytes.clone();
    too_much.resize(too_much.len() + 512, 0);
    rejected(&too_much);
    let mut junk = bytes;
    junk.push(1);
    rejected(&junk);
}
#[test]
fn complete_admission_no_prefix() {
    let mut bytes = Vec::new();
    record(&mut bytes, b"hello", 0o100755, 1, HELLO);
    let (later, _) = record(&mut bytes, b"late", 0o100644, 2, b"payload");
    finish(&mut bytes);
    assert_eq!(
        Archive::parse(&bytes)
            .unwrap()
            .get("hello")
            .unwrap()
            .bytes(),
        HELLO
    );
    bytes[later] = b'x';
    rejected(&bytes);
    rejected(&bytes[..later]);
}
#[test]
fn mutations_are_total() {
    let original = one(b"data", b"payload");
    let mut state = 0x1234_5678u32;
    for i in 0..4096usize {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let mut bytes = if i % 2 == 0 {
            original.clone()
        } else {
            vec![0; i % 512]
        };
        if !bytes.is_empty() {
            let pos = state as usize % bytes.len();
            bytes[pos] ^= (state >> 24) as u8;
        }
        if let Ok(archive) = Archive::parse(&bytes) {
            assert!(archive.len() <= MAX_ENTRIES);
            for entry in archive.entries() {
                assert!(entry.path().len() <= MAX_PATH_SIZE);
                assert!(entry.bytes().len() <= MAX_FILE_SIZE);
                assert_eq!(archive.get(entry.path()), Some(entry));
            }
        }
    }
}
#[test]
fn borrowed_data_exact() {
    let payload: Vec<_> = (0..=255).collect();
    let mut bytes = Vec::new();
    let (_, offset) = record(&mut bytes, b"data", 0o100644, 1, &payload);
    finish(&mut bytes);
    let entry = {
        let archive = Archive::parse(&bytes).unwrap();
        archive.get("data").unwrap()
    };
    assert_eq!(entry.bytes(), payload);
    assert_eq!(entry.bytes().as_ptr(), bytes[offset..].as_ptr());
    assert_eq!(entry.path(), "data");
}
