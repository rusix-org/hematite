# Native execution from the checked archive

Status: implemented and locally verified, 2026-10-26, with independent
[source/trace review](../reviews/0025-archive-execution.md),
[compiled-stack review](../evidence/archive-process-stack-review.md) and
[complete ARM/x86 evidence](../evidence/archive-process-execution.md). [0024](0024-initramfs.md) owns the archive admission contract;
[0023](0023-arm-native-process.md) owns execution and its
[verification contract](0023-process-verification.md). This milestone connects
those existing safe APIs without changing Executor, page tables, CPU0/peer-OFF
admission, native wire policy, interrupt handling or teardown.

The initial scope is the actual archive-backed `process-tests` boot profile.
Keep the ordinary monitor boot unchanged: a default init service needs a later
explicit startup/lifecycle decision. A passing profile is runnable isolated
native execution from an embedded initramfs, not a general boot filesystem,
init service, Linux binary execution or external bootloader-supplied archive.
No new unsafe mechanism, decompressor, filesystem extraction or guest allocation
is needed for archive admission.

## Safe composition and lifetimes

Add a small safe `hm_kernel::boot_image` module, with `#![forbid(unsafe_code)]`:

```rust
pub fn admit<'a>(archive: &'a [u8], path: &str)
    -> Result<elf::Image<'a>, AdmissionError>;

pub enum AdmissionError {
    Archive(initramfs::Error),
    Missing,
    Directory,
    Elf(elf::Error),
}
```

The function must call Archive::parse on the complete input first, then perform
exact lookup, require Kind::File, and call Image::parse on the entry bytes.
Directory selection must remain distinct from missing or invalid ELF. No early
scan for a requested name, normalization, alternate paths, fallback ELF, or
partial archive acceptance is permitted. A malformed suffix wins over a valid
requested entry and over an otherwise missing name. Error internals may be
wrapped unchanged; error-order differences within Archive or ELF retain their
existing contracts. The path argument is not borrowed by the returned Image.

Only Image escapes: it borrows immutable archive payload bytes, not the local
Archive plan. After Factory::load has copied into owned frames, Image and the
archive borrow can end. Returning Image must not require leaking an Archive or
forging a lifetime. Actual-type compiler clients cover a positive temporary
path argument, a positive Archive-independent Image borrow, input mutation while
Image is live, and Image escaping input storage.

Call admit for every run and preparation check rather than keeping a large
Archive plan live across load/prepare/run. Revalidating a bounded archive is an
intentional initial cost, measured separately if needed; no concurrency or
performance claim follows. The process case list retains only case names and
exact archive paths, never raw include_bytes ELF alternatives. Preparation
Budget/Exhausted/drop tests also obtain their hello image through admit.

## Canonical inputs and artifact binding

Add `crates/hm-core/fixtures/process-initramfs.cpio`, with one deterministic
archive order: `bin`, the existing 33 `bin/<case>` regular ELF files in the
frozen process case order, `etc`, and `etc/banner`. The banner is the existing
`hematite native initramfs\n` byte string and provides an invalid-ELF selection.
This is 36 ordinary entries (two directories, 34 files); trailer is not counted.
All bounds and metadata are exactly 0024: at most64 entries, 4MiB total,
1MiB per file, 128-byte paths, unique identity keys and explicit earlier parents.
The builder fails if any bound is exceeded; no parser limit is increased.

The process profile embeds this complete archive once. Retain the existing
small `initramfs.cpio` parser fixture for its independent host gate, but do not
embed it or individually embedded process ELF byte arrays in the process kernel.
The standalone 33 ELF files remain canonical build inputs and independent
readelf/disassembly references, not an execution fallback.

Extend deterministic fixture tooling with a separate process-archive mode/output
and manifest, reusing its reviewed encoder. Record archive SHA256/length, exact
entry order/path/type/mode/length/payload hashes, and the source process manifest
hash. GNU cpio must independently list the archive and extract each known file
to stdout for byte comparison against the standalone ELF/banner input. Normal
verification rebuilds without update and compares exact canonical archive and
manifest bytes. Preserve the real GNU-produced reference gate from0024.

Before QEMU, the host verifier checks the archive manifest and GNU comparisons,
all33 extracted ELF payload hashes against the process manifest, and existing
ELF symbols/opcodes/segment metadata. It then locates the complete exact archive
byte sequence across all built-kernel file-backed PT_LOAD contents, requires
exactly one occurrence across those contents, and records its ELF file offset
and SHA256. Check all segment/file interval arithmetic and require the entire
archive span within one PT_LOAD with PF_R set and PF_W clear.
Do not accept a manifest hash merely printed by the guest as proof of embedding.
Reject an archive match outside file-backed read-only kernel storage. Check that
no complete standalone process ELF payload occurs outside that archive span in
kernel load contents; this detects accidental retained raw-input embeddings.
Record the built kernel hash as usual. Source review additionally confirms the
case lookup uses that embedded array; byte presence alone does not prove use.

