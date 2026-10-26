# Owned bounded native startup input

Status: Stage A safe codec/API approved by independent
[review 0027](../reviews/0027-native-startup-input.md), 2026-10-26. Stage B remains
draft/unapproved; runtime/wire/monitor integration requires separate source,
stack and execution review. No spawn, process scheduling or IPC is implemented
by either stage. [ADR0005](../adr/0005-native-first-design.md) supplies the native
ownership direction; [0026](0026-native-launcher.md) supplies ordinary launch.

## Stage A: safe owned input and canonical codec

Add only `hm_kernel::startup_input`, with `#![forbid(unsafe_code)]`, no allocation,
no dependencies and no hardware access. Do not change native.rs, existing startup
bytes or core behavior in this stage. Freeze these public constants:

- `MAX_INPUT_SIZE: usize = 256`.
- `EXTENSION_OFFSET: usize = 64` (absolute offset within startup page).
- `EXTENSION_SIZE: usize = 4032` (remaining bytes of the existing4096-byte page).
- `HEADER_SIZE: usize = 24` (within the extension).
- `PAYLOAD_OFFSET: usize = 24` (extension-relative, absolute88).

```rust
pub enum Error { Length, Size, Magic, Version, Header, Reserved, Padding }
pub struct Input { /* private owned bytes, length and presence */ }
impl Input {
    pub const fn absent() -> Self;
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error>;
    pub fn as_bytes(&self) -> Option<&[u8]>;
    pub fn encode_into(&self, extension: &mut [u8]) -> Result<(), Error>;
}
pub struct View<'a> { /* private immutable extension borrow and presence */ }
impl<'a> View<'a> {
    pub fn is_present(self) -> bool;
    pub fn bytes(self) -> &'a [u8];
}
pub fn decode(extension: &[u8]) -> Result<View<'_>, Error>;
```

Error is Copy/Clone/Debug/Eq/PartialEq. Input is Debug but neither Copy nor Clone;
View is Copy/Clone/Debug/Eq/PartialEq. No public fields or unchecked constructors.
Input owns a fixed256-byte array and bounded private length/presence. from_bytes
copies arbitrary bytes before returning, rejecting length257+ with Length. An
absent Input returns None; present-empty returns Some(empty). Source mutation or
drop after construction cannot alter the object. No borrowed source pointer is
retained. as_bytes borrows only the Input and cannot outlive it.

encode_into requires exactly4032 destination bytes, returning Size before any
mutation otherwise. After that sole fallible check, it zeroes the destination
and writes canonical bytes directly into it. Absent encodes entirely zero.
Present, including present-empty, encodes this explicit little-endian header:

| Absolute offset | Extension offset | Bytes | Meaning |
| --- | --- | --- | --- |
| 64 | 0 | 8 | `HMINPUT\0` (seven ASCII letters plus NUL) |
| 72 | 8 | 4 | Extension version1 |
| 76 | 12 | 4 | Header length24 |
| 80 | 16 | 4 | Payload length0..256 |
| 84 | 20 | 4 | Reserved zero |
| 88 | 24 | length | Opaque input bytes |

Every byte after the payload through the end of the extension is zero. No UTF-8,
NUL termination, pointer, argument vector, Rust layout, alignment-dependent cast
or host endianness is implied. Encode need not materialize a4032-byte temporary:
its caller supplies destination storage. Ordinary Input storage is approximately
256 bytes; exact Rust size is internal, never a wire layout.

decode requires exactly4032 bytes (Size otherwise), checks the complete slice,
and returns absent only if every byte is zero. Otherwise require exact magic,
version1, header length24, reserved zero, payload length<=256 and zero full tail.
Errors correspond to Magic/Version/Header/Reserved/Length/Padding; precedence
for multiple defects is unspecified. Use checked arithmetic/slicing throughout.
The returned private View exposes only the admitted payload; absent bytes() is
empty. A forged View cannot be constructed through safe public APIs. Arbitrary
input must not panic or allocate. Payload bytes may include any header magic,
zeros, non-UTF8 bytes and terminal controls without being interpreted by codec.

Stage A has a separate `cargo --test startup_input` gate, never mixing existing
native/initramfs test inventories. Freeze fourteen test names:
`absent_canonical`, `present_empty_distinct`, `opaque_round_trip`,
`all_input_lengths`, `maximum_and_over_limit`, `owns_input_copy`,
`wrong_destination_size_unchanged`, `exact_decode_extent`, `malformed_header`,
`malformed_lengths`, `malformed_tail`, `reencoding_clears_stale_bytes`,
`legacy_prefix_unchanged`, `deterministic_mutations`.

