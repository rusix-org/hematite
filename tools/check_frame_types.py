#!/usr/bin/env python3
"""Compile the actual core frame module as a probe library and test safe clients."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_frames::{Frame, Pool, Block, KernelBox};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::heap_api::OwnedBlock;
'''
PROBES = {
    "valid_client": ('''pub fn client(pool: &Pool) -> bool {
        let Ok(mut a) = pool.allocate() else { return false; };
        let Ok(mut b) = pool.allocate() else { return false; };
        a.bytes_mut()[0] = 17; b.bytes_mut()[0] = 29;
        a.bytes()[0] == 17 && b.bytes()[0] == 29
    }''', None),
    "frame_clone": ("pub fn client(f: &Frame<'_>) { let _ = <Frame<'_> as Clone>::clone(f); }", "E0277"),
    "pool_clone": ("pub fn client(p: &Pool) { let _ = <Pool as Clone>::clone(p); }", "E0277"),
    "outlive_pool": ("pub fn client<'a>(p: Pool) -> Frame<'a> { p.allocate().unwrap() }", "E0515"),
    "drop_borrowed": ("pub fn client(f: Frame<'_>) -> u8 { let b = f.bytes(); drop(f); b[0] }", "E0505"),
    "mutable_alias": ("pub fn client(mut f: Frame<'_>) { let a = f.bytes_mut(); let b = f.bytes_mut(); a[0] = b[0]; }", "E0499"),
    "forge_frame": ("pub fn client(p: &Pool) { let _ = Frame { pool: p, index: 0 }; }", "E0451"),
    "private_constructor": ("pub fn client(p: hm_kernel::frame_plan::Plan) { let _ = Pool::from_boot(p); }", "E0624"),
    "frame_send": ("fn send<T: Send>() {} pub fn client() { send::<Frame<'static>>(); }", None),
    "pool_sync": ("fn sync<T: Sync>() {} pub fn client() { sync::<Pool>(); }", None),
    "pool_send": ("fn send<T: Send>() {} pub fn client() { send::<Pool>(); }", None),
    "frame_sync": ("fn sync<T: Sync>() {} pub fn client() { sync::<Frame<'static>>(); }", None),
    "block_send_sync": ("fn shared<T: Send + Sync>() {} pub fn client() { shared::<Block<'static>>(); }", None),
    "box_cell_send": ("fn send<T: Send>() {} pub fn client() { send::<KernelBox<'static,core::cell::Cell<u64>>>(); }", None),
    "box_cell_sync": ("fn sync<T: Sync>() {} pub fn client() { sync::<KernelBox<'static,core::cell::Cell<u64>>>(); }", "E0277"),
    "box_borrowed_cell_send": ("fn send<T: Send>() {} pub fn client() { send::<KernelBox<'static,&'static core::cell::Cell<u64>>>(); }", "E0277"),
    "box_raw_pointer_send": ("fn send<T: Send>() {} pub fn client() { send::<KernelBox<'static,*mut u64>>(); }", "E0277"),
    "private_metadata_guard": ("pub fn client(_: hm_frames::frames::MetadataGuard<'_>) {}", "E0603"),

    "valid_heap": ("pub fn client(p: &Pool) { let mut b = p.allocate_heap(8193,8192).unwrap(); b.bytes_mut()[8192]=7; let mut v=p.box_value(17u64).unwrap(); *v+=1; }", None),
    "block_clone": ("pub fn client(b: &Block<'_>) { let _ = <Block<'_> as Clone>::clone(b); }", "E0277"),
    "box_clone": ("pub fn client(b: &KernelBox<'_,u64>) { let _ = <KernelBox<'_,u64> as Clone>::clone(b); }", "E0277"),
    "borrowed_value_escape": ("pub fn client(p: &Pool) -> KernelBox<'_, &u64> { let v=7; p.box_value(&v).unwrap() }", "E0515"),
    "box_outlive_pool": ("pub fn client<'a>(p: Pool) -> KernelBox<'a,u64> { p.box_value(7).unwrap() }", "E0515"),
    "drop_borrowed_box": ("pub fn client(b: KernelBox<'_,u64>) -> u64 { let r=&*b; drop(b); *r }", "E0505"),
    "box_mutable_alias": ("pub fn client(mut b: KernelBox<'_,u64>) { let a=&mut*b; let c=&mut*b; *a=*c; }", "E0499"),
    "private_backing_bytes": ("pub fn client(mut b: KernelBox<'_,u64>) { b._block.bytes_mut()[0]=1; }", "E0616"),
    "forge_box": ("pub fn client(b: Block<'_>) { let _=KernelBox { _block:b, value:core::ptr::null_mut::<u64>(), owns:core::marker::PhantomData }; }", "E0451"),
    "drop_borrowed_block": ("pub fn client(b: Block<'_>) -> u8 { let r=b.bytes(); drop(b); r[0] }", "E0505"),
    "box_send": ("fn send<T: Send>() {} pub fn client() { send::<KernelBox<'static,u64>>(); }", None),
    "box_sync": ("fn sync<T: Sync>() {} pub fn client() { sync::<KernelBox<'static,u64>>(); }", None),
}


def run(command, output, name):
    (output / f"{name}.command.json").write_text(json.dumps(command, indent=2) + "\n")
    try:
        result = subprocess.run(command, capture_output=True, timeout=20)
        (output / f"{name}.stdout.log").write_bytes(result.stdout)
        (output / f"{name}.stderr.log").write_bytes(result.stderr)
        return result.returncode, result.stderr.decode("utf-8", errors="replace")
    except (OSError, subprocess.TimeoutExpired) as error:
        (output / f"{name}.error.log").write_text(str(error) + "\n")
        return None, str(error)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    library = args.library.resolve()
    if not library.is_file():
        parser.error("build hm-kernel first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory")
    output.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).resolve().parents[1] / "crates/hm-core/src/frames.rs"
    wrapper = output / "hm_frames.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n'
                       f'#[path={json.dumps(str(source))}] pub mod frames;\n'
                       'pub use frames::{Frame, Pool, Block, KernelBox};\n')
    base = ["rustc", "--edition=2024", "--target", args.target, "-C", "panic=abort",
            "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    frame_lib = output / "libhm_frames.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_frames", str(wrapper),
                          "-o", str(frame_lib)], output, "library")
    results = {"library": {"passed": code == 0, "returncode": code}}
    if code == 0:
        for name, (body, rejection) in PROBES.items():
            client = output / f"{name}.rs"
            client.write_text(PREAMBLE + body + "\n")
            command = base + ["--crate-type=lib", "--emit=metadata", "--extern",
                              f"hm_frames={frame_lib}", str(client), "-o", str(output / f"{name}.rmeta")]
            code, diagnostics = run(command, output, name)
            passed = code == 0 if rejection is None else (code is not None and code != 0 and rejection in diagnostics)
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) + 1 and all(r["passed"] for r in results.values())
    result = {"passed": passed, "scope": "actual frame/heap module safe-client types; no runtime physical-memory claim", "probes": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
