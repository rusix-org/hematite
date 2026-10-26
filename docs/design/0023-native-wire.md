# Experimental native process wire interface

Status: safe policy implemented and independently reviewed, 2026-10-26.
See [review 0023](../reviews/0023-arm-native-process.md) and
[verification](../evidence/native-policy.md). Core EL0 integration remains separate.
This is a local versioned experiment under ADR 0005, not a stable public ABI.
The hardware transition and ownership proof live in
[design 0023](0023-arm-native-process.md).

## Calls and startup

AArch64 `svc #0` uses x0 = version 1 and x8 = operation. Console write (1)
uses x1 = granted handle, x2 = user byte address, x3 = byte length. Exit (2)
uses x1 = status 0..255 and requires x2 = x3 = 0. Other registers are not
arguments. A returning call sets x0 to the status below and x1 to bytes written
(on successful write), or zero otherwise. Preserve x2..x30, SIMD, FP control
and permitted NZCV state. Entry starts with x0 = startup address and other GPR,
SIMD, FP and user TLS state zero; the process design specifies sanitized PSTATE.
An unsupported SVC immediate is a process fault, not another syscall namespace.

| Status | Value | Meaning |
| --- | ---: | --- |
| OK | 0 | Whole requested write completed |
| Version | 1 | Unsupported x0 version |
| Operation | 2 | Unsupported x8 operation |
| Handle | 3 | No matching writable console grant in this process |
| Address | 4 | Nonempty source is not entirely readable user storage |
| Length | 5 | More than 256 requested bytes |
| Budget | 6 | Successful bytes would exceed the 4096-byte session limit |
| Argument | 7 | Exit status or reserved arguments are invalid |

Validation precedence is version, operation, then operation-specific fields.
Write validates handle, length, cumulative byte budget, then address. Exit
validates status and reserved arguments. Invalid requests produce no console
output and consume no byte budget. A valid zero-length write still requires a
valid grant; it succeeds without consulting the supplied address or host output.
Ranges use checked arithmetic; copied reads are all-or-nothing. No raw user
pointer is dereferenced in syscall policy.

The startup page is zero-initialized and mapped user-readable, nonwritable and
nonexecutable at `0x1_0010_0000`. Its first 64 bytes have these explicit
little-endian offsets; the remainder is zero:

| Offset | Bytes | Value |
| --- | ---: | --- |
| 0 | 8 | ASCII `HEMATITE` |
| 8 | 4 | Version 1 |
| 12 | 4 | Header length 64 |
| 16 | 8 | Features: bit 0 means a console-write grant; other bits zero |
| 24 | 8 | Granted console handle, or zero when absent |
| 32 | 8 | Maximum write length 256 |
| 40 | 8 | Maximum successful console bytes 4096 |
| 48 | 8 | Maximum SVC attempts 64 |
| 56 | 8 | Reserved zero |

This page is built with byte encoders, never Rust repr/default-layout casts.
It grants no allocator, device pointer, ambient filesystem or Linux ABI.

## Grant and resource ownership

A private core-owned issuer persists across process runs. Safe policy gives it
a checked monotonically increasing u32 generation, starting at one, and retires
it instead of wrapping. A session's one optional console table entry has slot
one and WRITE rights. The wire token is `(generation << 32) | 1`; zero and any
other slot/generation are invalid. Issuance consumes a generation even when no
console is granted. The entry belongs to that Session, is never transferable,
and disappears when it stops or drops. The core explicitly chooses whether to
grant its serial console; no input from the program can create a grant.

Integers are identifiers scoped to a process table, not secret unguessable
capabilities. Guessing a different value cannot grant authority absent from that
table. Generation separation rejects stale prior-run tokens when the actual
executor retains its issuer. An independently created safe policy issuer has a
separate model namespace and confers no hardware authority. The experimental
capability model remains metadata-only; this small synchronous console table
must not be described as integration of its asynchronous lifetime model.

Each session accepts at most 64 SVC attempts, including malformed requests.
Before any further resume the runner checks both this limit and the absolute
kernel deadline. The 64th call can exit normally; a returning 64th call yields a
call-limit termination without executing a 65th SVC. User faults, deadline,
call-limit and exit are process outcomes, not kernel panics. The kernel deadline
is independent of call count and is not extended after a returning call.

## Safe policy API and core adapter

A dependency-free `hm_kernel::native` module defines Request (five u64 values),
Response (two u64 values), Session, Issuer, Status and Outcome. These are internal
safe Rust types, not shared-memory or wire-layout structs. Issuer/Session fields
are private and neither object is Clone. `issue(console: bool)` returns a fresh
Session or a generation-exhaustion error. `startup()` returns exactly 64 bytes.

`Session::dispatch(request, host)` validates before effects. The host trait has
`copy_from_user(address, output: &mut [u8]) -> Result<(), CopyError>` and
`write_console(bytes: &[u8])`. Policy owns a fixed 256-byte kernel buffer and
calls the output method only after a complete successful copy. No borrowed
user reference escapes. Only successful writes charge their full byte count;
errors leave it unchanged. The concrete core host uses stopped process-owned
backing frames under the restored boot root, while CPU1 is OFF and no DMA or
user task can modify them. PAN stays enabled. Hardware console output is a
bounded synchronous operation; a returning `write_console` guarantees every
byte was emitted. The concrete UART adapter bounds readiness polling; hardware
failure is terminal platform failure and never resumes the program with OK.
Such a failure may have emitted a prefix and is not a recoverable user ABI
outcome. This first interface has no queued write contract.

Outcome is Reply(Response) or Stop(Exit(u8)/CallLimit). A Reply describes the
call result for reporting; it never grants hardware resume authority. On the
64th returning call, dispatch immediately stores CallLimit and clears the grant,
yet returns that call's Reply. `termination()` exposes the terminal reason.
The runner checks termination and deadline before every ERET, including after a
Reply; the 64th result is recorded but never returned to user execution. `may_resume()` is false
once the accepted-call count reaches 64 or a valid exit/terminal outcome occurs.
Dispatch after a stopped session performs no host operation and returns the
stored terminal result; a session at its call limit transitions to CallLimit.
Counters use checked arithmetic even though the declared limits are small.

## Required verification

Safe policy tests cover exact startup bytes, absent/foreign/stale grants,
generation retirement, unsupported version/opcode rejection and validation
precedence, valid/invalid exit, copy failure with no output, zero/maximum/
over-limit writes, overflowing addresses rejected by the adapter, call limits
including invalid-call storms and the 64th valid exit, byte accounting, and
no effects after termination. Actual-module compiler probes reject field
forging, cloning and direct generation/counter manipulation.

The actual EL0 suite must independently exercise these rules through SVC,
using a real separately linked Rust program. It must prove bad addresses are
rejected by the real adapter, rather than only a mock host. Hardware execution,
fault containment, timer preemption and frame reclamation remain separate
mandatory gates in the process and verification designs.
