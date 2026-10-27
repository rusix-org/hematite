#!/usr/bin/env python3
"""Compile actual RW-copy privacy and owned-space clients; no public writer is exported."""
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
    'read_only_client': ("pub fn client(space: &Space<'_>) -> u64 { space.entry() }", None),
    'private_validate': ("pub fn client(space: &Space<'_>) { let _ = space.validate_rw(0, 1); }", 'E0624'),
    'private_copy': ("pub fn client(space: &mut Space<'_>) { let _ = space.copy_into_rw(0, &[1]); }", 'E0624'),
    'private_frames': ("pub fn client(space: Space<'_>) { let _ = space.pages; }", 'E0616'),
    'private_tables': ("pub fn client(space: Space<'_>) { let _ = space.tables; }", 'E0616'),
    'space_send': ("fn send<T: Send>() {} pub fn client() { send::<Space<'static>>(); }", 'E0277'),
    'space_sync': ("fn sync<T: Sync>() {} pub fn client() { sync::<Space<'static>>(); }", 'E0277'),
    'escape_backing': ("pub fn client<'a, 'p>(space: &'a mut Space<'p>) -> &'a mut [u8] { space.pages[0].as_mut().unwrap().bytes_mut() }", 'E0616'),
    'space_outlive_pool': ("pub fn client(make: for<'p> fn(&'p Pool) -> Factory<'p>, pool: Pool, input: &[u8]) -> Option<Space<'static>> { let factory = make(&pool); factory.load(&Image::parse(input).ok()?, 35).ok() }", 'E0515'),
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
        'pub use user_space::{Factory, Space};\npub use frames::Pool;\n' +
        'fn internal_adapter(space: &mut Space<\'_>, bytes: &[u8]) -> bool { space.validate_rw(0x100002000, bytes.len()).is_ok() && space.copy_into_rw(0x100002000, bytes).is_ok() }\n')
    (output / "input.json").write_text(json.dumps({"library": str(library),
        "sha256": hashlib.sha256(library.read_bytes()).hexdigest(), "target": args.target,
        "expected_probes": list(PROBES), "tool_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "assembly": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in source.glob("*.S")}, "sources": {name: hashlib.sha256(
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
        "scope": "actual private RW-copy methods and owned-space lifetimes; positive private adapter compiled, no execution claim"}
    (output / "result.json").write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
