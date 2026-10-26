# hematite (working name)

A proposed Rust kernel implementing a declared subset of the Linux userspace
ABI. The project is in **Phase 0: evaluation and decisions**; no kernel is
implemented, and the name and license are not approved.

Start with [AGENTS.md](AGENTS.md), [PROGRESS.md](PROGRESS.md), and the
[decision records](docs/adr/). Proposed decisions are not authorization to
implement them. Evaluation checkouts and build artifacts live in `.work/`.

Repository checks: `python3 tools/check_repository.py`. These checks validate
decision-record structure only; they do not establish kernel compatibility or
satisfy the Phase 0 boot gate.
