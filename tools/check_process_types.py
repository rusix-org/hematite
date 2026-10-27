#!/usr/bin/env python3
"""Compile actual native-process types and reject authority/lifetime violations."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_process::{Executor, Process, RunReport, Error, Space, Pool, Factory};
use hm_process::{Group, TaskId};
use hm_kernel::elf::Image;
'''
PROBES = {
    "negative_no_production_constructor": ('''pub fn client(e: &mut Executor, f: Factory<'_>) { let _ = Group::new_negative_task(e, f); }''', "E0599"),
    "negative_no_production_observer": ('''pub fn client(g: &Group<'_, '_>) { let _ = g.negative_observation(); }''', "E0599"),
    "negative_no_production_archive": ('''pub fn client() { let _ = Group::negative_archive(); }''', "E0599"),
    "negative_no_production_case": ('''pub fn client(e: &Executor) { let _ = e.negative_case(); }''', "E0599"),
    "negative_no_production_mark": ('''pub fn client(e: &mut Executor) { let _ = e.mark_negative_profile(); }''', "E0599"),
    "negative_no_production_consume": ('''pub fn client(e: &mut Executor) { let _ = e.take_negative_profile(); }''', "E0599"),
    "valid_task_owned_root": ('''pub fn client<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, bytes: &mut [u8], archive: &'static [u8]) -> Option<Group<'e, 'p>> {
        let image = Image::parse(bytes).ok()?;
        let group = hm_process::task_group(e, f, &image, archive).ok()?;
        drop(image); bytes.fill(0); Some(group)
    }''', None),
    "valid_task_copied_completion": ('''pub fn client(mut g: Group<'_, '_>) -> Option<hm_process::TaskCompletion> {
        fn copied<T: Copy>() {} copied::<hm_process::TaskCompletion>();
        let root = g.task_root().ok()?;
        let done = g.task_reap(root).ok()?; drop(g);
        let _ = (done.closure().root().id(), done.closure().end(), done.closure().evidence(), done.live_frames());
        Some(done)
    }''', None),
    "task_constructor_private": ('''pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &'static [u8]) {
        let _ = Group::new_task(e, f, image, archive, true);
    }''', "E0624"),
    "task_executor_exclusive": ('''pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &'static [u8], p: Process<'_>) {
        let g = hm_process::task_group(e, f, image, archive).ok().unwrap();
        let _ = e.run(p); let _ = g.task_root();
    }''', "E0499"),
    "task_archive_static": ('''pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>, archive: &[u8]) {
        let _ = hm_process::task_group(e, f, image, archive);
    }''', "E0521"),
    "task_pool_lifetime": ('''pub fn client(e: &'static mut Executor, pool: Pool, make: for<'p> fn(&'p Pool)->Factory<'p>, image: &Image<'_>, archive: &'static [u8]) -> Option<Group<'static, 'static>> {
        hm_process::task_group(e, make(&pool), image, archive).ok()
    }''', "E0515"),
    "task_shared_reap": ('''pub fn client(g: &Group<'_, '_>, id: TaskId) { let _ = g.task_reap(id); }''', "E0596"),
    "task_wrong_token": ('''pub fn client(g: &Group<'_, '_>) { let _ = g.task_completion(3u64); }''', "E0308"),
    "task_forge_completion": ('''pub fn client(done: hm_process::TaskCompletion) { let _ = hm_process::TaskCompletion { ..done }; }''', "E0451"),
    "task_no_production_group_snapshot": ('''pub fn client(g: &Group<'_, '_>) { let _ = g.task_issuance_snapshot(); }''', "E0599"),
    "task_no_production_executor_snapshot": ('''pub fn client(e: &Executor) { let _ = e.task_issuance_snapshot(); }''', "E0599"),
    "task_no_production_launch_snapshot": ('''pub fn client(g: &Group<'_, '_>) { let _ = g.task_launch_generation(); }''', "E0599"),
    "valid_group_owned_inputs": ('''pub fn client<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, bytes: &mut [u8]) {
        let mut group = Group::new(e, f, 512).ok().unwrap();
        let image = Image::parse(bytes).ok().unwrap();
        let input = hm_kernel::startup_input::Input::from_bytes(b"job").ok().unwrap();
        let id = group.admit(&image, true, Some(&input)).ok().unwrap();
        drop(input); drop(image); bytes.fill(0);
        let copied = id; let _ = (id.group(), id.slot(), id.generation());
        let _ = group.info(copied); let _ = group.turn();
        let _ = group.request_cancel(id); let _ = group.completion(id); let _ = group.reap(id);
    }''', None),
    "valid_group_token_survives_drop": ('''pub fn client(e: &mut Executor, f: Factory<'_>, image: &Image<'_>) -> Option<TaskId> {
        let mut group = Group::new(e, f, 512).ok()?;
        let id = group.admit(image, false, None).ok()?; drop(group); Some(id)
    }''', None),
    "group_clone": ('''pub fn client(g: &Group<'_, '_>) { let _ = <Group<'_, '_> as Clone>::clone(g); }''', "E0277"),
    "group_copy": ('''fn copy<T: Copy>() {} pub fn client() { copy::<Group<'static, 'static>>(); }''', "E0277"),
    "group_send": ('''fn send<T: Send>() {} pub fn client() { send::<Group<'static, 'static>>(); }''', "E0277"),
    "group_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Group<'static, 'static>>(); }''', "E0277"),
    "group_shared_turn": ('''pub fn client(g: &Group<'_, '_>) { let _ = g.turn(); }''', "E0596"),
    "group_no_callback": ('''pub fn client(g: &mut Group<'_, '_>) { let _ = g.turn(|| {}); }''', "E0061"),
    "group_wrong_token": ('''pub fn client(g: &mut Group<'_, '_>) { let _ = g.request_cancel(1u64); }''', "E0308"),
    "group_no_production_release": ('''pub fn client(g: &mut Group<'_, '_>, id: TaskId) { let _ = g.release_context_probe(id); }''', "E0599"),
    "group_no_production_observer": ('''pub fn client(g: &Group<'_, '_>, id: TaskId) { let _ = g.test_release_word(id); }''', "E0599"),
    "group_no_production_probe": ('''pub fn client(g: &mut Group<'_, '_>) { let _ = g.admit_context_probe(()); }''', "E0599"),
    "group_forge_token": ('''pub fn client(id: TaskId) { let _ = TaskId { ..id }; }''', "E0451"),
    "group_executor_exclusive": ('''pub fn client(e: &mut Executor, f: Factory<'_>, p: Process<'_>) {
        let mut g = Group::new(e, f, 512).ok().unwrap(); let _ = e.run(p); let _ = g.turn();
    }''', "E0499"),
    "group_pool_lifetime": ('''pub fn client(e: &'static mut Executor, pool: Pool, make: for<'p> fn(&'p Pool)->Factory<'p>) -> Option<Group<'static, 'static>> {
        Group::new(e, make(&pool), 512).ok()
    }''', "E0515"),
    "valid_prepare_run": ('''pub fn client(e: &mut Executor, space: Space<'_>) -> Option<RunReport> {
        let process = e.prepare(space, true, 1).ok()?;
        let _ = (process.entry(), process.root_address(), process.frame_count(), process.handle());
        e.run(process).ok()
    }''', None),
    "valid_input_released": ('''pub fn client<'p>(e: &mut Executor, f: &Factory<'p>) -> Option<Process<'p>> {
        let mut input = [0u8; 8192]; let image = Image::parse(&input).ok()?;
        let space = f.load(&image, 284).ok()?; drop(image); input.fill(0);
        e.prepare(space, true, 1).ok()
    }''', None),
    "valid_startup_input_released": ('''pub fn client<'p>(e: &mut Executor, space: Space<'p>) -> Option<Process<'p>> {
        let mut source = [7u8; 256];
        let input = hm_kernel::startup_input::Input::from_bytes(&source).ok()?;
        source.fill(0);
        let process = e.prepare_with_input(space, true, 1, &input).ok()?;
        drop(input); Some(process)
    }''', None),
    "startup_input_wrong_type": ('''pub fn client(e: &mut Executor, space: Space<'_>) {
        let _ = e.prepare_with_input(space, true, 1, b"unchecked");
    }''', "E0308"),
    "startup_input_forge": ('''pub fn client(e: &mut Executor, space: Space<'_>, input: hm_kernel::startup_input::Input) {
        let forged = hm_kernel::startup_input::Input { ..input };
        let _ = e.prepare_with_input(space, true, 1, &forged);
    }''', "E0451"),
    "private_startup_observer": ('''pub fn client(p: &Process<'_>) { let _ = p.copy_startup(0, &mut [0; 64]); }''', "E0624"),
    "startup_pool_lifetime": ('''pub fn client(e: &mut Executor, pool: Pool, bytes: &[u8], make: for<'a> fn(&'a Pool)->Factory<'a>) -> Option<Process<'static>> {
        let factory = make(&pool); let space = factory.load(&Image::parse(bytes).ok()?, 284).ok()?;
        e.prepare_with_input(space, true, 1, &hm_kernel::startup_input::Input::absent()).ok()
    }''', "E0515"),
    "valid_forget_inactive": ('''pub fn client(p: Process<'_>) { core::mem::forget(p); }''', None),
    "valid_owned_report": ('''pub fn client(e: &mut Executor, p: Process<'_>) -> Option<RunReport> { e.run(p).ok() }''', None),
    "private_terminal_constructor": ('''pub fn client(t: &hm_kernel::cpu_topology::Topology) { let _ = Executor::from_boot_terminal(t); }''', "E0624"),
    "private_output_mode": ('''pub fn client(e: &Executor) { let _ = e.output; }''', "E0616"),
    "private_constructor": ('''pub fn client(t: &hm_kernel::cpu_topology::Topology) { let _ = Executor::from_boot(t); }''', "E0624"),
    "private_context": ('''pub fn client(p: &mut Process<'_>) { p.context[32] = 0; }''', "E0616"),
    "private_space": ('''pub fn client(p: Process<'_>) { let _ = p.space; }''', "E0616"),
    "private_session": ('''pub fn client(p: Process<'_>) { let _ = p.session; }''', "E0616"),
    "private_issuer": ('''pub fn client(e: &mut Executor) { e.issuer = hm_kernel::native::Issuer::new(); }''', "E0616"),
    "process_clone": ('''pub fn client(p: &Process<'_>) { let _ = <Process<'_> as Clone>::clone(p); }''', "E0277"),
    "executor_clone": ('''pub fn client(e: &Executor) { let _ = <Executor as Clone>::clone(e); }''', "E0277"),
    "process_send": ('''fn send<T: Send>() {} pub fn client() { send::<Process<'static>>(); }''', "E0277"),
    "process_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Process<'static>>(); }''', "E0277"),
    "executor_send": ('''fn send<T: Send>() {} pub fn client() { send::<Executor>(); }''', "E0277"),
    "executor_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Executor>(); }''', "E0277"),
    "process_consumed_once": ('''pub fn client(e: &mut Executor, p: Process<'_>) { let _ = e.run(p); let _ = e.run(p); }''', "E0382"),
    "no_callback": ('''pub fn client(e: &mut Executor, p: Process<'_>) { let _ = e.run(p, || {}); }''', "E0061"),
    "pool_lifetime": ('''pub fn client(e: &mut Executor, make: for<'p> fn(&'p Pool) -> Factory<'p>, pool: Pool, input: &[u8]) -> Option<Process<'static>> {
        let factory = make(&pool); let space = factory.load(&Image::parse(input).ok()?, 284).ok()?;
        e.prepare(space, true, 1).ok()
    }''', "E0515"),
    "forge_process": ('''pub fn client(space: Space<'_>, session: hm_kernel::native::Session) {
        let _ = Process { space, session: Some(session), context: [0;106], handle: 1 };
    }''', "E0451"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", choices=["aarch64-unknown-none"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file():
        parser.error("build hm-kernel first")
    if output.exists():
        parser.error("use a fresh output directory")
    output.mkdir(parents=True)
    source = Path(__file__).resolve().parents[1] / "crates/hm-core/src"
    modules = ("frames", "cpu_local", "threads", "preempt", "secondary", "user_space", "process", "process_group")
    inputs = [name + ".rs" for name in modules] + ["process_layout.rs", "process_task.rs", "process_task_turn.rs",
        "process-aarch64.S", "user-space-aarch64.S", "threads-aarch64.S",
        "preempt-aarch64.S", "preempt-probe-aarch64.S", "secondary-aarch64.S"]
    wrapper = output / "hm_process.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n' + ''.join(
        f'#[path={json.dumps(str(source / (name + ".rs")))}] mod {name};\n' for name in modules) +
        'pub use user_space::{Factory, Space};\npub use frames::Pool;\npub use process::{Executor, Process, RunReport, Error};\npub use process_group::{Group, TaskId, TaskCompletion};\n' +
        '''// Compile-only bridge calls the real crate-private constructor unchanged.
pub fn task_group<'e, 'p>(e: &'e mut Executor, f: Factory<'p>, image: &hm_kernel::elf::Image<'_>, archive: &'static [u8]) -> Result<Group<'e, 'p>, process_group::Error> {
    Group::new_task(e, f, image, archive, true)
}
''')
    (output / "input.json").write_text(json.dumps({"library": str(library),
        "sha256": hashlib.sha256(library.read_bytes()).hexdigest(), "target": args.target,
        "expected_probes": list(PROBES), "wrapper_sha256": hashlib.sha256(wrapper.read_bytes()).hexdigest(), "sources": {name: hashlib.sha256(
            (source / name).read_bytes()).hexdigest() for name in inputs}}, indent=2) + '\n')
    base = ["rustc", "--edition=2024", "--target", args.target, "-C", "panic=abort",
        "--cfg", 'feature="startup-input-tests"', "--cfg", 'feature="secondary-tests"', "--cfg", 'feature="smp-tests"',
        "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    compiled = output / "libhm_process.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_process", str(wrapper), "-o", str(compiled)], output, "library")
    results = {"library": {"passed": code == 0 and compiled.is_file(), "returncode": code}}
    if results["library"]["passed"]:
        for name, (body, rejection) in PROBES.items():
            client, metadata = output / f"{name}.rs", output / f"{name}.rmeta"
            client.write_text(PREAMBLE + body + '\n')
            code, diagnostics = run(base + ["--crate-type=lib", "--emit=metadata", "--error-format=json",
                "--extern", f"hm_process={compiled}", str(client), "-o", str(metadata)], output, name)
            intended = rejected_only_for(diagnostics, rejection)
            passed = code == 0 and metadata.is_file() if rejection is None else code is not None and code != 0 and intended
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) + 1 and all(result["passed"] for result in results.values())
    result = {"passed": passed, "probes": results,
        "scope": "actual native-process safe-client authority and lifetime boundaries; execution tested separately"}
    (output / "result.json").write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
