# ARM owned mapping verification

Executed 2026-10-26 UTC on parent `4a8f704` plus this change. `make test-all`
passed using the pinned Rust 1.99.0 / QEMU 7.2.22 container, one CPU and TCG.
See [full output](owned-mappings-suite.log) and
[archived evidence](owned-mappings-final/): expanded commands, environment,
ELF identities, serial logs, exception traces and compiler diagnostics.

Both architecture suites passed formatting, Clippy, safety hygiene, 13 host
harness tests, 25 guest cases, five capability compiler probes and 22 frame/heap
compiler probes. ARM 64/128 MiB monitor profiles, CPU-feature rejection, and
unhandled/nested fault profiles passed; x86 CPU rejection and monitor passed.
The new x86 mapping cases establish explicit absence, not mapping support.

The ARM mapping contract exercises real bytes through the new virtual arena,
retains ordinary frames/heap blocks alongside mappings, mutates mapped contents
and verifies them after explicit unmap. Sixteen replacements reuse a warmed
virtual slot with a different physical frame while retaining the previous frame
and checking it remains unchanged. Drop/reallocation checks physical reuse and
zeroing. Exhaustion fills all available slots, checks original-frame return on
failure and verifies contents/accounting before and after cleanup. Full boot
mapping/protection checks run while owners remain live.

Core diagnostics independently issue bounded AT S1E1R probes, require expected
physical translation while mapped and faults after explicit unmap and Drop,
repeat 16 warmed remaps and verify zeroing on reuse. One deliberately forgotten
mapping retains its frame and slot; repeated diagnostics preserve capacity.
This adds one retained page to the existing two frame/heap diagnostic leaks.

The full suite ran 20 mapping compiler probes. Independent source review then
requested direct forged mapping construction coverage. The final 21 probes
(three accepted clients, 18 rejected clients) all passed with exact diagnostic
code checking using:

```sh
bash tools/dev.sh python3 tools/check_mapping_types.py \
  --library target/aarch64/boot-test/aarch64-unknown-none/release/libhm_kernel.rlib \
  --target aarch64-unknown-none \
  --output target/evidence/owned-mapping-types-final
```

See [final compiler output](owned-mapping-types-final.log) and
[results](owned-mappings-final/mapping-types-final/result.json). The archive's
source manifest records final source/tool hashes; only the added compiler
fixture changed after the full suite, and that final tool was executed above.
No kernel code changed after its passing suite. The first attempt to redirect
the suite log into container-owned `target/` failed with Permission denied
before any build started; redirecting into `docs/evidence/` succeeded. There
were no failed compiler, build or guest attempts in this increment.

The mechanism follows the independently reviewed
[ownership contract](../design/0008-arm-owned-mappings.md). It uses two permanent
table pages and 64 EL1 RW/NX mapping slots. This is not reclaimable table-tree
ownership, EL0 isolation, executable mapping support, SMP or native ARM cache
validation. QEMU success and independent AI review are not a complete soundness
audit. Remote CI has not executed this change.