These cover absent versus present-empty exact bytes, lengths0/1/255/256/257 and
all admitted lengths, all256 byte values including NUL/nonUTF8, copying before
source mutation/drop, wrong-size sentinel preservation, every truncation of4032
and size4033, all header fields/magic, noncanonical absent, full-tail nonzero
mutations, reuse of a previously longer encoding, and exact first64 bytes
unchanged when encoding a page's extension slice. Large test arrays belong on
the host, not kernel stacks. Deterministic mutations treat every panic as failure.
Actual-module compiler probes reject Input/View field forging, Input cloning,
borrows escaping their owner and View escaping/mutating extension storage.
Positive probes drop/overwrite source after construction, move Input, and use
View bytes while the extension remains alive. Host tools enforce exact inventory,
exit status, timeouts and missing-case failures. Build both no_std kernel targets
and retain all existing gates. Passing Stage A proves codec/ownership only.

## Stage B draft: explicitly opt-in startup extension

The existing v1 Session::startup() first64 bytes remain byte-for-byte unchanged,
including version1, length64, features and reserved word56. SVC request version1
and all operations/budgets/handles stay unchanged. The extension has its own
explicit version and magic; it does not reinterpret the base header's reserved
word or silently assign an undocumented feature bit.

Historical Executor::prepare keeps its exact all-zero startup-page remainder
contract from [0023 native wire](0023-native-wire.md). A new opt-in preparation
path accepts an already owned validated Input and installs the extension.
Absent input remains identical to historical startup; present-empty has a header
and is intentionally distinct. This is an explicit amendment for the opt-in path,
not a claim that every old v1 consumer tolerates a nonzero tail. Old programs
launched without input remain compatible. New echo understands the extension;
a legacy program that insists its entire tail is zero can reject supplied input
normally without affecting isolation. Do not silently discard nonempty input
or pretend a successful legacy exit proves it consumed the bytes.

The remaining integration details and hardware gates are specified below before
runtime implementation approval; Stage A may proceed independently after review.

### Proposed runtime composition and page geometry

Preserve `Executor::prepare(space, console, budget)` unchanged for all existing
callers. Add `prepare_with_input(space, console, budget, input: &Input)` returning
the same owned Process/error-with-original-Space types, with no input lifetime
parameter on Process. Both use a private shared preparation function accepting
Option<&Input>; legacy None must not create a256-byte temporary or encode a
nonzero extension. Input borrowing ends before the preparation call returns.
The caller can then mutate/drop Input before run; no source pointer, buffer,
callback or Rust reference into caller storage is retained in Process/Session.

Validate arbitrary input length by Input::from_bytes before archive loading,
timer initialization, issuer generation or frame allocation. All entries into
the new safe API require this already validated, unforgeable owned type. Fully
admit archive/path/ELF before issuance as today. Later allocation/prepare errors
may still consume a generation under the existing documented monotonic contract;
input-validation errors must not. No retry may reuse an issued generation.

The private Space startup installer receives the unchanged64-byte base and an
optional borrowed Input. Allocate one existing zeroed startup Frame, copy base,
and encode directly into its mutable byte slice64..4096 while boot mappings are
active and before any user leaf is published. Only a complete successful copy
may commit the fixed descriptor/owner/count. On any error, drop the local frame
and return the original inactive Space with no startup mapping. Encoder Size
failure here would be an internal geometry error, not a user-controlled partial
copy; handle it without publishing a modified Space. Never encode through a
user virtual pointer or into an already installed/mapped frame owner.

The startup page remains at0x1_0010_0000, user RO and PXN/UXN, with the same
reviewed descriptor. Input spans absolute addresses0x1_0010_0058 through at most
0x1_0010_0157. No new page, guard or executable alias is introduced. Frame budgets
remain one extra startup frame (maximum285 total); stack and ELF image ranges
are unchanged. The startup identity alias is kernel RW/NX data, not executable
code. All RX code alias restrictions and cache-maintenance rules stay unchanged.
No mutable Frame reference escapes installation; only owned inactive Space is
returned. Run still excludes peers/callbacks and executes no Rust under the
process root, including when the original Input lived in the omitted owned arena.

Extend the safe Platform boundary with run_native_with_input(path, &Input),
keeping existing run_native(path) for legacy absent input. ARM delegates through
one private launcher implementation with optional input and the same persistent
Executor; x86 remains Unsupported. Old no-input calls should not incur an owned
Input-sized temporary. Native status, generation reporting, output transport,
resource outcomes and quiescence policy stay unchanged. Last prepared generation
is set only after successful preparation; malformed input consumes none.

### Proposed monitor text and echo behavior

