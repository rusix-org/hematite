# Bounded writes into inactive owned user memory

The two private Space operations validate and copy1..256 bytes into retained RW
backings. They validate the complete range, exact owner/table descriptors and
boot identity aliases before resolving both mutable slices and writing. No raw
user VA is dereferenced; no Frame/reference escapes or page table changes occur.
All returned errors leave bytes unchanged. This is an inactive-memory primitive,
not an enabled READ syscall or an interactive userspace program.

[Design0031](../design/0031-owned-rw-copy.md) and
[the independent review](../reviews/0031-owned-rw-copy.md) cover the ownership,
error atomicity, actual fixture, compiled ordering and raw tests. The canonical
five-LOAD/six-page ELF has SHA256
3af1b54406bcdd052c5c00669e029ff9877982db7272a05f8f892edfd888f9fa.
Its real startup RO page produces35 retained frames per case. Tests cover exact
length/page boundaries, writable crossing, genuine RW prefixes ending in RO,
RX or an unmapped page, window/overflow/guard rejection, full protected-page
readback and exact unchanged-prefix/sentinel checks. Descriptors and frame/free
counts remain unchanged; every case drops all owners before publishing PASS.

Both [fresh strict profiles](rw-copy/guest-third-strict/result.json) pass at
64/128 MiB, each with24 cases,48 actual AT observations and all41 bootstrap
checks. Exact PAR fault codes5/7/15, actual installed/restored roots and retained
physical pages are checked. Raw traces contain16 baseline IRQs and24 registered
baseline fault/recovery pairs, with no EL0 execution or HVC. They verify backing
copy/readback and translation permissions; eventual EL0 consumption remains an
interactive-integration requirement.

The [nine actual compiler clients](rw-copy/types-first/result.json) and private
in-crate adapter pass. [Five synthetic/mutation host tests](rw-copy/host-first/result.json)
and both-architecture [static checks](rw-copy/static-second/result.json) pass.
All35 kernels compile. The previous34 images remain byte-identical; the new
profile's maximum is62,480/65,536 bytes including the unchanged640 allowance.
Its RW admission chain is35,488 including that allowance. Emitted checks and
source/destination slice resolution precede every store; no fallible branch or
IRQ-state change follows the first copy.

Preserved failures: the first static attempt found two collapsible driver
conditions, corrected without suppressions. The
[first launch attempt](rw-copy/guest-first/result.json) failed before any VM:
the host binder passed a Path where the shared ELF helper requires bytes.
Correcting the adapter and source-bound manifest leaves fixture/kernel bytes
unchanged. Both [diagnostic guests](rw-copy/guest-second/result.json) then
execute successfully but intentionally fail aggregate acceptance until raw review;
the strict fresh run follows independent approval.

The [full ARM/x86 run](rw-copy-full-first/result.json) passes in364.4 seconds
with every captured input unchanged. All
[35 executed kernels](rw-copy-full-first/kernel-comparison.json) match reviewed
candidates. Raw archives retain121 ARM/18 x86 result records with no failed
verdict. All 351 captured inputs match committed source `8f50dde0` exactly;
see [committed-blob verification](rw-copy-full-first/verified-commit.json).
Independent final audit replays both copy profiles. Matching remote CI
for this checkpoint is pending. Hardware input, terminal ownership, idle waiting
and a real userspace input loop are still unimplemented.
