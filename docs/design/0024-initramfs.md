# Checked native initramfs admission

Status: safe parser implemented and locally verified, 2026-10-26, with independent
[review 0024](../reviews/0024-initramfs.md) and
[full regression evidence](../evidence/initramfs-admission.md). Archive-backed
execution remains separately gated. This is intended to close the input-source portion
of [0021](0021-arm-first-userspace.md) through the existing ELF/owned-space/native
execution path in [0023](0023-arm-native-process.md); it adds no filesystem,
Linux ABI, decompressor, extraction authority or new core memory mechanism.

## Safe API and resource ownership

Add `hm_kernel::initramfs`, allocation-free and `#![forbid(unsafe_code)]`.
`Archive::parse(bytes: &[u8]) -> Result<Archive<'_>, Error>` validates the entire
archive before publishing any entry. Its private fixed array stores at most
64 immutable `Entry<'a>` values and a private count; storage may use
`[Option<Entry<'a>>; 64]`. No heap, recursion, raw pointers, mutable slices or
callbacks. A private constructor binds every validated name/data slice to the
original immutable input borrow. `Entry` may be Copy/Clone; this does not extend
the input lifetime. `Archive` need not be Clone.

Expose `Archive::entries()` as an iterator over exactly the validated entries,
`len()`, `is_empty()`, and `get(path: &str) -> Option<Entry<'a>>` for exact byte
matching without normalization. Entry exposes `path() -> &'a str`,
`kind() -> Kind` (File/Directory), `bytes() -> &'a [u8]`, and `permissions() -> u16`
(the low nine mode bits only). Directory bytes are empty. Names, types and data
cannot be forged through safe public field access. Metadata is descriptive;
execute permission is determined by the existing ELF loader, not archive modes.

Bounds: total input at most 4 MiB, each regular payload at most 1 MiB, path at
most 128 bytes excluding its terminal NUL, and at most 64 non-trailer entries
including directories. Empty regular files and a trailer-only archive are valid.
Errors distinguish Header, Unsupported, Bounds, Capacity, Path, Duplicate,
Hierarchy, Padding and Trailer; precedence between simultaneously invalid
properties is unspecified. All arithmetic and slices are checked, including
alignment additions. Invalid input returns only an error, never a usable prefix.

## Restricted newc profile

Only uncompressed ASCII `070701` is accepted. CRC `070702`, old ASCII/binary,
compression and concatenated archives are unsupported. Every header starts at
a four-byte archive offset and contains exactly 110 bytes: six magic bytes,
then thirteen eight-byte ASCII hexadecimal unsigned 32-bit fields in this order:
ino, mode, uid, gid, nlink, mtime, filesize, devmajor, devminor, rdevmajor,
rdevminor, namesize, check. Accept upper/lower hex digits, not signs, spaces or
NULs. Parse fields by checked byte decoding, not casts or host endianness.

For each record calculate name_start=header_start+110, name_end=name_start+
namesize, data_start=align4(name_end), data_end=data_start+filesize,
next=align4(data_end). All intervals must fit the input before reading. namesize
is 2..129 for ordinary entries, includes exactly one terminal NUL, and excludes
interior NULs. Both padding intervals must contain only zero bytes. Data may
contain arbitrary bytes, including embedded archive magic or trailer text.
The cursor advances strictly after each header/name/data; no scans for magic.

Ordinary modes are precisely regular-file type 0100000 or directory type 0040000
plus any low 0777 permission bits. Reject other type or upper/special permission
bits, including symlinks, devices, sockets, FIFO, setuid, setgid and sticky.
Require rdevmajor=rdevminor=check=0. Require regular-file nlink=1; directories
have nlink>=1 and filesize=0. The workspace filesystem reports directory
link counts of1, so these positive descriptive counts are admitted. Directory
link counts do not create links and are
not checked against child counts. uid, gid and mtime accept any decoded u32 but
confer no identity or authority. devmajor/devminor/ino are inert identity keys:
reject a repeated triple among all ordinary entries, even when nlink claims1.
This excludes ambiguous inode aliases rather than implementing hardlinks.

Paths are case-sensitive relative ASCII. Every component contains one or more
`A-Z a-z 0-9 . _ -` bytes, but is neither `.` nor `..`. Reject absolute names,
backslashes, repeated or trailing slashes, control/non-ASCII bytes, embedded
NULs and empty components. There is no implicit root entry or `./` stripping.
Reject every duplicate full path. Every proper directory prefix must already
exist as an explicit Directory entry; `bin/app` requires an earlier `bin`.
A regular file can never become an ancestor. Enumeration preserves archive
order; sorting is unnecessary. Bounded pairwise checks cost at most 64 squared
path comparisons of at most128 bytes, plus linear input/padding inspection.

