# ARM exception verification

Executed 2026-10-26 UTC on parent `f501bcf` plus this change. Command:
`make test-all`, using pinned Rust 1.99.0 and QEMU 7.2.22 with TCG and one CPU.
Full log: [exception-suite.log](exception-suite.log). Exact tested source hashes,
expanded commands, ELF hashes, serial output, compiler diagnostics and QEMU
exception traces are in [exception-final/](exception-final/).

Both architectures passed formatting, Clippy, structural safety checks,
all 13 host harness tests, all 22 guest cases, five capability compiler probes
and 22 frame/heap compiler probes. ARM 64 MiB and 128 MiB monitor profiles and
both architectures' CPU-rejection profiles passed.

The ARM fault contract performs four repetitions of six probes: null read,
normal-stack guard read, emergency-stack guard read, read-only write,
non-executable instruction fetch and full context preservation. Each successful
probe returns through an exact registered fixup; an unexpectedly permitted
access reports failure. The context fixture checks all 31 GPRs and 32 Q
registers, NZCV, FPCR, FPSR and normal/emergency SP values. The assembly handler
uses a 256-byte emergency frame and no FP/SIMD instructions or Rust calls.

The raw boot trace records current-EL data abort syndromes `0x96000006`
(null translation), `0x96000007` (guard translation), `0x9600004f` (RO write),
and instruction abort `0x8600000f` (NX), followed by exception returns to
the registered labels. Address values and image boundaries are preserved in
the trace and ELF identity, rather than assumed stable between builds.

Two separate negative-profile images passed strict checks:

- An unregistered BRK after setting normal SP inside its guard still reaches
  the emergency handler, prints exactly one fatal marker and exits with status 2.
- With emergency SP set to its guard plus 256, the first saved-register store
  faults. QEMU records BRK followed by a Data Abort (`0x96000047`, FAR in the
  emergency guard) and entry at vector offset 0. The guest emits exactly one
  NESTED marker and one FATAL marker, then exits 2. This distinguishes actual
  nested routing from outer BRK rejection.

Both negative tests require no HMTEST records and completion within 20 seconds;
launch errors, missing/duplicate markers, wrong exits and timeouts fail. Host
unit tests exercise rejection of those false-success conditions. All ARM runs
retain QEMU `-d int` traces. There were no failed build or guest attempts in
this increment.

The x86 fault-contract case explicitly requires the API to be unavailable; no
x86 fault recovery is claimed. ARM recovery remains limited to these fixed
EL1 sites: there is no arbitrary-address user copy, demand paging, EL0 fault
handling, IRQ delivery, preemption or SMP. Other exception vectors are fatal.
Independent AI design/source review is recorded separately, not a human audit
or full soundness proof. Remote CI has not executed this change.
