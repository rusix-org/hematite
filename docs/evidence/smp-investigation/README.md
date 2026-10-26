# Two-CPU profile inspection

Executed 2026-10-26 using the existing pinned container at parent `08cf6d2`.
[Command](command.json), [QEMU version](environment.log), stdout/stderr and
the raw `two-cpu.dtb` are retained. QEMU exited 0 after generating the device
tree; no kernel was supplied or executed. This is configuration evidence,
not secondary boot, PSCI execution or SMP verification.

`python3 docs/evidence/smp-investigation/inspect_dtb.py` reproduces
[topology.json](topology.json), including the DTB SHA256 and selected property
bytes. The script is a bounded read-only inspection aid for this trusted QEMU
output, not the proposed kernel topology parser.

The generated `/cpus` nodes describe IDs 0 and 1 with `enable-method = psci`.
`/psci` advertises HVC, PSCI 1.0/0.2 compatibility, CPU_ON `0xc4000003` and
CPU_OFF `0x84000002`. The GIC distributor and CPU-interface ranges retain the
existing addresses. The physical timer interrupt tuple has PPI number 14 and
flags `0x304`. Exact values are observations of this pinned profile; no broad
hardware or firmware support is implied.
