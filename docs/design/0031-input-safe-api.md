# Interactive input: first safe metadata APIs

Status: exact safe API approved in
[the contract review](../reviews/0031-interactive-input-contract.md);
implementation and verification in progress.
This is the safe-codec/policy portion of0031, not permission for core copying,
UART, WFI, coordinator or actual shell implementation.

## Wire module

Add forbid-unsafe `interactive_wire`, with READ6, INPUT_SLOT5, MAX_READ256,
CALL_LIMIT4096, BYTE_LIMIT65536 and GRANT_SIZE64 constants. Immutable Copy Grant
has private input_token:u64, `new(token)->Result<Grant,Error>`, `input_token(self)
->u64`, `encode_into(self,&mut[u8])->Result<(),Error>` and
`decode(&[u8])->Result<Grant,Error>`. This is non-authoritative data.
Token shape requires low32 exactly5 and nonzero high32 generation. Encoding
rejects wrong destination length before modifying it; otherwise overwrites all64
bytes. Decode precedence is Length, Magic, Version, Size, Flags, Handle, Limits,
Reserved. Magic is exactly8 bytes HMINPUT\0, identical bytes to the existing outer
startup-input magic, with a different nested layout/location and size field.
Fields/offsets are exactly those in0031-interactive-input-contract. Error variants
are Length/Magic/Version/Size/Flags/Handle/Limits/Reserved. Unknown limits/flags
or nonzero reserved values fail; no alternative encodings are canonical.

## Fixed Session profiles and entrypoints

Add private SessionProfile::{Legacy,Interactive}, carried in the original
Session. Existing Issuer::issue(console) creates Legacy unchanged. New public
safe metadata `Issuer::issue_interactive()->Result<Session,GenerationExhausted>`
uses the same nonwrapping issuer, always creates console-enabled Interactive,
and accepts no numeric limits. Core authority remains exclusively private.

All Session operations use their selected profile's limits. Legacy64/4096 bytes,
startup flags and existing behavior remain exact. Interactive4096/65536, maxWRITE
256, startup flags3 while live (console1/input2). After terminal stop, clear both
advertised grants: flags0 and console0. Profile and reported numeric limits
remain Interactive, not reset credit. Immutable Process startup bytes are never
mistaken for the live core authority table.

Preserve existing Admission enum. New separate:
`InputAdmission::{Complete{session:Session,outcome:Outcome},Read(ReadAdmission)}`.
`Session::admit_input(self,Request,&mut impl Host)->InputAdmission` counts once
using the same Session, recognizing READ only for Interactive. Other operations
evaluate only WRITE/EXIT; task opcodes always Operation in this entrypoint.
`dispatch` on Interactive evaluates WRITE/EXIT and rejects READ/task opcodes;
`admit_task` on Interactive also rejects task opcodes rather than minting a task
guard. Their budgets still follow the original Interactive profile. On Legacy,
new admit_input has exactly legacy dispatch behavior, including rejection of6.
Existing Legacy admit_task remains unchanged.

For recognized version1 READ at call4096, terminate CallLimit before making any
guard or effects. Wrong version/opcode at final call returns the existing error
reply then revokes/stops; WRITE/EXIT final-call behavior remains existing policy.
Already stopped sessions remain sticky and do not increment again.

## Owned READ guard

ReadAdmission is non-Clone/non-Copy with private original Session and copied
Request. Public read-only `request()->Request`, `calls()->usize`,
`written()->usize`; no Session reference or caller borrow is retained.
`reject(self,ReadError)->(Session,Outcome)` accepts only typed
ReadError::{Handle,Length,Address}, returning that status/count0, same counters.
Core must validate in that order; metadata cannot validate real authority/range.
There is no generic success reply method.

`park(self)->Result<ParkedRead,Self>` additionally checks wire capacity1..256,
returning the unchanged guard on failure. It does not check/claim core authority
or memory accessibility. ParkedRead privately owns original Session+Request,
exposes copied request/calls/written getters, and has
`finish(self,count:usize)->Result<(Session,Outcome),Self>`. Require count1..256,
count<=original capacity, original capacity1..256, and Session still resumable;
otherwise return the exact ParkedRead unchanged. Success yields Ok/count with
no extra call or output-byte charge and the original remaining allowances.
Dropping either owner abandons that Session; no callback, buffer, wake authority,
user borrow or restart/reset method exists. Only the later sealed charged kernel
Delivery may call finish after actual range validation and copy/commit checks.

## Required host and compiler evidence

Separate integration tests, with exact inventory: canonical/roundtrip envelope,
every field defect and size/no-write-on-error, token limits/kind, nested outer
codec, arbitrary/truncated bytes; exact old policy parity; profile startup/stop
flags/limits; READ admission counter and no Host effects; shared READ/WRITE
counters/bytes; guard reject/invalid park return; multiple park/finish cycles;
invalid finish returns original owner; final READ and malformed last calls;
stopped stickiness; guard drop/session locality; no task guard from Interactive
through any entrypoint. Compiler clients reject cloning/copying/forging guards,
reuse after park/finish, retained/escaped borrow and forged profile fields. Both
architecture static gates and existing strict native/task-admission tests remain
required. No host test establishes real user copy, RX access or hardware wake.

Before any new guest, batch safe changes and require actual type/layout and
all affected linked stack/order gates. Hardware remains independently gated.
