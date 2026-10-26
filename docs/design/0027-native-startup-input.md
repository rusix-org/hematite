# Owned bounded native startup input

Status: Stages A and B implemented and verified locally, 2026-10-26.
[Independent review](../reviews/0027-native-startup-input.md),
[codec evidence](../evidence/native-startup-input.md) and
[delivery evidence](../evidence/native-startup-delivery.md) distinguish the
host-only codec from actual protected execution. Matching remote CI remains
open. No spawn, process scheduling or IPC is implemented by either stage.
[ADR 0005](../adr/0005-native-first-design.md) supplies the native ownership
direction; [0026](0026-native-launcher.md) supplies ordinary launch.

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

## Stage B: explicitly opt-in startup extension

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

The runtime composition and hardware verification contract is specified below.

### Runtime composition and page geometry

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

### Monitor text and echo behavior

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

### Stage B verification and review gate

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

### Stage B concrete API and verification freeze (pending review)

The exact additional core API is:

```rust
pub fn prepare_with_input<'p>(
    &mut self, space: Space<'p>, console: bool, budget: usize, input: &Input,
) -> Result<Process<'p>, (Error, Space<'p>)>;
```

It retains the existing prepare error mapping: startup Budget/Exhausted become
process Budget/Exhausted, geometry/codec invariant failures become Corrupt,
context mismatch becomes Context, and retired issuance becomes Generation.
Every error returns the original inactive Space; allocating/copying the local
startup Frame never commits until all fallible steps succeed. Input is already
valid by construction, so this API has no user-controlled malformed-header case.
The Platform addition is
`fn run_native_with_input(&mut self, path: &str, input: &Input) -> Result<LaunchReport, LaunchError>`
with default Err(Unsupported). ARM uses the same persistent launcher/issuer and
existing error mapping; ordinary run_native stays the historical absent path.
Length257 is rejected at Input construction before either Platform launch API.

For inactive verification only, add a narrowly scoped core method under the new
test feature:
`pub(super) fn copy_startup(&self, offset: usize, output: &mut [u8]) -> Result<(), user_space::Error>`.
It accepts offsets only within the fixed4096-byte startup page, checks offset+
length overflow and complete containment before any write, then delegates to
Space::read_into at STARTUP+offset under verified boot context. Bad bounds return
Bounds and leave output unchanged. It exposes no arbitrary VA selection, raw
pointer, mutable Frame or mapped borrow. Borrowed Process makes this mutually
exclusive with consuming run. Inspect the full page in256-byte chunks after
both source and Input have ended/mutated, before entering EL0. Keep this method
private to core test code, not a new public process-memory API. Include exact
end/overflow/crossing negative observer cases with unchanged sentinel output.

The distinct ARM feature is `startup-input-tests`, implying boot-tests. It runs
on one CPU at64MiB and128MiB, using its own Trace Executor and the existing
BootPlatform proof. No PSCI calls are expected. Do not combine its runtime boot
selection with other special profiles; all-features compilation remains a lint
gate rather than a combined runtime scenario. Keep the complete old66-run profile
unchanged and run it separately.

A separate canonical checked `startup-input-initramfs.cpio` contains `bin` and
eight independently linked probes: absent, empty, one, bytes255, binary256,
payload_write, payload_execute, input_loop. Each is admitted through the existing
archive/ELF helper. It has no kernel absolute symbols. Its source includes the
actual reviewed decoder and validates the entire base/extension/padding at EL0
against independently declared case expectations. The eight ELF manifests
record entry, write_svc, newline_svc, exit_svc, fault_write, branch_payload,
loop_start and loop_end as applicable; independent disassembly checks each used
instruction and target. Required symbols cannot be substituted by kernel output.

Two passes execute this exact12-case order (24 processes total):

