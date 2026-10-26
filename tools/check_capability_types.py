#!/usr/bin/env python3
"""Compile positive and deliberately invalid clients of the capability model."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::capability::{with_model, Config, Owner, Rights, Handle, Model};
const CONFIG: Config = Config { byte_budget: 16, max_in_flight: 2, generation_limit: 2 };
'''
PROBES = {
    "valid_client": ('''
pub fn client() -> bool {
    with_model(CONFIG, |mut model| {
        let handle = model.create(Owner(1), 8, Rights::READ).unwrap();
        model.release(Owner(1), handle).is_ok()
    }).unwrap_or(false)
}
''', None),
    "cross_instance": ('''
pub fn client() {
    let _ = with_model(CONFIG, |mut first| {
        let handle = first.create(Owner(1), 8, Rights::READ).unwrap();
        let _ = with_model(CONFIG, |mut second| second.release(Owner(1), handle));
    });
}
''', "lifetime"),
    "escaping_handle": ('''
pub fn client() -> Handle<'static> {
    with_model(CONFIG, |mut model| model.create(Owner(1), 8, Rights::READ).unwrap()).unwrap()
}
''', "lifetime"),
    "private_generation": ('''
pub fn client(mut handle: Handle<'_>) { handle.generation = 0; }
''', "E0616"),
    "uncloneable_instance": ('''
pub fn client(model: &Model<'_>) { let _ = <Model<'_> as Clone>::clone(model); }
''', "E0277"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library = args.library.resolve()
    output = args.output.resolve()
    if not library.is_file():
        parser.error("build the hm-kernel library first")
    if output.exists() and any(output.iterdir()):
        parser.error("use a fresh output directory to preserve earlier results")
    output.mkdir(parents=True, exist_ok=True)
    results = {}
    for name, (body, rejection) in PROBES.items():
        source = output / f"{name}.rs"
        source.write_text(PREAMBLE + body)
        command = ["rustc", "--edition=2024", "--crate-type=lib", "--emit=metadata",
                   "--target", args.target, "-C", "panic=abort", "--extern",
                   f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}",
                   "-o", str(output / f"{name}.rmeta"), str(source)]
        (output / f"{name}.command.json").write_text(json.dumps(command, indent=2) + "\n")
        try:
            proc = subprocess.run(command, capture_output=True, timeout=20)
            diagnostics = proc.stderr.decode("utf-8", errors="replace")
            (output / f"{name}.stderr.log").write_bytes(proc.stderr)
            (output / f"{name}.stdout.log").write_bytes(proc.stdout)
            if rejection is None:
                passed = proc.returncode == 0
            elif rejection == "lifetime":
                passed = proc.returncode != 0 and (
                    "lifetime may not live long enough" in diagnostics
                    or "borrowed data escapes" in diagnostics)
            else:
                passed = proc.returncode != 0 and rejection in diagnostics
            results[name] = {"passed": passed, "returncode": proc.returncode,
                             "expected_rejection": rejection}
        except (OSError, subprocess.TimeoutExpired) as error:
            results[name] = {"passed": False, "error": str(error)}
    result = {"passed": all(r["passed"] for r in results.values()),
              "scope": "safe-client type boundary; not runtime isolation", "probes": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
