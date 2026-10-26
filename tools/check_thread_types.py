#!/usr/bin/env python3
"""Compile actual ARM thread types and verify pinned large-frame stack probes."""

import argparse
import json
from pathlib import Path
import re
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_threads::Runner;
use hm_kernel::thread_api::{ThreadContext, ThreadJob, RunReport, ThreadError};
'''
PROBES = {
    "valid_callback_run": ('''fn task(context: &mut dyn ThreadContext, input: u64) -> u64 {
        let id = context.id(); context.yield_now(); input.wrapping_add(id as u64)
    }
    pub fn client(runner: &Runner) -> Result<RunReport, ThreadError> {
        runner.run(&[ThreadJob { entry: task, input: 17 }])
    }''', None),
    "valid_forget_borrow": ('''fn task(context: &mut dyn ThreadContext, input: u64) -> u64 {
        core::mem::forget(context); input
    }
    pub fn client(runner: &Runner) { let _ = runner.run(&[ThreadJob { entry: task, input: 29 }]); }
    ''', None),
    "runner_clone": ('''pub fn client(runner: &Runner) {
        let _ = <Runner as Clone>::clone(runner);
    }''', "E0277"),
    "runner_send": ('''fn send<T: Send>() {} pub fn client() { send::<Runner>(); }''', "E0277"),
    "runner_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Runner>(); }''', "E0277"),
    "private_constructor": ('''pub fn client() { let _ = Runner::from_boot(); }''', "E0624"),
    "forge_runner": ('''pub fn client() {
        let _ = Runner { _single_cpu: core::marker::PhantomData };
    }''', "E0451"),
    "private_register_context": ('''use hm_threads::threads::Context;
        pub fn client(_: &mut Context) {}
    ''', "E0603"),
    "private_token": ('''use hm_threads::threads::Token;
        pub fn client(_: &mut Token) {}
    ''', "E0603"),
    "capturing_callback": ('''pub fn client(runner: &Runner, input: u64) {
        let _ = runner.run(&[ThreadJob { entry: |_, _| input, input: 0 }]);
    }''', "E0308"),
    "borrowed_callback": ('''pub fn client(runner: &Runner, input: &u64) {
        let _ = runner.run(&[ThreadJob { entry: |_, _| *input, input: 0 }]);
    }''', "E0308"),
    "context_escape": ('''fn store(_: &'static mut dyn ThreadContext) {}
        pub fn client(context: &mut dyn ThreadContext) { store(context); }
    ''', "E0521"),
    "context_mutable_alias": ('''pub fn client(context: &mut dyn ThreadContext) {
        let first = &mut *context; let second = &mut *context;
        first.yield_now(); second.yield_now();
    }''', "E0499"),
    "context_send": ('''fn send<T: Send>() {} pub fn client() {
        send::<&mut dyn ThreadContext>();
    }''', "E0277"),
}


def check_stack_probes(output, target, root):
    fixture = root / "tools/fixtures/thread-large-frame.rs"
    ir_path, asm_path = output / "large-frame.ll", output / "large-frame.s"
    command = ["rustc", "--edition=2024", "--crate-type=lib", "--target", target,
               "-C", "panic=abort", "-C", "opt-level=3", "-C", "codegen-units=1",
               "-C", "lto=yes", "-C", "relocation-model=static",
               f"--emit=llvm-ir={ir_path},asm={asm_path}", str(fixture)]
    code, _ = run(command, output, "stack-probes")
    checks = {"compiler_succeeded": code == 0}
    if code == 0 and ir_path.is_file() and asm_path.is_file():
        ir, assembly = ir_path.read_text(), asm_path.read_text()
        checks["inline_probe_attribute"] = '"probe-stack"="inline-asm"' in ir
        setup = re.search(r"\bsub\s+(x\d+),\s*sp,\s*#32,\s*lsl\s*#12", assembly)
        loop = re.search(
            r"(?P<label>\.LBB[\w]+):\n\s*sub\s+sp,\s*sp,\s*#1,\s*lsl\s*#12\n"
            r"\s*ldr\s+xzr,\s*\[sp\]\n\s*cmp\s+sp,\s*(?P<bound>x\d+)\n"
            r"\s*b\.ne\s+(?P=label)", assembly)
        memset = re.search(r"\bbl\s+memset\b", assembly)
        checks["full_128k_page_probe_loop"] = bool(
            setup and loop and memset and setup[1] == loop["bound"]
            and setup.start() < loop.start() < memset.start())
    else:
        checks["outputs_present"] = False
    return {"passed": all(checks.values()), "returncode": code, "checks": checks,
            "scope": "pinned compiler fixture; actual worker guard overflow is tested in QEMU"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", choices=["aarch64-unknown-none"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output, library = args.output.resolve(), args.library.resolve()
    if not library.is_file():
        parser.error("build hm-kernel first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory")
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[1]
    source = root / "crates/hm-core/src/threads.rs"
    wrapper = output / "hm_threads.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n'
                       f'#[path={json.dumps(str(source))}] pub mod threads;\n'
                       'pub use threads::Runner;\n')
    base = ["rustc", "--edition=2024", "--target", args.target, "-C", "panic=abort",
            "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    thread_lib = output / "libhm_threads.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_threads", str(wrapper),
                          "-o", str(thread_lib)], output, "library")
    results = {"library": {"passed": code == 0, "returncode": code}}
    if code == 0:
        for name, (body, rejection) in PROBES.items():
            client = output / f"{name}.rs"
            client.write_text(PREAMBLE + body + "\n")
            command = base + ["--crate-type=lib", "--emit=metadata", "--error-format=json", "--extern",
                              f"hm_threads={thread_lib}", str(client), "-o", str(output / f"{name}.rmeta")]
            code, diagnostics = run(command, output, name)
            passed = code == 0 if rejection is None else (
                code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    stack_probes = check_stack_probes(output, args.target, root)
    passed = (len(results) == len(PROBES) + 1
              and all(result["passed"] for result in results.values())
              and stack_probes["passed"])
    result = {"passed": passed, "scope": "actual ARM thread types and pinned compiler stack probing",
              "probes": results, "stack_probes": stack_probes}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
