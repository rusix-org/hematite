#!/usr/bin/env python3
"""Run the bootstrap kernel in QEMU and validate its complete test report."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


CASES = (
    "write_protect", "nx", "smep", "smap", "wx", "stack_guard", "null_guard",
    "checked_range", "monitor_bounds",
)
SUCCESS_EXIT = 33
TIMEOUT_SECONDS = 20


def validate_run(console, returncode, *, timed_out=False, launch_error=None):
    """Require an exact ordered inventory, delimiters, and debug-exit status."""
    expected = [f"HMTEST BEGIN {len(CASES)}"]
    expected += [f"HMTEST PASS {name}" for name in CASES]
    expected += [f"HMTEST END {len(CASES)}"]
    errors = []
    observed = []
    in_report = False
    # Split only on actual serial newlines, not Unicode/control separators.
    lines = console.split(b"\n")
    for number, raw in enumerate(lines, start=1):
        if raw.endswith(b"\r"):
            raw = raw[:-1]
        if b"HMTEST" in raw:
            try:
                line = raw.decode("ascii")
            except UnicodeDecodeError:
                errors.append(f"Non-ASCII test record at line {number}")
                continue
            if number == len(lines):
                errors.append(f"Unterminated test record at line {number}")
            observed.append(line)
            if line.startswith("HMTEST BEGIN"):
                in_report = True
            elif line.startswith("HMTEST END"):
                in_report = False
        elif in_report:
            errors.append(f"Unexpected output inside test report at line {number}")
    if observed != expected:
        errors.append("Test records do not match the exact expected inventory and order")
    if returncode != SUCCESS_EXIT:
        errors.append(f"Expected QEMU debug-exit {SUCCESS_EXIT}, got {returncode}")
    if timed_out:
        errors.append(f"QEMU exceeded {TIMEOUT_SECONDS}-second deadline")
    if launch_error is not None:
        errors.append(f"QEMU could not launch: {launch_error}")
    return {
        "passed": not errors,
        "scope": "hematite bootstrap tests; no Linux ABI compatibility claim",
        "expected_cases": list(CASES),
        "observed_records": observed,
        "returncode": returncode,
        "timed_out": timed_out,
        "launch_error": launch_error,
        "errors": errors,
    }


def qemu_command(kernel, accel):
    return [
        "qemu-system-x86_64",
        "-machine", "microvm,pic=off,pit=off,rtc=off,isa-serial=on",
        "-accel", accel, "-cpu", "max" if accel == "tcg" else "host",
        "-m", "128M", "-smp", "1", "-nodefaults", "-no-user-config",
        "-display", "none", "-monitor", "none", "-serial", "stdio",
        "-no-reboot", "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-kernel", str(kernel),
    ]


def run_qemu(command, output):
    """Capture failure evidence as well as success; callers supply a fresh dir."""
    (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    started = datetime.now(timezone.utc).isoformat()
    start = time.monotonic()
    returncode, timed_out, launch_error = None, False, None
    with (output / "serial.log").open("wb") as log:
        try:
            completed = subprocess.run(
                command, stdin=subprocess.DEVNULL, stdout=log,
                stderr=subprocess.STDOUT, timeout=TIMEOUT_SECONDS,
            )
            returncode = completed.returncode
        except subprocess.TimeoutExpired:
            # subprocess.run kills and reaps QEMU before propagating timeout.
            timed_out = True
        except OSError as error:
            launch_error = str(error)
    result = validate_run(
        (output / "serial.log").read_bytes(), returncode,
        timed_out=timed_out, launch_error=launch_error,
    )
    result.update(started_utc=started, elapsed_seconds=time.monotonic() - start,
                  timeout_seconds=TIMEOUT_SECONDS)
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--accel", choices=("tcg", "kvm"), default="tcg")
    args = parser.parse_args()
    kernel, output = args.kernel.resolve(), args.output.resolve()
    if not kernel.is_file():
        parser.error(f"kernel ELF not found: {kernel}")
    if output.exists() and (not output.is_dir() or any(output.iterdir())):
        parser.error("output must be a new or empty directory; preserve earlier evidence")
    output.mkdir(parents=True, exist_ok=True)
    (output / "input.json").write_text(json.dumps({
        "kernel": str(kernel),
        "sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(),
        "accel": args.accel,
    }, indent=2) + "\n")
    result = run_qemu(qemu_command(kernel, args.accel), output)
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
