#!/usr/bin/env python3
"""Compile actual ARM mapping/frame modules and reject ownership violations."""
import argparse
import json
from pathlib import Path
import sys

from check_frame_types import run

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_mappings::{Mapper, MappedPage, Frame, Pool};
use hm_kernel::frame_api::OwnedFrame;
use hm_kernel::mapping_api::OwnedMapping;
'''
PROBES = {
    "valid_map_unmap": ('''pub fn client(m: &Mapper, mut f: Frame<'_>) -> bool {
        f.bytes_mut()[0] = 31;
        let Ok(mut page) = m.map(f) else { return false; };
        let observed = page.bytes()[0] == 31;
        page.bytes_mut()[0] = 47;
        let f = page.unmap();
        observed && f.bytes()[0] == 47
    }''', None),
    "valid_shared_reads": ('''pub fn client(p: &MappedPage<'_, '_>) -> bool {
        let a = p.bytes(); let b = p.bytes(); a[0] == b[0]
    }''', None),
    "valid_forget": ('''pub fn client(m: Mapper, p: Pool) {
        let Ok(f) = p.allocate() else { return; };
        let Ok(page) = m.map(f) else { return; };
        core::mem::forget(page); drop(m); drop(p);
    }''', None),
    "frame_after_map": ('''pub fn client(m: &Mapper, f: Frame<'_>) -> u8 {
        let _page = m.map(f); f.bytes()[0]
    }''', "E0382"),
    "frame_borrow_during_map": ('''pub fn client(m: &Mapper, f: Frame<'_>) -> u8 {
        let bytes = f.bytes(); let _page = m.map(f); bytes[0]
    }''', "E0505"),
    "borrow_after_unmap": ('''pub fn client(p: MappedPage<'_, '_>) -> u8 {
        let bytes = p.bytes(); let _f = p.unmap(); bytes[0]
    }''', "E0505"),
    "borrow_after_drop": ('''pub fn client(p: MappedPage<'_, '_>) -> u8 {
        let bytes = p.bytes(); drop(p); bytes[0]
    }''', "E0505"),
    "mutable_alias": ('''pub fn client(mut p: MappedPage<'_, '_>) {
        let a = p.bytes_mut(); let b = p.bytes_mut(); a[0] = b[0];
    }''', "E0499"),
    "shared_mutable_alias": ('''pub fn client(mut p: MappedPage<'_, '_>) {
        let a = p.bytes(); let b = p.bytes_mut(); b[0] = a[0];
    }''', "E0502"),
    "outlive_mapper": ('''pub fn client<'a, 'b>(m: Mapper, f: Frame<'b>) -> Option<MappedPage<'a, 'b>> {
        m.map(f).ok()
    }''', "E0515"),
    "outlive_pool": ('''pub fn client<'a>(m: &'a Mapper, p: Pool) -> Option<MappedPage<'a, 'a>> {
        let f = p.allocate().ok()?; m.map(f).ok()
    }''', "E0515"),
    "mapping_clone": ('''pub fn client(p: &MappedPage<'_, '_>) {
        let _ = <MappedPage<'_, '_> as Clone>::clone(p);
    }''', "E0277"),
    "mapper_clone": ('''pub fn client(m: &Mapper) { let _ = <Mapper as Clone>::clone(m); }''', "E0277"),
    "mapping_send": ('''fn send<T: Send>() {} pub fn client() { send::<MappedPage<'static, 'static>>(); }''', "E0277"),
    "mapping_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<MappedPage<'static, 'static>>(); }''', "E0277"),
    "mapper_sync": ('''fn sync<T: Sync>() {} pub fn client() { sync::<Mapper>(); }''', "E0277"),
    "private_backing": ('''pub fn client(p: MappedPage<'_, '_>) { let _ = p.frame; }''', "E0616"),
    "forge_mapping": ('''pub fn client(m: &Mapper, f: Frame<'_>) {
        let _ = MappedPage { mapper: m, slot: 0, frame: core::mem::ManuallyDrop::new(f) };
    }''', "E0451"),
    "integer_backing": ('''pub fn client(m: &Mapper) { let _ = m.map(0x40000000u64); }''', "E0308"),
    "double_unmap": ('''pub fn client(p: MappedPage<'_, '_>) { let _ = p.unmap(); let _ = p.unmap(); }''', "E0382"),
    "private_constructor": ('''pub fn client() { let _ = Mapper::from_boot(); }''', "E0624"),
}


def rejected_only_for(diagnostics, expected):
    """Require the intended compiler diagnostic, not an unrelated build error."""
    codes = []
    for line in diagnostics.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            return False
        if item.get("level") != "error":
            continue
        code = item.get("code")
        if code is not None:
            codes.append(code.get("code"))
        elif not item.get("message", "").startswith("aborting due to "):
            return False
    return bool(codes) and all(code == expected for code in codes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", choices=["aarch64-unknown-none"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    library = args.library.resolve()
    if not library.is_file():
        parser.error("build hm-kernel first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory")
    output.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).resolve().parents[1] / "crates/hm-core/src"
    wrapper = output / "hm_mappings.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n'
                       f'#[path={json.dumps(str(source / "frames.rs"))}] pub mod frames;\n'
                       f'#[path={json.dumps(str(source / "mappings.rs"))}] pub mod mappings;\n'
                       'pub use frames::{Frame, Pool};\npub use mappings::{Mapper, MappedPage};\n')
    base = ["rustc", "--edition=2024", "--target", args.target, "-C", "panic=abort",
            "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    mapping_lib = output / "libhm_mappings.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_mappings", str(wrapper),
                          "-o", str(mapping_lib)], output, "library")
    results = {"library": {"passed": code == 0, "returncode": code}}
    if code == 0:
        for name, (body, rejection) in PROBES.items():
            client = output / f"{name}.rs"
            client.write_text(PREAMBLE + body + "\n")
            command = base + ["--crate-type=lib", "--emit=metadata", "--error-format=json", "--extern",
                              f"hm_mappings={mapping_lib}", str(client), "-o", str(output / f"{name}.rmeta")]
            code, diagnostics = run(command, output, name)
            passed = code == 0 if rejection is None else (code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) + 1 and all(r["passed"] for r in results.values())
    result = {"passed": passed, "scope": "actual ARM mapping/frame safe-client types; runtime translation tested separately", "probes": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