Exactly one `TRAILER!!!` name terminates records. It is reserved and never an
ordinary file. Its namesize is11 including NUL; mode/filesize/ino/uid/gid/mtime/
devmajor/devminor/rdevmajor/rdevminor/check are zero and nlink is1. Apply the
same four-byte padding validation. Remaining input is 0..511 zero bytes only;
this admits no tail or standard512-byte block padding, while rejecting further
records, oversized padding and nonzero junk. Missing, truncated or malformed
trailers reject the entire archive even if earlier files are valid. A second
trailer is nonzero trailing data and is rejected.

These are deliberately narrower rules than general cpio: no extraction-time
normalization, alternate record types, hardlink resolution, permissive padding,
implicit parents, root marker or archive concatenation. They are an explicit
native boot-input profile, not a claim to accept arbitrary Linux initramfs.

## Reproducibility and verification gates

A deterministic host builder emits a committed archive containing explicit
`bin` and native ELF entries, stable metadata, unique inode keys, zero padding
and trailer. Record exact bytes, SHA256 and source ELF hashes. Normal checks
rebuild and compare without updating canonical files. Independently list the
archive with pinned GNU cpio, compare names/types/sizes with a manifest, and
independently extract individual known payloads with GNU cpio `--to-stdout`
to compare hashes without filesystem extraction. Never extract untrusted
malformed inputs in the host test suite.

Also construct a real GNU `cpio --format=newc` archive from an explicit ordered
list of relative names without `.`/`./` and without hardlinked source files.
Use ordinary permission bits, explicit parent directories and default512-byte
blocking. Record tool version/commands/listing and parse that result with the
actual safe module. Any GNU metadata incompatibility is investigated before
changing this contract; do not silently relax admission to make a test pass.
Reproducibility applies to the deterministic canonical builder; GNU filesystem
inode/mtime values need not reproduce byte-for-byte.

Host tests cover valid empty/max64 archives, all alignment residues, empty
files, exact file/archive/path limits, upper/lower hex, descriptive metadata,
and byte-exact regular data. Reject every bad header/magic/hex field, oversized
or truncated interval, nonzero padding, malformed name/path/type/link metadata,
duplicate path/inode, missing/late/non-directory parent, malformed/missing/
duplicate trailer and excess/nonzero trailing data. A valid requested ELF early
in an archive followed by corruption must fail parse before lookup is possible.
Include exhaustive truncations of small fixtures and bounded deterministic
mutations, with panic treated as failure. Generate large limit inputs on the
host; do not place multi-MiB arrays on kernel stacks.

Actual-type compiler clients reject forged Archive/Entry fields, mutable data
access, entries escaping input lifetime and input mutation while an entry is
live. Positive clients may drop Archive while retaining copied Entry provided
the input outlives it; they may parse Entry bytes as an ELF Image. Test both
supported kernel compilation targets without adding unsafe exceptions.

Later integration replaces the embedded-ELF selection with a fully parsed
embedded archive and exact `bin/...` lookup, then invokes the unchanged
Image::parse -> Factory::load -> Executor::prepare/run pipeline. Test missing
entry and directory selection as recoverable errors before frame allocation;
ELF errors remain independently reported. The archive borrow may end after
owned loading has copied bytes. Run actual native output/exit, invalid-call,
fault/deadline and reclamation cases from archive entries, with independently
verified archive/ELF hashes and existing strict hardware trace gates. Parsing
or listing alone does not satisfy the archive-backed execution gate. Adding
archive bytes can move kernel BSS: rebuild and rebind the existing kernelwrite
fixture using its reviewed fixed-point procedure, never loosen address checks.
Reinspect compiled stack bounds before guest execution.

## Primary provenance

The [libarchive cpio(5) format specification](https://github.com/libarchive/libarchive/blob/master/libarchive/cpio.5)
defines newc field widths/order, NUL-terminated names, four-byte padding and
trailer semantics. The [GNU cpio manual](https://www.gnu.org/software/cpio/manual/cpio.html)
defines tool format/listing/blocking options; pinned observed GNU output is the
interoperability evidence. Our limits, path alphabet, parent ordering, metadata
restrictions and complete-admission API are design choices. No Linux kernel
implementation source is consulted or translated.
