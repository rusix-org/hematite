# 0029 supplement: ordinary task-test bootstrap

Status: wiring approved in review 0029; compiled/guest acceptance pending. No injection hooks.

The `task-tests` feature implies `boot-tests`. ARM-only `task_checks` and
`task_inputs` modules contain the safe driver and immutable fixture archives.
The driver declares `forbid(unsafe_code)` and accepts borrowed Pool, Factory,
Executor, actual boot-root/vector addresses and a diagnostic writer.

`aarch64::task_tests(&mut BootPlatform) -> bool` follows the existing runtime-test
bootstrap: obtain the private initialized pool/topology, require CPU0 only,
perform the existing timer delivery/context/quiescence/deadline preflight, and
print `HMTASK PREFLIGHT checks=<observed bits> irqs=8`; success requires 15. Construct
Factory and Executor through their existing unsafe boot constructors, retaining
the same permanent mapping/exclusion/no-DMA/no-external-CPU proof as runtime_tests.
No owner or reference escapes the wrapper; the driver returns before locals drop.
The output writer uses the existing serial-byte operation and CRLF convention.

Boot dispatch gives existing secondary/process/startup-input/runtime profiles
precedence over task-tests when features are combined. Task-tests precedes
space/preemption/general boot-test dispatch. Failed driver/preflight exits false;
success runs the existing bootstrap suite and reports its result through the
existing EL1 exit. No EL0 semihosting, altered timer constants or new exception
path is introduced. Existing feature profiles keep their dispatch behavior.

The host builds a distinct `task-test` image, binds both exact archives and their
layout-derived manifest, and checks the fixed ordinary inventory at 64/128 MiB.
Compiler layout, source, all affected stack bounds and raw trace acceptance
remain prerequisites to claiming the new gate passes. The ordinary monitor demo
continues to use its existing archive and launch path.

The compiled fourth candidate isolates arch::init with a task-tests-only
inline(never) attribute. Existing initialization, error and return behavior is
unchanged; this prevents initialization temporaries from persisting through task
execution. Independent review checked both the task path and separate init
subtree. The baseline suite was already out of line and was not changed.
