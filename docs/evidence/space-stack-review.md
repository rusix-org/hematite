# Owned-space compiled stack review

2026-10-26, independent AI inspection. No guest execution by this reviewer.
Parent build log: [space-third-build.log](space-third-build.log).
Input assembly:
`target/aarch64/space-test/aarch64-unknown-none/release/deps/hematite-9cf396a302e942c4.s`.
SHA-256: `f0ed39fa788478197a7b07d918024933a110153ac95e6678f67db52635077dd4`.
The parent's [hash record](space-third-build.sha256) also identifies the ELF.

The earlier candidate exceeded the 64 KiB coordinator stack before execution;
see [review 0022](../reviews/0022-arm-owned-address-spaces.md). The second
oversized assembly is retained in [space-oversized-stack.s.gz](space-oversized-stack.s.gz).
Separating guest cases into non-inlined functions removed their simultaneous
large return slots. This review applies to the third build; a later rebuild
must preserve the machine code or be checked again.

## Method and frame sizes

Read each emitted prologue, including stack-probe loops, and its CFI offsets;
follow direct calls to add simultaneously live frames. A local function's
maximum CFI offset is not by itself the aggregate bound. `boot_main` retains
its 7,440-byte frame while invoking the guest cases. The entry assembly does
not add a coordinator-stack frame before this call.

| Function | Bytes |
| --- | ---: |
| boot_main | 7,440 |
| space_checks::run | 16 |
| checks | 1,712 |
| fixture_case | 18,576 |
| budget_case | 10,624 |
| pressure_case | 11,648 |
| forget_case | 28,240 |
| geometry_max | 1,216 |
| geometry_cross | 8,400 |
| geometry_load | 18,992 |
| verify_bytes | 4,192 |
| probes | 5,616 |
| Factory::load | 17,264 |
| Space::build_tables | 0 |
| Space::audit | 48 |
| Space::probe | 960 |
| Space::buffer_visible | 0 |
| Space::read_into | 64 |
| Image::parse | 1,056 |

The largest new path is boot_main → run → checks → forget_case → load → audit:
**54,720 bytes**, leaving **10,816 bytes** below 65,536. The cross-page path
through geometry_cross → geometry_load → load → audit uses **53,872 bytes**.
Verification and AT-probe calls do not overlap the loader frame. The assembly
probe itself neither changes SP nor calls another routine.

Checked the linked builtins with:

```sh
llvm-objdump -d --disassemble-symbols=memcpy,memset,memcmp \
  target/aarch64/space-test/aarch64-unknown-none/release/hematite
```

All three are leaf functions with no stack adjustment. The loader's other
normal callees are build_tables and audit; cleanup drop glue uses 16 bytes.
Terminal bounds/slice failure paths reach the small panic/abort chain rather
than recursively formatting or unwinding owned values. Formatting happens on
shallower case/report paths; the inspected formatter frames are at most 256
bytes, followed by small pad/output helpers. New probe intervals have DAIF
fully masked, so asynchronous guest callbacks cannot add coordinator frames.

The existing post-probe baseline was checked too because it shares boot_main.
Its deepest inspected direct chain is boot_main (7,440) → run_tests (35,072)
→ cpu_topology_tests::valid (8,320) → extra_valid (4,240) → topology::parse
(6,464) → Node::property (128) → terminal slice/panic/abort chain (48):
**61,712 bytes**, leaving **3,824 bytes**. This is a static scoped call-path
inspection, not runtime stack-watermark instrumentation or a general bound for
future features. Existing interrupt/emergency and worker stacks are separate.

**Third-build stack gate passes for the reviewed profile.** The earlier
overflow remains preserved as a failure. Guest execution and full regression
results are separate evidence supplied by the integrating agent.
