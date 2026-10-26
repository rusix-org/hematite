# Startup input StageB compiled stack review

Independent AI review; static emitted-code inspection, not a measured watermark.
No guest approval is implied until the complete final candidate inventory passes.

## Initial partial candidate

The first build stopped on the standalone startup-input feature's missing
arm_vectors extern cfg. Its preceding normal ARM candidate was inspected from
[first-candidates](first-candidates/inputs.json), ELF
`e33eb4ae20c4c260eca1ac566b260c06b70700a957273d8aa7a94ee3442ab17a`,
emitted assembly
`e8ee6b2c1afede717f0ef8fd2735f7881e824695510c1fd8cd224b1c2250d178`.
The actual normal execution chain has these simultaneously retained frames:

| Function | Bytes |
| --- | ---: |
| boot_main |2512|
| run_monitor |448|
| run_command |608|
| State::run_inner |1472|
| load_and_run |17984|
| run_loaded |29632|
| Executor::run |9872|
| Executor::execute |960|

Sum63,488. The reviewed640-byte conservative fixed helper allowance gives
64,128, leaving1,408 below65,536. Both State entry wrappers tail-branch and
allocate no frame. Input preparation uses9,088, startup installation112 and
encoding32; their branch is smaller than the execution branch. No4096-byte
encoding temporary is introduced. The retained command frame includes the new
owned input storage. Baseline diagnostics and other profiles still require
final inventory comparison. The process-test ELF remains byte-identical to
prior-reviewed`e93bc4336086267b592f032291ed0ce33fee5ab8a1983dc3940e89fa5b6caa34`.

This is a provisional normal-path result. The new startup-input guest profile
and full22 final candidates remain pending.

## Rejected startup-input candidate

The second inventory's new startup-input-test has ELF SHA-256
`710c0de44cfc313a152d80523ddbb8d2d39e2017b8aacbf8b6fc67c48f88955b` and assembly SHA-256
`68790d7d1dc8c87e59037c8a5ce0d3c80a4041546834fbfd6a0c382d7a6dcb9e` (independently matched decompressed archive).
Its emitted owner-helper chains exceed the64KiB coordinator stack:

- Preparation: boot7536 + run16 + checks736 + one18976 + loaded19728 +
  prepare_case19632 + Executor::prepare_with_input9088 =75,712 before callees.
- Execution: the same chain through loaded, then run_ready11296 +
  Executor::run9872 + execute944 =69,104 before helpers.
- Prepare/drop: boot7536 + run16 + checks736 + prepare_drop18416 +
  drop_loaded20752 + prepare_case19632 + prepare9088 =76,176.

**Rejected before any guest.** Additional noinline owner-return helpers retain
large slots simultaneously. Refactor their lifetimes/call structure and inspect
a fresh emitted candidate; do not raise the stack size or relax the gate.

The complete second inventory contains22 profiles. Independently matched every
archived assembly hash. Nineteen existing profiles have byte-identical ELFs to
launcher-third, retaining their prior stack proof. The two changed existing
profiles are ARM monitor and x86 monitor. The ARM chain retains the provisional
64,128 bound above. On x86, only run_command's frame grows (zero to56 bytes in
emitted CFA); boot_main, run_monitor and every other function's frame remain
unchanged. This small command branch stays below the previously reviewed48,928
baseline-test maximum. The new startup-input profile alone blocks this batch.

Approved the proposed source refactor to combine preparation with consuming
execution/drop in one helper each, eliminating extra Process-return layers.
The source/Input lexical scope and independent full-page readback must remain;
only a fresh compiled candidate can establish its resulting bound.

## Refactored third candidate: accepted

Reviewed the exact source refactor: loaded now constructs/copies input in a
lexical block, ends both caller source and Input, reads back the full page and
consumes Process directly. drop_loaded likewise prepares and drops without an
intermediate Process-return helper. Error metadata stays borrowed from the owned
Result. The removed helper layers do not remove coverage or ownership checks.

Independently matched all22 current ELF hashes and archived assembly hashes to
[third-candidates](third-candidates/inputs.json). Only startup-input-test changes
from second. Its ELF is
`c200f6955bda17e1a4d244749c7700f46b110b3d0a3703b9daf9ff4373230b91`,
assembly
`ca5ba68d952b8aa2bc53ae3e0dfd16522f386b7098b04e4e521a306bbc67342a`.

The new live run chain is boot7536 + run16 + checks736 + one18976 + loaded21488
+ Executor::run9872 + execute944 =59,568. Adding the conservative640-byte
fixed helper allowance gives60,208. Preparation, actual exhaustion and inactive
prepare/drop branches are smaller: the conservative direct-call failure branch
including encode/panic helpers is57,264; prepare/drop is56,912. Full coordinator
baseline tests remain the larger61,808 direct-call bound, or62,448 with the same
640 allowance. Symbols place guard at0x4027c000 and top at0x4028d000, confirming
64KiB usable after the4KiB guard. No bound relies on increasing stack storage.

The unchanged ARM normal candidate retains64,128 including its allowance;
x86 and the19 byte-identical existing profiles retain the bounds above. Inspected
canonical app/probe disassemblies: summing even mutually exclusive downward
stack adjustments gives at most1,200 bytes for info,48 for echo, and64 for any
startup probe, well below their64KiB user stacks. The private assembly process
trap frame/exception stack mechanism is unchanged.

**Source and compiled-stack gates pass for this exact third22-profile inventory.**
First guest execution is approved within the frozen verification plan. Runtime
acceptance still requires independently inspected actual serial/trace evidence;
this static approval is not an execution result.
