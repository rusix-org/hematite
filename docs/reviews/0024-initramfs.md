# Initramfs admission review

2026-10-26. Independent AI review, not human approval or a complete soundness
audit.

**The safe parser, integration tests and verification tools are source-approved;
host admission and ARM compiler evidence are independently verified below.**
Archive-backed boot/process integration needs a separate source,
compiled-stack and execution review; this approval authorizes no core change.

## Admission and API

The borrowed, allocation-free API publishes an Archive only after every record,
the terminal record and the complete tail are admitted. This prevents an early
valid init executable from hiding a corrupt suffix. Entry copies retain the
input lifetime and expose immutable slices, not mappings, mutable bytes or
execution authority. Exact lookup does not normalize paths. The bounded entry
array and bounded names make the pairwise identity/hierarchy checks finite;
every cursor addition, alignment and slice must remain checked in source.

The profile explicitly distinguishes cpio representation from native policy:
newc only, regular files/directories, canonical relative ASCII paths, earlier
explicit parents, unique paths and device/inode triples, zero padding, one
strict trailer and at most 511 zero tail bytes. All 64 entries include directories
but exclude the trailer. The 128-byte name bound excludes NUL, regular empty
files and empty archives are valid, and the 4MiB bound includes all overhead.
Archive modes and ownership/timestamp fields grant no native capability. These
restrictions do not claim support for arbitrary Linux initramfs or all cpio
producer options, directory ordering, block sizes, hardlinks or compression.

The initial directory link-count minimum of two was corrected before
implementation: actual `os.stat` observations on this workspace report one for
the repository, crates and docs directories. The revised profile admits any
positive directory count as descriptive metadata. Files still require exactly
one link, directory payloads remain empty and duplicate inode keys are rejected.
Tests must include directory counts one and two plus rejection of zero.

Independently consulted the
[libarchive cpio(5) format specification](https://github.com/libarchive/libarchive/blob/master/libarchive/cpio.5)
for the 110-byte newc header, field ordering, name/data alignment and trailer,
and the [GNU cpio manual](https://www.gnu.org/software/cpio/manual/cpio.html)
for the producer/tool interface. Numeric restrictions, canonical path policy
and complete admission are this design's choices. No kernel implementation
source was used or translated.

## Required evidence and later boundary

Source review must check full header decoding, terminal recognition before
ordinary-name validation, monotonic cursor movement, complete suffix rejection,
and absence of panic/unchecked indexing for arbitrary bytes. Tests must cover
all thirteen malformed numeric fields, boundary and alignment residues, exact
limits and one beyond, identity aliases across entry types, file-as-parent,
missing/late parents, malformed/duplicate trailers and a valid executable
followed by corruption. Exhaustive truncations apply to small fixtures;
large limit cases belong on the host, not kernel stacks.

Require actual-type clients on both targets for private construction,
immutability and input lifetimes, including a valid copied Entry that outlives
Archive while its input remains alive. A deterministic builder must reproduce
committed bytes, while pinned GNU listing/extraction and an independently
produced GNU archive provide format-interoperability evidence. Extract only
the trusted generated fixture in a fresh bounded directory, never malformed
test inputs. Freeze the exact host-test inventory before accepting results.

The later integration must select a regular entry after complete archive
admission and feed its bytes through the existing ELF/owned-space/process APIs.
Missing/directory selection and ELF errors must precede frame allocation.
Parsing success alone does not prove archive-backed execution. Embedding the
archive changes linked geometry and can increase live stack frames; retain the
kernel-data fixed-point check, independent payload hashes, actual process trace
gates and a fresh aggregate stack review. The proposed integration paragraph
is a plan, not an implementation approval or a completed Phase 2 gate.

## Source and evidence verdict

Reviewed `crates/hm-kernel/src/initramfs.rs`: all thirteen fields are decoded
before use; range ends and alignment use checked arithmetic and checked slices.
The cursor starts aligned and advances past a complete positive-length header
and name. The private count starts at zero, is checked against 64 before each
write and increments only once per admitted ordinary entry; this justifies the
fixed-array indexing and iterator slice. No arbitrary-input panic, allocation
or unsafe path was found. Trailer admission checks every field and the entire
suffix before the local plan can escape. Copied entries retain immutable input
borrows and do not depend on the plan's lifetime.

Reviewed all sixteen Rust integration cases. Initial coverage gaps were fixed:
directory/file duplicate paths and inode keys now appear in both orders, the
64-entry limit includes a directory, and late-parent rejection uses a genuine
directory. Missing GNU-reference environment/input fails the test rather than
skipping it. Canonical and GNU payloads are compared byte-for-byte with the
native ELF and banner; ELF planning is checked separately. Small-archive
truncations, all numeric-field positions, suffix corruption, limit boundaries
and deterministic mutations exercise complete admission. Mutation coverage is
bounded testing, not an exhaustive proof or a completed fuzzing campaign.

Reviewed `build_initramfs.py`, `check_initramfs.py`,
`check_initramfs_types.py` and `test_test_initramfs.py`. The builder compares
canonical bytes and manifest, independently creates a GNU archive, requires
exact listing metadata and extracts trusted payloads only to stdout. The host
harness requires the exact sixteen test names/order/results plus successful
process exit, deadline and bounded logs. Compiler clients exercise the actual
library, with four positive clients and seven specific diagnostic rejections.

Independently inspected preserved [host results](../evidence/initramfs-second-host/result.json)
and raw stdout: all sixteen tests passed without skips. All recorded source
hashes match the reviewed files. The [GNU evidence](../evidence/initramfs-first-fixture/result.json)
records GNU cpio 2.13, successful creation/listing/extraction, and byte-exact
payloads from both archives. Its observed directory link count is one, directly
covering the revised profile. Inspected the [ARM type results](../evidence/initramfs-first-types/result.json)
and raw diagnostics: all eleven clients have the expected outcome, with only
the intended error code for each negative client (unused-import warnings are
irrelevant). This review ran no builds or guests. x86 compiler and full-suite
regression evidence remain separate; no archive-backed execution is claimed.