Keep InputLine capacity64 and printable-ASCII admission. `run <path>` means absent
input. Split the command's remainder on its first ASCII space only: `run <path>
<text>` supplies every byte after that one delimiter as present opaque input,
including further leading/trailing spaces. A trailing delimiter supplies
present-empty. The path must be nonempty with no whitespace and remains an exact
archive lookup. Thus `run  bin/echo` is invalid (empty path), while
`run bin/echo  hi ` supplies ` hi `. No quotes, escape expansion, argv, environment
or UTF-8 interpretation is added. Bare `run`/`run ` still print usage. The source
line is copied into Input before core launch; the monitor never passes a borrowed
line tail as a retained startup pointer. Direct typed callers can supply all256
bytes even though the interactive64-byte command limit permits less.

Add separately linked `bin/echo` to the normal application archive (seven ordinary
entries, five programs). It validates the existing base header, calls the reviewed
extension decoder, writes the admitted payload exactly, writes one newline, and
exits0. Absent and present-empty both print a newline; they remain distinguishable
through View::is_present for programs that care. A zero-length first write still
uses the valid console grant but produces no output under the existing ABI.
Use the actual safe codec source or compiled dependency-free library in the
native app, with explicit source hashes; do not duplicate an untested raw decoder.
Payload length256 fits a single existing maximum write; the newline is a separate
write, so each echo has two WRITE SVCs plus EXIT (three attempts, at most257
successful bytes). Native wire version and console budgets do not increase.
Terminal LF-to-CRLF conversion remains transport only; Trace mode records original
bytes. Passing input is data transfer, not extra authority: it creates no handle,
resource grant, filesystem access, spawn right or shared memory.

### Proposed Stage B verification and review gate

Before core code, independently review the concrete installer/shared prepare
change, source lifetimes, decoder reuse and monitor grammar. Preserve all existing
v1 exact-header/zero-tail host tests and all33-case/two-pass process fixtures.
Legacy prepare must reproduce their full startup pages exactly. New compiler
clients prove Input may be dropped after prepare_with_input before run, Process
cannot outlive Pool, unvalidated/forged Input cannot reach the core, and no public
mutable startup owner or raw mapping authority is exposed.

Add host UI tests for absent, present-empty, embedded/multiple/trailing spaces,
invalid path prefix, longest admitted line, overflow rejection/recovery and typed
length257 failure. Input length failures and invalid archive/path/ELF selection
must preserve current frame count and last prepared generation. Add core inactive
startup inspection tests comparing base64, exact extension encoding and zero
remainder for absent/empty/1/255/256-byte inputs. Include ownership tests that
mutate/drop the original source and Input after preparation, then verify/run
using only the owned process copy. Budget0 and exhausted-pool preparation failures
must return an unchanged original Space, with no startup leaf or frame leak.

Run the ordinary monitor echo with absent, explicit empty, text and repeated
spaces, followed by legacy hello/info and the existing fault/deadline/test/hello
recovery sequence. Compare exact user-visible output and real ELF-based SVC
locations, prepared-generation progression and per-launch frame reclamation.
Preserve old fresh-boot and warm-up profiles; input must not become a prerequisite
for any old app or initialize a second executor. Update the archive builder and
strict artifact/listing/hash checks for echo, without raw embedded ELF fallback.

A separate bounded hardware-input profile must exercise the typed256-byte path
because the interactive line limit cannot reach it. Its independently generated
fixture expectations include absent, present-empty, one byte,255 bytes, all256
byte values, then another absent launch to catch retained/stale bytes. Real EL0
code must read/validate presence, length, payload and zero tail, and execute the
same existing write/exit ABI; distinguish absent versus empty through checked
exit status or an explicit test result, not kernel metadata alone. Use the sealed
Trace sink for arbitrary binary output so terminal escapes cannot confuse prompt
parsing. Add real writes to the RO payload location and execution attempts there,
requiring exact lower-EL permission faults and full subsequent recovery. Invalid
length257 is rejected before any EL0 entry or generation. Freeze the exact new
profile inventory/serial grammar/ELF symbols in its implementation review; none
of these cases may replace or weaken the existing66-run process matrix.

Reinspect compiled maximum stacks for every affected normal, process, baseline
and negative feature profile before guests. Only about1952 bytes of normal launch
headroom existed at the preceding verified milestone; an extra owned Input,
Result return slot or inlined codec can exceed it. Never put a4096-byte encoded
page temporary on that chain. Keep command parsing/admission/loading/prepare
helpers separately bounded and use direct destination encoding; actual compiler
prologues, not source estimates, decide admission. Retain all ARM/x86 regressions,
strict missing-case failures and failed candidates. Stage B cannot be labeled
verified from Stage A host tests alone.

## Later services boundary

Owned startup bytes establish a small request-copy pattern: validate bounds,
copy into one owner's storage, end the sender borrow, then execute with explicit
existing grants. A later capability-scoped spawn API can build on that pattern,
but must separately define who may spawn, which grants can be transferred or
attenuated, resource accounting, child ownership, wait/cancellation, scheduling
and teardown. Service IPC additionally needs message ownership, queue bounds,
backpressure and reply/cancellation lifetimes. This synchronous startup copy
supplies none of those mechanisms and makes no spawn/service completion claim.