| Case | Fixture | Input | Expected termination |
| --- | --- | --- | --- |
| absent | absent | None | exit10 |
| empty | empty | Some(empty) | exit11 |
| one | one | byte41 hex | exit12 |
| bytes255 | bytes255 | bytes00..fe hex | exit12 |
| binary256 | binary256 | bytes00..ff hex | exit12 |
| after_binary_absent | absent | None | exit10 |
| payload_write | payload_write | byte57 hex | permission fault |
| after_write_absent | absent | None | exit10 |
| payload_execute | payload_execute | bytesc0 03 5f d6 hex | execution fault |
| after_execute_absent | absent | None | exit10 |
| input_loop | input_loop | byte4c hex | deadline |
| after_loop_absent | absent | None | exit10 |

Every exit probe first validates presence/length/exact bytes/full zero tail,
then emits payload (including the zero-length write), newline, and exit. These
are exactly three SVCs per successful probe. Exit10 proves absent,11 proves
present-empty,12 proves the specified nonempty payload; failed validation exits
with a distinct failure status that the host rejects. Expected output is the
exact input byte sequence followed by0a, with Trace hex encoding; no checksum
stands in for byte comparison. Absent/empty zero writes have CALL/RESULT but
no OUTPUT record. After-cases catch stale bytes following binary/fault/deadline.

The write probe validates its expected input, then clears NZCV and executes the
named `strb wzr,[x9]` with x9=STARTUP+88 (0x1_0010_0058). Require ESR0x9200004f,
FAR that address, ELR=fault_write, BTYPE0 and the established sanitized PSTATE.
The execute probe validates input, clears NZCV and uses named `br x9` to that
same aligned payload VA; BRX9 is independently checked as0xd61f0120. Require
ESR0x8200000f, FAR=ELR=STARTUP+88 and captured BTYPE1, using the already reviewed
pinned-QEMU BR/NX interpretation from0023: the source RX page is unguarded
(no GP bit), so BR X9 selects BTYPE01, not the guarded-source BTYPE11. Its
payload encodes a known RET instruction; this is not a standalone safe sentinel.
The exact instruction-permission fault gate rejects any execution of that byte
sequence, regardless of where an incorrect RET might subsequently branch. Loop is a named `b .`
with loop_end=loop_start+4 and ends through the existing deadline IRQ. These
three probes issue no SVC on their successful fault/deadline paths. Per pass,
nine exits give27 SVCs, two faults and one IRQ:30 process slices,60 across two
passes. Preserve exact root/control/GIC checks on every slice.

Freeze this serial envelope (decimal numbers, existing strict field parser):

```text
HMINPUT REJECT phase=before error=length initialized=0 generation=0 before=<n> after=<n>
HMINPUT PREFLIGHT checks=15 irqs=8
HMINPUT BEGIN 24
HMINPUT INFO wire=1 extension=1 max_input=256 cpus=1 timer_frequency=<n> timer_ticks=<n>
```

REJECTbefore constructs length257 input before any Executor exists; require the
Length error and unchanged Pool. After the existing8-IRQ timer preflight, each
process emits these exact ordered record forms:

```text
HMINPUT RUN index=<n> pass=<n> case=<name> fixture=<name> present=<0|1> length=<n> generation=<n> entry=<n> root=<n> before=<n> during=<n> owned=<n>
HMINPUT READBACK index=<n> base=1 extension=1 zero_tail=1 source_detached=1
... existing unmodified HMPROC CALL/OUTPUT/RESULT records for this index ...
HMINPUT STOP index=<n> reason=<exit|fault|deadline> code=<n> calls=<n> written=<n> esr=<n> far=<n> elr=<n> spsr=<n> irqs=<n>
HMINPUT RESTORE index=<n> root=<n> vbar=<n> controls=1 inactive=1 before=<n> after=<n>
HMINPUT PASS index=<n> pass=<n> case=<name>
```

Indexes are0..23; pass0/1; generations are actual Process token high32 and1..24.
RUN owned is independently derived ELF pages+29; during=before-owned. READBACK
is emitted only after chunked actual startup comparison and lexical Input/source
lifetime checks; EL0 probe validation is independently required too. Payload and
source mutation use known bounded arrays, not a retained reference to the same
Input as an alleged independent oracle. All process before/after values equal
the pre-run common baseline; no readback callback runs during execution.

