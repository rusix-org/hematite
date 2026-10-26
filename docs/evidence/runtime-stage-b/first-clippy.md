Initial ARM Clippy check, 2026-10-26, exited 101:

```text
tools/dev.sh cargo clippy --workspace --all-features --locked --offline --target aarch64-unknown-none --target-dir target/group-stage-b/clippy-arm -- -D warnings

error: this function has too many arguments (8/7)
   --> crates/hm-core/src/runtime_checks.rs:361:1
361 | fn pair(
```

This records the observed diagnostic from the tool output. It is not a passing
lint check or runtime result. The driver is being refactored after compiled
stack review independently rejected the first runtime-test candidate.
