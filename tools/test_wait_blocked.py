#!/usr/bin/env python3
"""Verify all-blocked workers retain their live run until the host deadline."""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import subprocess
import sys
import time

from test_boot_profiles import prepare, write_json
from test_preemption_negative import artifact, bounded_bytes


def validate(console, result):
    complete = console.split(b"\n")
    records = [line.removesuffix(b"\r") for line in complete[:-1] if b"HMWAIT" in line]
    return {
        "registered_all_blocked": records == [b"HMWAIT BLOCKED"],
        "no_partial_marker": b"HMWAIT" not in complete[-1],
        "no_completion_or_fault": all(marker not in console for marker in
                                      (b"HMTEST", b"HMPREEMPT", b"HMEXCEPTION")),
        "host_deadline": result["timed_out"] and result["returncode"] == -9,
        "launched": result["launch_error"] is None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    kernel, output = args.kernel.resolve(), args.output.resolve()
    if not kernel.is_file() or output.exists():
        parser.error("require a kernel ELF and fresh evidence directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    command = prepare(output, kernel, "max", "aarch64", 128)
    result = {"started_utc": datetime.now(timezone.utc).isoformat(),
              "timeout_seconds": 2, "timed_out": False, "returncode": None,
              "launch_error": None, "scope": "retained blocked run; not wakeup or CPU-idle proof"}
    start = time.monotonic()
    with (output / "serial.log").open("wb") as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log,
                                  stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    result["timed_out"] = True
                    proc.kill()
                    proc.wait()
                result["returncode"] = proc.returncode
        except OSError as error:
            result["launch_error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - start
    result["checks"] = validate(bounded_bytes(output / "serial.log"), result)
    result["artifacts"] = {name: artifact(output / name) for name in ("serial.log", "exceptions.log")}
    result["checks"]["bounded_logs"] = all(
        item["exists"] and item["within_size_limit"] for item in result["artifacts"].values())
    result["passed"] = all(result["checks"].values())
    write_json(output / "result.json", result)
    print(result)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
