#!/usr/bin/env python3
"""Compile actual owned-space types and reject authority/lifetime violations."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_spaces::{Factory, Space, Pool};
use hm_kernel::elf::Image;
'''
PROBES = {
    "valid_load": ('''pub fn client<'p>(factory: &Factory<'p>, input: &[u8]) -> Option<Space<'p>> {
        factory.load(&Image::parse(input).ok()?, 284).ok()
    }''', None),
    "valid_release_input": ('''pub fn client<'p>(factory: &Factory<'p>) -> Option<Space<'p>> {
        let mut input = [0u8; 8192];
        let image = Image::parse(&input).ok()?;
        let space = factory.load(&image, 284).ok()?;
        drop(image); input.fill(0); Some(space)
    }''', None),
    "valid_forget": ('''pub fn client(space: Space<'_>) { core::mem::forget(space); }''', None),
    "valid_copied_inspection": ('''pub fn client(space: Space<'_>) -> [u8; 8] {
        let mut output = [0; 8]; let _ = space.read_into(space.entry(), &mut output);
        drop(space); output
    }''', None),
    "private_constructor": ('''pub fn client(pool: &Pool, topology: &hm_kernel::cpu_topology::Topology) {
        let _ = Factory::from_boot(pool, topology);
    }''', "E0624"),
    "private_frames": ('''pub fn client(space: Space<'_>) { let _ = space.pages; }''', "E0616"),
    "private_tables": ('''pub fn client(space: Space<'_>) { let _ = space.tables; }''', "E0616"),
    "space_clone": ('''pub fn client(space: &Space<'_>) { let _ = <Space<'_> as Clone>::clone(space); }''', "E0277"),
    "factory_clone": ('''pub fn client(factory: &Factory<'_>) { let _ = <Factory<'_> as Clone>::clone(factory); }''', "E0277"),
    "space_send": ('''fn send<T: Send>() {} pub fn client() { send::<Space<'static>>(); }''', "E0277"),
    "space_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Space<'static>>(); }''', "E0277"),
    "factory_send": ('''fn send<T: Send>() {} pub fn client() { send::<Factory<'static>>(); }''', "E0277"),
    "factory_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Factory<'static>>(); }''', "E0277"),
    "space_use_after_drop": ('''pub fn client(space: Space<'_>) -> u64 { drop(space); space.entry() }''', "E0382"),
    "space_outlive_pool": ('''pub fn client(make: for<'p> fn(&'p Pool) -> Factory<'p>, pool: Pool, input: &[u8]) -> Option<Space<'static>> {
        let factory = make(&pool); factory.load(&Image::parse(input).ok()?, 284).ok()
    }''', "E0515"),
    "forge_factory": ('''pub fn client(pool: &Pool) {
        let _ = Factory { pool, cpus: 1, _local: core::marker::PhantomData };
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
    modules = ("frames", "cpu_local", "threads", "preempt", "secondary", "user_space")
    wrapper = output / "hm_spaces.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n' + ''.join(
        f'#[path={json.dumps(str(source / (name + ".rs")))}] mod {name};\n' for name in modules) +
        'pub use user_space::{Factory, Space};\npub use frames::Pool;\n')
    (output / "input.json").write_text(json.dumps({"library": str(library),
        "sha256": hashlib.sha256(library.read_bytes()).hexdigest(), "target": args.target,
        "expected_probes": list(PROBES), "sources": {name: hashlib.sha256(
            (source / (name + ".rs")).read_bytes()).hexdigest() for name in modules}}, indent=2) + '\n')
    base = ["rustc", "--edition=2024", "--target", args.target, "-C", "panic=abort",
        "--cfg", 'feature="secondary-tests"', "--cfg", 'feature="smp-tests"',
        "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    compiled = output / "libhm_spaces.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_spaces", str(wrapper), "-o", str(compiled)], output, "library")
    results = {"library": {"passed": code == 0 and compiled.is_file(), "returncode": code}}
    if results["library"]["passed"]:
        for name, (body, rejection) in PROBES.items():
            client, metadata = output / f"{name}.rs", output / f"{name}.rmeta"
            client.write_text(PREAMBLE + body + '\n')
            code, diagnostics = run(base + ["--crate-type=lib", "--emit=metadata", "--error-format=json",
                "--extern", f"hm_spaces={compiled}", str(client), "-o", str(metadata)], output, name)
            intended = rejected_only_for(diagnostics, rejection)
            passed = code == 0 and metadata.is_file() if rejection is None else code is not None and code != 0 and intended
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) + 1 and all(result["passed"] for result in results.values())
    result = {"passed": passed, "probes": results,
        "scope": "actual owned-space/frame safe-client authority and lifetime boundaries; no execution claim"}
    (output / "result.json").write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
