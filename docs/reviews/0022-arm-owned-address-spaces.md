# Owned ARM address-space review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit. This reviewer does not build or execute guests.

## Scope

Reviewed the Stage B proposal in
[design 0022](../design/0022-arm-owned-address-spaces.md) against boot assembly,
the actual Pool/Frame implementation, Mapper, CPU0 authority, BUSY ownership,
secondary lifecycle and current exception entry at parent `85985f4`.
Source review of the new implementation and execution evidence remain pending.
EL0 entry, instruction-cache preparation, native ABI, fault containment and
initramfs remain later gates under design 0021.

## Ownership and authority

The private Factory constructor needs the actual admitted boot profile; an
arbitrary Pool borrow or caller-provided topology is insufficient authority.
Space retains all data and table owners, has no byte-reference or Frame escape,
and cannot move between CPUs. Its lifetime depends on the Pool, not the ELF
input after copying. Partial construction must record each allocation before
the next fallible step; ordinary owner destruction then releases only that
construction's prefix. Forgetting an inactive object leaks all its owners and
cannot leave hardware using freed pages.

Twelve table pages, sixteen stack pages and at most 256 image pages give the
284-frame bound. The startup address is only a reservation in this stage; it
must remain absent. Frame-count bounds do not prove compiled stack usage.
Constructor temporaries and return moves need inspection before guest tests.

Copied identity tables preserve kernel code, data, emergency/coordinator stacks,
guards and device mappings. Executable user backing has an EL1 RO/NX identity
alias in the private root. The original writable boot alias is inactive on
CPU0, and actual CPU1 OFF plus BUSY excludes another active boot alias. This
requires all backing owners and mutable byte borrows to remain inaccessible
until the original root and invalidation have been restored.

## Installed diagnostic

There must be no safe active guard that a caller can forget, nor any arbitrary
callback under the private root. Existing callers can retain references into
the CPU0 owned arena, which is absent there. Only the private assembly operation
may run while installed, using trusted local copies of query/result storage.
The owner and every table frame remain live across both root transitions.

CPU identity, masked interrupts, exact boot controls and firmware peer-off
admission precede installation. The same BUSY lease excludes existing CPU_ON
and thread runners. Unexpected exceptions are terminal; they cannot enter
ordinary cleanup with an uncertain active root. Every normal return restores
the original TTBR and PAR, completes local translation invalidation, and only
then permits Rust execution and owner destruction.

ASID zero and global mappings require full invalidation: changing TTBR alone
does not remove cached permissions. Common mappings preserve physical outputs,
granules and memory attributes. No changed or new user address is dereferenced
between root installation and completed maintenance. Active tables themselves
are never edited. AF is already set and hardware dirty/access updates are
disabled by the admitted control profile.

AT observations establish data translation and read/write permissions, not
instruction execution, ordinary-load PAN enforcement, or fault containment.
NX and physical W^X have descriptor evidence at this stage. Those distinctions
must survive test naming and progress reports.

## Architectural checks

