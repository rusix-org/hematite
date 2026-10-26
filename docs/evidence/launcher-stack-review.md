# Native launcher compiled stack review

2026-10-26. Independent AI inspection, no builds or guests by this reviewer.

## First candidate: rejected before execution

Inputs and hashes are preserved in
[launcher-first-candidates](launcher-first-candidates/inputs.json), with emitted
assembly gzip archives and symbol logs. ARM normal ELF
`fa045390bbb16cd5bc8ba582125fadfedd5f3017461f9fc261ac0067237e5d3d`
and assembly
`b58f52ed10e7cbcaba58ef1533be68ecf5c79456e7c6a11db4f4e32fecb5b90a`
fail the64KiB coordinator stack gate.

Read real prologues, page-probe loops and BL call edges. These frames remain
simultaneously live; none of the calls is an epilogue tail transfer:

| Frame | Bytes |
| --- | ---: |
| boot_main |2512|
| run_monitor |448|
| native_launch::run_command |80|
| launcher::State::run |10256|
| launcher::load_and_run |17968|
| launcher::run_loaded |29632|
| Executor::run |9872|
| Executor::execute |960|

The sum is71,728 before leaf helpers, already6,192 bytes beyond65,536.
State::run contains inlined archive/ELF planning temporaries; run_loaded also
retains large process/result return slots. Source-level scope separation did
not prevent these compiled frames. Parent and designer were notified before
any normal guest. A refactored candidate requires a fresh emitted-assembly
review; this failure must not be replaced by successful later evidence.

The first process candidate has boot_main7536 (96 bytes above the previous
archive candidate), with the established baseline remainder unchanged; its
baseline sum is61,808. This is a scoped observation, not approval of other
feature profiles. All affected ARM profile callers and the ordinary x86 monitor
baseline still require final artifact-specific checks.

## Second candidates: compiled gate passes

All21 compiled profiles are recorded in
[second-candidate inputs](launcher-second-candidates/inputs.json). Independently
matched every ELF hash and decompressed assembly hash against that inventory.
Normal ARM ELF is
`0ad998d20161cf406323da919304579ed2340f7fc10c5ebcb14895c8f43a82b3`;
assembly is
`53803731a61a23f437bb70ade08f1854d7e5fb1532c8b5d948bab5a33523e0de`.
The process ELF remains`e93bc4336086267b592f032291ed0ce33fee5ab8a1983dc3940e89fa5b6caa34`;
x86 monitor remains`4f86e773008b3ce448a692ec90f46c31e9ade9599d3e89609b9d6fc8af734ea1`.

The separate noinline admission helper removes archive temporaries from the
live execution chain: State::run falls from10,256 to1,472 bytes. The helper
itself uses9,328 and calls Archive::parse before returning. The error-borrow
refactor preserves cleanup but does not reduce run_loaded's29,632-byte frame.
All other normal execution frames in the first table are unchanged. The new
sum through Executor::execute is62,944. A conservative640-byte allowance covers
the inspected fixed formatting/console/terminal helper chains, giving
**63,584 bytes**, leaving1,952 below65,536. Directly inspected Console96,
fmt::write112, scalar formatting48, integral padding96, prefix48 and terminal16
frames fit this allowance. Terminal-only output takes less than the conservative
trace formatter path. Timer preflight and admission occur before load/execute;
their separate call paths fit below this maximum. Factory load, prepare failure,
ordinary cleanup and retained monitor frames were included.

Inspected emitted prologues and direct call chains across all19 ARM and2 x86
profiles. The following are coordinator-path maxima before a conservative
additional640-byte allowance for fixed leaf/indirect helper paths (normal launch
uses its separately computed bound above):

| Profiles | Bytes |
| --- | ---: |
| ARM monitor bootstrap test |57232|
| ARM boot-test; preemption, boundary, no-timer |56784|
| ARM space/process baseline |61808|
| ARM secondary, stale-generation, CPU-off-return |61968|
| ARM SMP baseline |62704|
| ARM unhandled/nested/thread-overflow/preemption-overflow/wait-blocked coordinator |12496|
| ARM SMP no-peer-timer/peer-overflow/all-blocked coordinator |13536/13472/13664|
| x86 monitor/bootstrap |48928/48592|

The largest other conservative bound is SMP63,344, also below64KiB. The table
includes terminating paths' panic helpers. Deliberate worker/peer-stack overflow
tests are not claimed to avoid their requested overflow: these numbers cover
their coordinator and retained boot frames. Existing worker/interrupt stack
mechanisms remain separate and unchanged. Calls across assembly stack switches
were not incorrectly added as simultaneous coordinator frames.

Symbols confirm ARM normal guard0x40310000/top0x40321000 and x86
bottom0x131000/top0x141000, each providing64KiB usable stack. Process entry uses
its separately retained exception frame; normal ARM exception top is0x40326000.
The four normal app disassemblies contain at most1,200 bytes total downward
stack adjustment in the largest ELF (info), summing even mutually exclusive
function paths; the other three contain32. Their64KiB user stacks have ample
room. This is static inspection, not a stack watermark measurement.

**Second-candidate source/compiled-stack gate passes for the exact inventory.**
Artifact validation and first actual interactive trace inspection remain
separate gates. No stack size or acceptance threshold was increased.

## Final-source reproduction candidate

The first full suite caught a stale normal-app fixture after source formatting;
failure remains in`launcher-first-full-suite.log` and its raw evidence directory.
Independently compared old/new`info.elf`: equal686,648-byte lengths, exactly one
different byte at file offset12664,81→108. Executable LOAD bytes and RW LOAD
bytes are identical; the change lies in read-only panic location metadata for
the moved source line. It changes neither normal application instructions nor
their stack bounds. The other three app inputs remain unchanged.

Independently compared all21 [third candidates](launcher-third-candidates/inputs.json)
with the reviewed second candidates. Every emitted function body is identical
in every profile; all current ELF/decompressed assembly hashes match the third
inventory. Twenty kernel ELFs are byte-identical. Only the normal ARM kernel
changes because it embeds the corrected archive. Its final ELF SHA-256 is
`4b8d02898264846846e19772bd0ec7a3159e468d5913f31e448822f3b99df04c`,
assembly SHA-256
`7cae524fbe5e08e0c4ba257bcfef3d6dec53514bab78f41f150490cabee9f80c`.
**The same reviewed stack bounds pass for this exact final-source inventory.**
Final reproducibility/artifact checks and execution evidence must bind these
new hashes; the earlier successful runtime logs remain scoped to their inputs.
