#!/usr/bin/env python3
"""Bounded negative ARM preemption profiles with strict failure evidence."""

import argparse
from datetime import datetime, timezone
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import time

from test_boot_profiles import prepare, write_json


NO_TIMER_SECONDS = 2
OVERFLOW_SECONDS = 10
MAX_LOG_BYTES = 16 * 1024 * 1024


def serial_checks(console, marker):
    lines = console.split(b"\n")
    complete = [line.removesuffix(b"\r") for line in lines[:-1]]
    records = [line for line in complete if b"HMPREEMPT" in line]
    return {
        "exact_launch_marker": records == [marker],
        "no_partial_marker": b"HMPREEMPT" not in lines[-1],
        "no_normal_report": b"HMTEST" not in console,
    }


def no_timer_checks(console, trace, result, entry_pc=None):
    checks = serial_checks(console, b"HMPREEMPT EXPECT NO-TIMER")
    returns = [line for line in trace.splitlines() if line.startswith(b"Exception return")]
    checks.update({
        "expected_host_timeout": result["timed_out"],
        "host_killed_blocked_guest": result["returncode"] == -9,
        "launched": result["launch_error"] is None,
        "no_fatal_marker": b"HMEXCEPTION" not in console,
        "no_exception_or_exit": not any(
            line.startswith(b"Taking exception")
            for line in trace.splitlines()),
        # First dispatch deliberately uses ERET to enable IRQ atomically with
        # worker entry. No delivery means that callback can never be resumed.
        "entered_first_worker_once": len(returns) == 1 and re.fullmatch(
            rb"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x[0-9a-fA-F]+",
            returns[0]) is not None,
        "complete_trace": bool(trace) and trace.endswith(b"\n"),
        "entry_matches_kernel_symbol": entry_pc is not None and len(returns) == 1
            and returns[0] == (f"Exception return from AArch64 EL1 to AArch64 EL1 PC {entry_pc:#x}").encode(),
    })
    return checks


def overflow_checks(console, trace, result, guard):
    checks = serial_checks(console, b"HMPREEMPT EXPECT OVERFLOW")
    complete = [line.removesuffix(b"\r") for line in console.split(b"\n")[:-1]]
    markers = [line for line in complete if b"HMEXCEPTION" in line]
    lines = trace.splitlines()
    abort = b"Taking exception 4 [Data Abort] on CPU 0"
    irq = b"Taking exception 5 [IRQ] on CPU 0"
    exit_event = b"Taking exception 16 [Semihosting call] on CPU 0"
    events = [line for line in lines if line.startswith(b"Taking exception")]
    fars = [int(match[1], 16) for line in lines
            if (match := re.fullmatch(rb"\.\.\.with FAR (0x[0-9a-fA-F]+)", line))]
    abort_index = lines.index(abort) if abort in lines else len(lines)
    checks.update({
        "fatal_exit": result["returncode"] == 2,
        "within_deadline": not result["timed_out"],
        "launched": result["launch_error"] is None,
        "exact_fatal_marker": markers == [b"HMEXCEPTION FATAL"],
        "no_partial_fatal_marker": b"HMEXCEPTION" not in console.split(b"\n")[-1],
        "single_abort_then_exit": len(events) >= 2 and events[-2:] == [abort, exit_event]
                                  and all(event == irq for event in events[:-2]),
        "translation_read_abort": lines.count(b"...with ESR 0x25/0x96000007") == 1,
        "worker_zero_guard": guard is not None and len(fars) == 1
                             and guard <= fars[0] < guard + 4096,
        "fault_not_resumed": not any(line.startswith(b"Exception return")
                                     for line in lines[abort_index + 1:]),
        "terminal_exit": bool(lines) and lines[-1] == b"...handling as semihosting call 0x20",
        "complete_trace": bool(trace) and trace.endswith(b"\n"),
    })
    return checks


def bounded_bytes(path):
    return path.read_bytes() if path.is_file() and path.stat().st_size <= MAX_LOG_BYTES else b""


def artifact(path):
    if not path.is_file():
        return {"exists": False, "within_size_limit": False}
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            digest.update(chunk)
    size = path.stat().st_size
    return {"exists": True, "bytes": size, "sha256": digest.hexdigest(),
            "within_size_limit": size <= MAX_LOG_BYTES}


def read_symbol(kernel, output, name):
    command = ["readelf", "-sW", str(kernel)]
    write_json(output / "symbols.command.json", command)
    metadata = {"returncode": None, "error": None, "symbol": name}
    values = []
    try:
        proc = subprocess.run(command, capture_output=True, timeout=5)
        metadata["returncode"] = proc.returncode
        (output / "symbols.stdout.log").write_bytes(proc.stdout)
        (output / "symbols.stderr.log").write_bytes(proc.stderr)
        if proc.returncode == 0:
            for line in proc.stdout.decode("ascii").splitlines():
                fields = line.split()
                if len(fields) >= 8 and fields[-1] == name:
                    values.append(int(fields[1], 16))
    except (OSError, subprocess.TimeoutExpired, UnicodeError, ValueError) as error:
        metadata["error"] = str(error)
    metadata["values"] = values
    write_json(output / "symbols.result.json", metadata)
    return values[0] if metadata["error"] is None and len(values) == 1 else None


def run_profile(kernel, output, overflow):
    command = prepare(output, kernel, "max", "aarch64", 128)
    timeout = OVERFLOW_SECONDS if overflow else NO_TIMER_SECONDS
    started = time.monotonic()
    result = {"started_utc": datetime.now(timezone.utc).isoformat(),
              "scope": "preemptible worker guard fault" if overflow else "disabled timer blocks nonyielding workers",
              "timeout_seconds": timeout, "timed_out": False,
              "returncode": None, "launch_error": None}
    with (output / "serial.log").open("wb") as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                  stdout=log, stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    result["timed_out"] = True
                    proc.kill()
                    proc.wait()
                result["returncode"] = proc.returncode
        except OSError as error:
            result["launch_error"] = str(error)
    result["elapsed_seconds"] = time.monotonic() - started
    serial_path, trace_path = output / "serial.log", output / "exceptions.log"
    console, trace = bounded_bytes(serial_path), bounded_bytes(trace_path)
    result["artifacts"] = {"serial": artifact(serial_path), "exceptions": artifact(trace_path)}
    if overflow:
        guard = read_symbol(kernel, output, "__thread0_guard")
        result["worker_zero_guard"] = guard
        checks = overflow_checks(console, trace, result, guard)
    else:
        entry_pc = read_symbol(kernel, output, "arm_preempt_start")
        result["entry_pc"] = entry_pc
        checks = no_timer_checks(console, trace, result, entry_pc)
    checks["logs_present_and_bounded"] = all(
        entry["exists"] and entry["within_size_limit"] for entry in result["artifacts"].values())
    result["checks"] = checks
    result["passed"] = all(checks.values())
    write_json(output / "result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-timer-kernel", type=Path, required=True)
    parser.add_argument("--overflow-kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not args.no_timer_kernel.is_file() or not args.overflow_kernel.is_file():
        parser.error("both negative kernel ELFs are required")
    output = args.output.resolve()
    if output.exists():
        parser.error("use a fresh evidence directory")
    output.mkdir(parents=True)
    results = {
        "no-timer": run_profile(args.no_timer_kernel.resolve(), output / "no-timer", False),
        "overflow": run_profile(args.overflow_kernel.resolve(), output / "overflow", True),
    }
    result = {"passed": all(profile["passed"] for profile in results.values()),
              "profiles": {name: profile["passed"] for name, profile in results.items()}}
    write_json(output / "result.json", result)
    print(result)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