After24 runs, emit:

```text
HMINPUT REJECT phase=after error=length initialized=1 generation=24 before=<n> after=<n>
HMINPUT PREPARE budget=1 exhausted=1 drop=1 generation=27 before=<n> after=<n>
HMINPUT END 24
```

The second257-byte rejection must leave resources/last prepared generation
unchanged. Then exercise budget0 and actual exhausted-pool preparation with a
valid nonempty Input, confirming returned Space unchanged/no startup leaf and
restored free count. These failed prepares burn generations25/26 as documented.
A final valid prepare/drop yields actual generation27 and full reclamation,
proving no extra generation was consumed by the rejected input. PREPARE's three
success flags are emitted only after all assertions. No EL0 entry occurs for
these failures or the final dropped Process. All records use the same baseline.
The profile then runs the existing strict baseline suite and its16 EL1 timer IRQs
before privileged test exit. Exact trace prefix8, process60 slices, suffix16
must be checked; no arbitrary EL1 exception allowance is introduced.

Host artifact gates bind all probe ELF/archive bytes to the built kernel as in
0025 and record actual source hashes. Mutation tests reject wrong presence,
length, output byte, generation, free count, case/order, exit proof, readback
flag, syndrome, target, branch type, extra/missing SVC/IRQ and missing rollback
records. These frozen cases supersede the earlier paragraph's placeholder for
later profile inventory/grammar. Implementation and compiled-stack approval
preceded execution; the linked evidence records the accepted binaries.

Final Stage B wire/artifact details: after decoder validation, every probe payload
WRITE uses the fixed address STARTUP+88, including zero-length writes (no pointer
identity dependence on an empty View slice). The separate newline_svc writes
one byte from the independently named immutable RO symbol `newline`, whose byte
is0a. Exit uses the distinct exit_svc symbol. Each of the eight probe manifest
records appears in an envelope containing exactly `version: 1` and `probes`
(the eight named records). Each record has exactly `file`, `path`, `sha256`, `entry`, `pages`, `symbols`,
`segments`, `input`; input has exactly `present` (boolean) and `hex` (canonical
lowercase payload hex). Exact symbol inventory is native_start, write_svc,
newline_svc, exit_svc, fault_write, branch_payload, loop_start, loop_end, newline.
All symbols may be retained by shared assembly, but every exercised symbol and
opcode/address is independently verified. Recovery cases reuse absent's manifest;
they do not create additional ELF variants.

The ordinary monitor integration sequence extends0026 after its warm-up test,
apps and baseline status with five echo launches: absent (`run bin/echo`),
present-empty (`run bin/echo `), text (`run bin/echo hello input`), preserved spaces
(`run bin/echo  hi `, payload ` hi `), and exactly51 `A` bytes after the13-byte
`run bin/echo ` prefix (the64-byte accepted command bound). Query status after
each. Then retain info, hello, fault, loop, interleaved test and hello with their
existing status observations. This gives10 launches/generations1..10,21 SVCs,
one fault and one deadline IRQ:23 process slices. Test invalid empty-path spacing
and65-byte overflow before first launch, with status confirming no initialization
or generation. Their rejected lines must recover the prompt. Former extra-text
usage rejections are explicitly replaced by this byte-input grammar, not silently
accepted under an unchanged test expectation.

Preserve the existing fresh128MiB legacy hello profile. Add a separate fresh128MiB
input-first profile: status, `run bin/echo first`, status, with no warm-up test.
Require `first` plus newline, exit0, generation1, unchanged initial free count,
three SVCs and exactly8 preflight IRQs. Main64/128MiB profiles retain warm-up16,
preflight8 and interleaved16 EL1 IRQs. Terminal transport remains LF-to-CRLF;
wire output byte counts are original payload length plus one.