The AP alternatives and separate UXN/PXN semantics agree with the official
[Arm memory-model guide 102376, section 10](https://documentation-service.arm.com/static/6298a839b334256d9ea8af70?token=).
Its sections 11 and 13 cover access/dirty updates and alias attributes. The
specified permission words also agree with the existing boot leaf encoding.
The official [Arm memory-management guide 101811, version 1.4, section 8](https://documentation-service.arm.com/static/68e680a2587fd957f5efd742)
requires translation invalidation and completion barriers when mappings change.
The design's shared physical outputs and immutable table switching are a local
reasoning argument, not a quotation of a ready-made context-switch algorithm.
No Linux implementation source was used.

## Verdict

**Approved for Stage B implementation only**, after review of the final
16-query copied-buffer contract, scoped Pool permission exception, disabled
hardware table updates, and pre/post-switch full-invalidation protocol. The
descriptor bits match the designer's checked official section 3 diagram and
the existing boot encoding; permission meanings were independently checked
against the official section 10 table.

This is preimplementation design approval. New Rust/assembly source, compiled
stack use, actual-type compiler checks, and guest execution remain unverified.
Implementation review must verify compiler memory effects on the assembly call,
register preservation, local buffer placement, exact restoration before Rust
resumes, and all allocation/error cleanup paths. It must also confirm that
the actual factory constructor establishes the stronger topology/root proof.

Required guest evidence includes the real linked ELF, unaligned page-crossing
copies and zero-fill, every descriptor/backing owner, read/write AT results,
all failure prefixes plus real Pool exhaustion, exact reclamation, maximum
capacity, bounded forgotten ownership and repeated root restoration followed
by the existing boot mapping checks. A successful boot or subset does not
complete those gates or establish protected EL0 execution.

## Source and harness review

Reviewed `user_space.rs`, `user-space-aarch64.S`, `test_space.py`, its mutation
tests and `check_space_types.py`. **No blocking finding remains in those files**
after the corrections below. BootPlatform integration, compiled stack use and
execution remain the integrating agent's pending gates.

The loader records each allocated owner directly in its bounded arrays, copies
file intervals through checked admitted pages, and preserves zeroed padding,
BSS and stack. Failed construction drops its own retained prefix. The complete
table audit checks exact words against retained ownership and immutable boot
leaves; no caller supplies a physical mapping address. Table storage remains
privileged and unexposed. The read inspector validates the entire mapped range
before copying and returns no backing references.

Probe copies at most sixteen queries and retains the address space throughout
the assembly call. Review identified that a Rust local is not by itself proof
of coordinator-stack placement: optimization could place results in a caller
return slot in the omitted owned arena. The corrected implementation checks
the actual complete input/result/root-buffer ranges against matching boot and
private EL1 RW identity leaves before entering assembly. This makes visibility
independent of stack-placement optimization.

The assembly performs boot/private/restored-boot AT passes with the same bounded
queries, stores each triple in its fixed result record, and saves the original
PAR in a caller-saved register untouched by the probe loop. It preserves SP,
callee-saved registers and LR. Both root transitions complete the reviewed
pre/post invalidation sequence; the last pass restores original PAR before
return. The ordinary external-call boundary carries compiler memory effects.
Rust compares all observed control registers and the restored root before
returning or releasing BUSY; disagreement is terminal without owner cleanup.
No Rust byte references to backing storage are live during the assembly call.

The host harness requires exact mapping/query inventories, two repeated passes,
root/control restoration, frame-accounting cases, ordered post-probe baseline,
bounded complete artifacts and admitted exception traces. Review tightened
fault decoding to reject stage-2 S/PTW bits rather than accepting a matching
fault-status code alone; mutation tests cover this. Successful translations are
compared with retained page bases plus query offsets. The compile clients import
the actual implementation modules and reject constructor/owner access, lifetime
escape, cloning and Send/Sync while admitting release of copied ELF input.
Compiler execution and the reported Python-test pass are not executions by
this reviewer.

The integrating BootPlatform callsite was subsequently inspected. It passes
the private actual Pool and topology produced by `initialize`, whose full
mapping checks and reservation exclusions precede construction. The dedicated
boot dispatch reaches this call before starting peers or worker scopes, and
Factory still checks exact control context and admitted topology. Existing
fatal-profile and SMP/secondary feature precedence remains intact; the space
profile runs the normal boot suite only after its own checks. The Pool contract
amendment records immutable original tables and the scoped private-root
permission exception. No blocking integration finding remains in these paths.

Reviewed the safe `space_checks.rs` guest client and its correspondence with
the strict host protocol. It loads the actual embedded ELF, checks every file
byte and zero-fill/padding, retains a live owned-arena borrow through 142 AT
queries, exercises all 33 fixture allocation prefixes, exhausts the actual
Pool to eight frames with a retained Block, and tests maximum-image and
unaligned cross-page cases before a bounded deliberate leak. The host consumes
the same 71-query inventory twice. Review corrected a range-validation test
that initially started inside the guard: it now begins at the last valid
stack byte and crosses into an absent page, requiring unchanged output even
for the initially valid byte. No blocking guest-test source finding remains.

## Compiled stack finding

The first emitted release assembly revealed a **blocking aggregate stack
overflow before guest execution**. Prologues in
`hematite-9cf396a302e942c4.s` charged `boot_main` 7,440 bytes, `run` 16,
`checks` 28,208, `geometry` 36,896 and `Factory::load` 17,264. The nested
geometry/load path therefore required at least **89,824 bytes**, exceeding
the 65,536-byte coordinator stack before loader callees. Geometry entry alone
already exceeded the bound. Large structures in individually bounded functions
were not sufficient evidence of aggregate safety.

The integrating agent and implementation author were notified to avoid booting
this candidate and split test phases or reduce ownership storage, followed by
a fresh compiled aggregate audit. This finding supersedes source-only approval
for executing the first candidate; resolution and rerun remain pending.

The third compiled candidate resolves this finding by separating test phases
and geometry input buffers from owner-bearing helpers. Independent prologue,
call-path and linked-builtin inspection finds a 54,720-byte largest new path,
below 65,536; the existing post-probe baseline's inspected deepest chain is
61,712 bytes. The [stack review](../evidence/space-stack-review.md) records
the exact assembly hash, frame sizes, scope and remaining margins. **The
third-build stack gate passes**; later machine-code changes require rechecking.

## Observed execution evidence

Independently inspected the parent's
[three-profile result](../evidence/space-first-qemu/result.json) and raw serial,
exception trace, command, input hash and result records for the 64 MiB/one-CPU
and 128 MiB/two-CPU runs. This is evidence inspection, not a separate guest run
by the reviewer. Both inputs identify ELF SHA-256
`cc6357bf6343bc1f7fd871d716d6673b37de4eab1c77ebe97f19e3b321881eca`, matching
the stack-reviewed build. They exited zero within approximately 1.2 seconds.

Each inspected serial log contains 21 distinct user backing pages, 142 probe
triples, ten root observations, all five space cases and then all 41 baseline
passes. Independent raw-value comparisons confirm exact before/after PAR
equality, one unchanged private root, restored boot root/control reports, and
the expected user read/write translations. Resource records show baseline
3,306 free frames, fixture allocation 33 with full drop reclamation, all 33
budget failures, actual pressure to eight frames, maximum allocation 284, and
the deliberate retained 33-frame leak. Guest source additionally checks that
a subsequent allocation does not reuse observed leaked backing addresses.

The two-CPU trace contains twenty CPU0 PSCI HVC events, matching FEATURES and
AFFINITY_INFO checks before ten probe batches; no CPU1 event occurs. CPU1 OFF
is supported by the reviewed query-result checks and successful guest report,
with the trace corroborating PSCI calls rather than independently recording
the returned firmware status. Neither inspected trace mentions EL0. Source and
trace evidence support **owned ELF loading and installed AT data translations**,
not EL0 instruction execution or execute-permission fault tests. Both traces
end in the single admitted semihosting exit after baseline exceptions/IRQs.

The aggregate records all three requested profiles passing. **The inspected
local Stage B execution evidence supports its stated scope.** Full repository
regressions and remote CI remain separate gates; protected native userspace
execution and the complete Phase 2 exit gate remain open.
