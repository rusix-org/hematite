#![forbid(unsafe_code)]
use hm_kernel::boot_image::{AdmissionError, admit};
use hm_kernel::{elf, initramfs};

const ARCHIVE: &[u8] = include_bytes!("../../hm-core/fixtures/initramfs.cpio");
const HELLO: &[u8] = include_bytes!("../../hm-core/fixtures/process/hello.elf");
const BANNER: &[u8] = b"hematite native initramfs\n";

#[test]
fn canonical_native_image() {
    let image = admit(ARCHIVE, "bin/hello").unwrap();
    assert_eq!(image, elf::Image::parse(HELLO).unwrap());
    assert_eq!(
        initramfs::Archive::parse(ARCHIVE)
            .unwrap()
            .get("bin/hello")
            .unwrap()
            .bytes(),
        HELLO,
    );
}

#[test]
fn missing_entry() {
    assert_eq!(admit(ARCHIVE, "bin/missing"), Err(AdmissionError::Missing));
}

#[test]
fn directory_entry() {
    for path in ["bin", "etc"] {
        assert_eq!(admit(ARCHIVE, path), Err(AdmissionError::Directory));
    }
}

#[test]
fn invalid_elf_entry() {
    assert_eq!(
        initramfs::Archive::parse(ARCHIVE)
            .unwrap()
            .get("etc/banner")
            .unwrap()
            .bytes(),
        BANNER,
    );
    assert_eq!(
        admit(ARCHIVE, "etc/banner"),
        Err(AdmissionError::Elf(elf::Image::parse(BANNER).unwrap_err()))
    );
}

#[test]
fn exact_lookup_only() {
    for path in [
        "",
        "./bin/hello",
        "/bin/hello",
        "bin//hello",
        "bin/hello/",
        "bin/../bin/hello",
        "bin/Hello",
        "bin/hell",
        "TRAILER!!!",
    ] {
        assert_eq!(admit(ARCHIVE, path), Err(AdmissionError::Missing));
    }
}

fn corrupt_suffix() -> Vec<u8> {
    // Keep the real admitted hello entry intact and append nonzero trailing
    // junk. Complete archive admission must fail before selecting that entry.
    assert!(admit(ARCHIVE, "bin/hello").is_ok());
    let mut bytes = ARCHIVE.to_vec();
    bytes.push(1);
    bytes
}

#[test]
fn corrupt_suffix_rejects_existing_image() {
    let malformed = include_bytes!("../../hm-core/fixtures/malformed-initramfs.cpio");
    let malformed_error = initramfs::Archive::parse(malformed).unwrap_err();
    assert_eq!(
        admit(malformed, "first"),
        Err(AdmissionError::Archive(malformed_error)),
    );
    let bytes = corrupt_suffix();
    let error = initramfs::Archive::parse(&bytes).unwrap_err();
    assert_eq!(
        admit(&bytes, "bin/hello"),
        Err(AdmissionError::Archive(error))
    );
}

#[test]
fn corrupt_suffix_precedes_missing() {
    let bytes = corrupt_suffix();
    let error = initramfs::Archive::parse(&bytes).unwrap_err();
    assert_eq!(
        admit(&bytes, "bin/missing"),
        Err(AdmissionError::Archive(error))
    );
}

#[test]
fn image_outlives_temporary_path() {
    let bytes = ARCHIVE.to_vec();
    let image = {
        let path = String::from("bin/hello");
        admit(&bytes, &path).unwrap()
    };
    assert_eq!(image, elf::Image::parse(HELLO).unwrap());
}
