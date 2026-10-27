# 0029 supplement: boxed preparation stack ownership

Status: experiment rejected and reverted; no guest approval from this change.

The second task-matrix candidate requires 72,480 bytes against the existing
65,536-byte stack. Its reachable load_boxed frame is 17,936 bytes and retains
both the loaded Result and a moved Space. Driver-only reductions did not clear
the gate. Do not change the stack size, allowance, allocation order or tests.

Pass the owned `Result<Space<'pool>, user_space::Error>` directly from
`load_boxed` into the existing noninline `box_loaded` helper. Move the existing
load-error match into that helper, before its preparation logic. It extracts
the successful Space, derives the same frame limit, calls the same prepare or
prepare_with_input method, and then the unchanged box_process function.

Preserve exact Error::Resources/Load/Prepare mapping and retirement decisions.
Load failure must never prepare, issue console authority or allocate a Process
box. Successful load, startup allocation, console issuance and Process boxing
keep their existing order. Each failed owned value is dropped exactly once
under the caller's existing Lease. No reference or owner escapes, and there is
no unsafe code, public API, layout, metadata-charge or counter-boundary change.

This is a compiler-measured experiment: ownership slots may still be generated
in the callee. Accept it only after source review and actual emitted stack
measurement show the full reachable chain fits with the unchanged allowance.
Rebuild all affected profiles and run existing regressions; preserve rejected
candidates. No claim of a reduction is made from the source shape alone.

The third candidate moved the duplicate storage into box_loaded: load_boxed fell
from17,936 to9,008 bytes while box_loaded grew from9,888 to18,832 (net+16).
The original helpers were restored. The fourth candidate instead uses lexical
Group cleanup and a task-tests-only noinline boot-init boundary; review0029
records the measured passing chain. No core ownership refactor is retained.
