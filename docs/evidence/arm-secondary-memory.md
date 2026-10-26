# ARM secondary memory verification

Executed 2026-10-26 UTC on parent `75d0a6a` plus this change. Final command
`make test-all` exited 0. [Full output](secondary-memory-suite-second.log),
[source hashes](secondary-memory-final/source-sha256.txt), compiler results,
commands, ELF identities and raw traces are in
[final evidence](secondary-memory-final/). Existing pinned container/toolchain
and resource bounds apply; no new dependencies were introduced.

Both targets passed formatting, Clippy, safety/repository checks, 41 host harness
tests, 39 baseline guest cases and all existing compiler/monitor profiles. ARM
passed five scheduling/wait cases at 128 and 64 MiB with same-guest baseline
regressions, forced-boundary checks, withheld timer and overflow negatives,
all-blocked retention, exact timer traces and full register sentinels.

The new secondary_memory case checks six secondary stack reservations: 64 KiB
coordinator, 16 KiB emergency and four 64 KiB workers, each with a 4 KiB invalid
guard page. It validates exact sizes, alignment, adjacency, kernel containment,
every guard descriptor and every usable page's full RW/NX identity descriptor.
It checks all 512 entries of the distinct secondary root, requiring shared
device/identity subtrees and zero elsewhere, including the owned arena slot.
Full boot mapping checks incorporate the new guards before creating the pool.

The live pool's immutable eligibility Plan must exclude each complete reservation
and the secondary root page. The exclusion query also rejects empty, unaligned,
outside-window and wrapped-endpoint inputs; an actually eligible page must not
be reported excluded. No allocation or bitmap mutation is needed. The case
runs after actual CPU0 arena mapping tests. x86 explicitly treats the ARM-only
hook as inapplicable. These tests verify constructed mappings and reserved
storage, not CPU1 hardware translation, stack use or guard fault execution.

[Independent AI review](../reviews/0017-arm-secondary-memory.md) covered design
before implementation and source; it is not human review or a complete audit.
CPU1 never ran or installed the new root. Shared identity RAM remains visible
to privileged code, so Pool/Mapper authority must remain CPU0-only. Secondary
entry, mailbox ownership, confirmed shutdown and concurrent execution remain
open requirements.

The [first suite](secondary-memory-suite-first.log) stopped at Clippy's
identity-operation lint in a malformed-range fixture before guest execution.
Replacing `u64::MAX & !4095` with equivalent `!4095u64` resolved it; the complete
second suite passed. Public publication remains pending approval; no push retry
occurred.