Kernel-sensitive fault targets remain exact. Updating canonical inputs is an
explicit bounded fixed-point operation, separate from normal checks:

1. Starting from a recorded candidate kernel __bss_start, regenerate the33
   canonical process ELFs and process manifest with that absolute address.
2. Regenerate the small parser archive from the resulting hello ELF, then the
   process archive and both manifests from these exact canonical bytes.
3. Build the process kernel, read actual __text_start/__bss_start independently,
   and compare with the target address recorded in every relevant manifest/ELF.
4. If BSS moved, repeat steps1–3 with the observed value; stop after at most
   four iterations and report failure rather than accepting a stale binding.

Once stable, rerun all ordinary non-updating reproducibility checks. Existing
host checks still require each ELF absolute kernel_data symbol and the process
manifest value to equal actual kernel __bss_start. Archive embedding may change
BSS, so old evidence or a successful previous ELF-only build cannot substitute.
Store iteration commands/hashes/symbol observations, including failures.

## Negative admission and execution gates

Before allocating process frames, run four safe admission checks:

- malformed archive, including an apparently valid early named regular entry
  followed by a malformed suffix: Archive error, never an ELF/Missing result;
- valid process archive with exact nonexistent `bin/missing`: Missing;
- valid process archive with `bin`: Directory;
- valid process archive with `etc/banner`: Elf error.

Use a small bounded malformed fixture for the guest suffix case, constructed as
constant bytes or on a small stack buffer; its early payload need not be a valid
ELF because Archive rejection must precede ELF admission. Host helper tests also
use a genuinely valid ELF early followed by corrupt suffix. Do not clone the
multi-MiB archive onto the kernel stack or expose a mutated valid prefix.
Every negative call checks Pool free count unchanged and produces no Space,
Process, handle generation or EL0 transition. Missing/directory/ELF cases use
the actual embedded valid archive, not toy replacement metadata.

Extend the strict process serial grammar immediately after the existing
`HMPROC PREFLIGHT checks=15 irqs=8` and before `HMPROC BEGIN 66`:

```text
HMPROC ARCHIVE bytes=<n> entries=36 files=34
HMPROC ADMISSION case=malformed error=archive before=<n> after=<n>
HMPROC ADMISSION case=missing error=missing before=<n> after=<n>
HMPROC ADMISSION case=directory error=directory before=<n> after=<n>
HMPROC ADMISSION case=invalidelf error=elf before=<n> after=<n>
```

Require exactly this order, exact field sets, archive length equal to verified
artifact bytes, and identical before/after free counts across all four records. These counts
must also equal the common baseline recorded by every one of the 66 runs,
excluding a leak between admission checks and BEGIN.
Missing, duplicated, reordered or unexpected records fail. The preflight and
all subsequent HMPROC records retain their existing meaning. These admission
checks cause no architectural exceptions or PSCI calls; the independently
parsed hardware transaction counts stay unchanged. Host parser mutation tests
must reject incorrect archive size/counts, missing/error-mismatched admission
cases and changed free counts.

Execute the full existing33-case/two-pass66-run matrix through archive lookup,
including all recoverable calls, register checks, real protection faults,
privileged instructions, deadline escape and repeated exact frame reclamation.
Preserve all three RAM/CPU profiles, baseline kernel tests, private callback-free
execution, strict EL0 trace correlation, exact syscall/event expectations,
prepare failure/drop checks, and pinned-emulator qualifications. Archive lookup
must not consume issuer generations, so expected handle sequences are unchanged.
No reduction to hello-only execution is sufficient for this integration gate.

Host tests for the composition helper cover each error variant, valid native
Image bytes/entry, malformed complete suffix despite early valid ELF, missing
name in malformed archive, and exact lookup semantics. Existing archive parser
and ELF mutation suites remain required. Inspect compiled call-chain stack
bounds with the Archive parser/returned Image temporaries included before any
guest run; source estimates or only the previous process frame audit are
insufficient. Preserve failures and run the complete ARM64/x86 regression suite.

Successful evidence establishes the archive-backed input-source portion of
0021 together with the already established execution mechanisms. Phase2 exit
claims must still enumerate remaining requirements and CI publication status;
this plan neither supplies Linux compatibility nor silently finishes other gates.
