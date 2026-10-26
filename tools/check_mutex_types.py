#!/usr/bin/env python3
"""Check actual core mutex types and optionally run hosted parallel stress."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
'''
PROBES = {
    "valid_owned_access": ('''pub fn client() -> u64 {
        let mut mutex = Mutex::new(7u64);
        { let mut guard = mutex.try_lock().unwrap(); *guard += 1; }
        *mutex.get_mut() += 1; mutex.into_inner()
    }''', None),
    "valid_send_nonsync_value": ('''fn both<T: Send + Sync>() {}
        pub fn client() { both::<Mutex<core::cell::Cell<u64>>>(); }
    ''', None),
    "valid_borrowed_value": ('''pub fn client(value: &u64) -> u64 {
        let mutex = Mutex::new(value); let guard = mutex.try_lock().unwrap(); **guard
    }''', None),
    "valid_const": ('''static LOCK: Mutex<u64> = Mutex::new(31);
        pub fn client() -> Option<u64> { LOCK.try_lock().map(|guard| *guard) }
    ''', None),
    "valid_forget": ('''pub fn client() -> u64 {
        let mut mutex = Mutex::new(17u64);
        core::mem::forget(mutex.try_lock().unwrap());
        *mutex.get_mut() = 29; mutex.into_inner()
    }''', None),
    "mutex_nonsend_sync": ('''fn sync<T: Sync>() {}
        pub fn client() { sync::<Mutex<*mut u8>>(); }
    ''', "E0277"),
    "mutex_nonsend_send": ('''fn send<T: Send>() {}
        pub fn client() { send::<Mutex<*mut u8>>(); }
    ''', "E0277"),
    "guard_send": ('''fn send<T: Send>() {}
        pub fn client() { send::<Guard<'static, u64>>(); }
    ''', "E0277"),
    "guard_sync": ('''fn sync<T: Sync>() {}
        pub fn client() { sync::<Guard<'static, u64>>(); }
    ''', "E0277"),
    "cell_guard_sync": ('''fn sync<T: Sync>() {}
        pub fn client() { sync::<Guard<'static, core::cell::Cell<u64>>>(); }
    ''', "E0277"),
    "guard_clone": ('''pub fn client(guard: &Guard<'_, u64>) {
        let _ = <Guard<'_, u64> as Clone>::clone(guard);
    }''', "E0277"),
    "mutex_clone": ('''pub fn client(mutex: &Mutex<u64>) {
        let _ = <Mutex<u64> as Clone>::clone(mutex);
    }''', "E0277"),
    "guard_outlives_mutex": ('''pub fn client<'a>() -> Guard<'a, u64> {
        let mutex = Mutex::new(17); mutex.try_lock().unwrap()
    }''', "E0515"),
    "borrow_outlives_guard": ('''pub fn client(mutex: &Mutex<u64>) -> &u64 {
        let guard = mutex.try_lock().unwrap(); &*guard
    }''', "E0515"),
    "borrowed_value_escape": ('''pub fn client<'a>() -> Mutex<&'a u64> {
        let value = 17; Mutex::new(&value)
    }''', "E0515"),
    "drop_borrowed_guard": ('''pub fn client(guard: Guard<'_, u64>) -> u64 {
        let value = &*guard; drop(guard); *value
    }''', "E0505"),
    "into_inner_while_locked": ('''pub fn client(mutex: Mutex<u64>) -> u64 {
        let guard = mutex.try_lock().unwrap(); let value = mutex.into_inner();
        value + *guard
    }''', "E0505"),
    "get_mut_while_locked": ('''pub fn client(mutex: &mut Mutex<u64>) {
        let guard = mutex.try_lock().unwrap(); *mutex.get_mut() = *guard;
    }''', "E0502"),
    "mutable_guard_alias": ('''pub fn client(mut guard: Guard<'_, u64>) {
        let first = &mut *guard; let second = &mut *guard; *first = *second;
    }''', "E0499"),
    "private_value": ('''pub fn client(mutex: &Mutex<u64>) { let _ = &mutex.value; }
    ''', "E0616"),
    "private_lock_state": ('''pub fn client(mutex: &Mutex<u64>) { let _ = &mutex.locked; }
    ''', "E0616"),
    "forge_guard": ('''pub fn client(mutex: &Mutex<u64>) {
        let _ = Guard { mutex, _not_send_sync: core::marker::PhantomData };
    }''', "E0451"),
}


def compile_module(output, root, library, target=None):
    output.mkdir(parents=True, exist_ok=True)
    source = root / "crates/hm-core/src/mutex.rs"
    wrapper = output / "hm_mutex.rs"
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n'
                       f'#[path={json.dumps(str(source))}] pub mod mutex;\n'
                       'pub use mutex::{Mutex, Guard};\n')
    base = ["rustc", "--edition=2024", "-C", "panic=abort",
            "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}"]
    if target is not None:
        base += ["--target", target]
    mutex_lib = output / "libhm_mutex.rlib"
    code, _ = run(base + ["--crate-type=rlib", "--crate-name=hm_mutex", str(wrapper),
                          "-o", str(mutex_lib)], output, "library")
    return base, mutex_lib, code


def hosted_stress(output, root, library):
    base, mutex_lib, code = compile_module(output, root, library)
    result = {"library": {"passed": code == 0, "returncode": code}}
    version_code, _ = run(["rustc", "-vV"], output, "rustc-version")
    version = (output / "rustc-version.stdout.log").read_text() if version_code == 0 else ""
    host = re.search(r"^host: (\S+)$", version, re.MULTILINE)
    result["toolchain"] = {"passed": version_code == 0 and host is not None,
                           "returncode": version_code}
    executable_sha256 = None
    if code == 0:
        executable = output / "mutex-host"
        code, _ = run(base + ["-C", "opt-level=2", "--extern", f"hm_mutex={mutex_lib}",
                              str(root / "tools/test_mutex_host.rs"), "-o", str(executable)],
                      output, "compile-host")
        result["compile"] = {"passed": code == 0, "returncode": code}
        if code == 0:
            executable_sha256 = hashlib.sha256(executable.read_bytes()).hexdigest()
            code, diagnostics = run([str(executable)], output, "run-host")
            stdout = (output / "run-host.stdout.log").read_text() if code is not None else ""
            marker = re.fullmatch(
                r"HMMUTEX HOST PASS workers=4 iterations=20000 updates=80000 rejected=4 retries=\d+ drops=1\n",
                stdout)
            result["execution"] = {"passed": code == 0 and not diagnostics and marker is not None,
                                   "returncode": code, "workers": 4, "updates": 80000}
    passed = len(result) == 4 and all(value["passed"] for value in result.values())
    return {"passed": passed, "scope": "actual kernel mutex under host OS threads; not guest SMP",
            "target": host.group(1) if host else None, "rustc_verbose_version": version,
            "executable_sha256": executable_sha256, "panic_strategy": "abort",
            "checks": result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", choices=["aarch64-unknown-none", "x86_64-unknown-none"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--host-library", type=Path,
                        help="actual hm-kernel rlib built for the compiler host; enables parallel stress")
    args = parser.parse_args()
    output, library = args.output.resolve(), args.library.resolve()
    if not library.is_file() or (args.host_library is not None and not args.host_library.is_file()):
        parser.error("build every requested hm-kernel library first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory")
    root = Path(__file__).resolve().parents[1]
    base, mutex_lib, code = compile_module(output, root, library, args.target)
    results = {"library": {"passed": code == 0, "returncode": code}}
    if code == 0:
        for name, (body, rejection) in PROBES.items():
            client = output / f"{name}.rs"
            client.write_text(PREAMBLE + body + "\n")
            command = base + ["--crate-type=lib", "--emit=metadata", "--error-format=json",
                              "--extern", f"hm_mutex={mutex_lib}", str(client),
                              "-o", str(output / f"{name}.rmeta")]
            code, diagnostics = run(command, output, name)
            passed = code == 0 if rejection is None else (
                code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
            results[name] = {"passed": passed, "returncode": code, "expected_rejection": rejection}
    passed = len(results) == len(PROBES) + 1 and all(value["passed"] for value in results.values())
    result = {"passed": passed, "scope": "actual core mutex safe-client type boundary", "probes": results}
    if args.host_library is not None:
        result["host_stress"] = hosted_stress(output / "host", root, args.host_library.resolve())
        result["passed"] &= result["host_stress"]["passed"]
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
