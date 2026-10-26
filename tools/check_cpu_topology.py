#!/usr/bin/env python3
"""Decode pinned QEMU device trees using the actual kernel topology parser."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file() or output.exists():
        parser.error("require a host hm-kernel library and a fresh evidence directory")
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    results, trees = {}, []
    for cpus in (1, 2, 3):
        tree = output / f"{cpus}-cpu.dtb"
        command = ["qemu-system-aarch64", "-machine", f"virt,gic-version=2,dumpdtb={tree}",
                   "-cpu", "max", "-accel", "tcg", "-smp", str(cpus), "-m", "128M",
                   "-display", "none", "-nodefaults", "-no-user-config"]
        code, _ = run(command, output, f"dump-{cpus}")
        good = code == 0 and tree.is_file() and 40 <= tree.stat().st_size <= 2 * 1024 * 1024
        results[f"dump-{cpus}"] = {"passed": good, "returncode": code,
                                  "sha256": hashlib.sha256(tree.read_bytes()).hexdigest() if good else None}
        trees.append(tree)
    executable = output / "topology-host"
    if all(item["passed"] for item in results.values()):
        command = ["rustc", "--edition=2024", "-C", "panic=abort", "-C", "opt-level=2",
                   "--extern", f"hm_kernel={library}", "-L", f"dependency={library.parent / 'deps'}",
                   str(root / "tools/test_topology_host.rs"), "-o", str(executable)]
        code, _ = run(command, output, "compile-host")
        results["compile"] = {"passed": code == 0, "returncode": code}
        if code == 0:
            code, stderr = run([str(executable), *map(str, trees)], output, "run-host")
            stdout = (output / "run-host.stdout.log").read_text()
            results["execution"] = {
                "passed": code == 0 and not stderr and stdout == "HMTOPOLOGY HOST PASS profiles=3\n",
                "returncode": code, "sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            }
    result = {"passed": len(results) == 5 and all(item["passed"] for item in results.values()),
              "scope": "actual parser against generated device trees; no secondary execution",
              "checks": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
