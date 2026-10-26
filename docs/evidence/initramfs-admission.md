# Checked initramfs admission

2026-10-26, implementation `cf976ef`, based on parent commit `46b9df4`. This implements the safe input
boundary in [design 0024](../design/0024-initramfs.md), with
[independent AI review](../reviews/0024-initramfs.md). The [full ARM/x86 regression suite](initramfs-final/verification-summary.json)
passes, with [raw log](initramfs-final-test-all.log) and exact
[source identities](initramfs-final/source-sha256.txt). Archive-backed execution
is a separate, still-open gate.

The allocation-free parser validates every record and the complete trailer/tail
before exposing entries. It admits bounded uncompressed newc regular files and
explicit directories, checks canonical relative paths and earlier parents, and
rejects duplicate names/inode identities, links, devices, special permissions,
malformed fields and padding. Its private immutable entries borrow the input;
metadata does not confer memory or execution authority.

## Direct evidence

`make test-all` exited zero. Its final archive contains 77 passing result-bearing
ARM records and twelve x86 records. All 123 Python harness tests pass per target,
including eight new archive-harness tests. The [sixteen actual parser tests](initramfs-final/aarch64/initramfs-host/result.json),
[GNU fixture verification](initramfs-final/aarch64/initramfs-fixture/result.json)
and eleven actual-library compiler clients on
[ARM](initramfs-final/aarch64/initramfs-types/result.json) and
[x86](initramfs-final/x86_64/initramfs-types/result.json) all pass. Existing
198 process runs, baselines, owned spaces, scheduler/SMP, negative profiles and
monitor checks also pass. Run `make test-all` to reproduce the full gate;
`make test` selects ARM and includes the host GNU/parser tests.


- [GNU fixture verification](initramfs-first-fixture/result.json): the
  deterministic canonical archive reproduces native hello ELF bytes and a
  banner, with explicit directories. Pinned GNU cpio 2.13 independently lists
  names/types/modes/sizes and extracts exact payloads to stdout. It also creates
  a separate newc reference from known staged files. No malformed archive is
  extracted to the host filesystem.
- [Sixteen actual Rust tests](initramfs-second-host/result.json): canonical and
  GNU archives feed the real parser and existing ELF admission. Coverage includes
  complete-admission rejection of a corrupt suffix after a valid ELF, all header
  field positions, all small-input truncations, padding/trailers, traversal,
  hierarchy/duplicate aliases, exact 64-entry/128-byte-path/1MiB-file/4MiB-archive
  limits, all alignment residues, zero-copy borrow identity and 4096 deterministic
  mutations. These are bounded tests, not exhaustive fuzzing or runtime isolation.
- [Eleven ARM compiler clients](initramfs-first-types/result.json): four valid
  uses and seven expected diagnostics reject forging, mutable input/data access
  and lifetime escapes against the actual library. Entry may outlive Archive
  only while the immutable input still exists.
- Eight host-harness tests reject missing/duplicate/filtered cases, malformed
  summaries, failed processes, timeouts, oversized logs and incorrect GNU
  listings. Missing GNU reference input is a failure, never a skipped Rust test.

Preimplementation review found real directory link counts of one on this
filesystem. Directory nlink is therefore positive descriptive metadata; regular
files still require exactly one link and all inode identities must be unique.
The GNU reference dynamically covers this case. The initial formatting check
and three test-only Clippy findings are preserved in the first build/host logs;
module ordering, a single-element loop and two small Vec expressions were fixed.
The parser required no runtime correction after source review.

The Dockerfile adds GNU cpio from its existing pinned Debian snapshot. Rust and
QEMU pins are unchanged. No new unsafe code or guest entry path is introduced.
The existing process ELF remains byte-identical to the previously reviewed
`d8505cb2973854287b474e8d08175be76af9e9957e3e641d2a67d342c8269d52` binary.
This is archive admission only: the guest still selects its earlier embedded
ELFs. Initramfs-to-process integration, a filesystem, Linux compatibility and
physical hardware remain outside this evidence. No new remote CI is claimed.
