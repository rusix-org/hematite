#!/usr/bin/env python3
"""Strict two-CPU ARM startup/shutdown profiles; not concurrent scheduling."""

import argparse
from datetime import datetime, timezone
import hashlib
import re
from pathlib import Path
import subprocess
import sys
import time

from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_preemption_negative import artifact, bounded_bytes, MAX_LOG_BYTES


TIMEOUT_SECONDS = 20
CHECKS = 255  # Eight observations specified by lifecycle design 0018.
SYMBOLS = ("arm_secondary_root", "__secondary_stack_guard", "__secondary_stack_top")
NUMBER = r"(0|[1-9][0-9]{0,19})"
INFO = re.compile(rf"HMSMP INFO version={NUMBER} cpu_on=0 cpu_off=0 affinity_info=0")
SESSION = re.compile(
    rf"HMSMP SESSION generation={NUMBER} cpu=1 on=0 returned=1 off=1 "
    rf"checks={NUMBER} root={NUMBER} sp={NUMBER}")
NEGATIVES = {"stale-generation": ("STALE-GENERATION", 3),
             "cpu-off-return": ("CPU-OFF-RETURN", 4)}


def records(console):
    found, errors, positions = [], [], []
    inside = False
    lines = console.split(b"\n")
    for index, raw in enumerate(lines):
        raw = raw.removesuffix(b"\r")
        if b"HMSMP" in raw:
            if index == len(lines) - 1:
                errors.append("Unterminated lifecycle record")
            try:
                value = raw.decode("ascii")
            except UnicodeError:
                errors.append("Non-ASCII lifecycle record")
                continue
            found.append(value)
            positions.append(index)
            if value.startswith("HMSMP BEGIN"):
                inside = True
            elif value.startswith("HMSMP END"):
                inside = False
        elif inside and raw:
            errors.append("Unexpected output inside lifecycle report")
        elif inside and index < len(lines) - 1:
            errors.append("Blank line inside lifecycle report")
    return found, positions, errors


def valid_info(record):
    match = INFO.fullmatch(record)
    return bool(match and 0x10000 <= int(match[1]) <= 0x1ffff)


def trace_checks(trace, *, negative=False, topology=False):
    lines = trace.splitlines()
    events = [line for line in lines if line.startswith(b"Taking exception")]
    expected = (b"Taking exception 16 [Semihosting call] on CPU 1" if negative else
                b"Taking exception 16 [Semihosting call] on CPU 0")
    semihost = [line for line in events if b"[Semihosting call]" in line]
    peer_faults = [line for line in events if line.endswith(b"on CPU 1") and
                   any(kind in line for kind in (b"[Data Abort]", b"[Prefetch Abort]", b"[IRQ]"))]
    allowed_cpu0 = {b"Taking exception 11 [Hypervisor Call] on CPU 0"}
    if not negative:
        allowed_cpu0.update({b"Taking exception 3 [Prefetch Abort] on CPU 0",
                             b"Taking exception 4 [Data Abort] on CPU 0",
                             b"Taking exception 5 [IRQ] on CPU 0", expected})
    allowed_cpu1 = {b"Taking exception 11 [Hypervisor Call] on CPU 1"}
    if negative:
        allowed_cpu1.add(expected)
    if topology:
        allowed_cpu0 = {expected}
        allowed_cpu1 = set()
    cpu0_hvc = events.count(b"Taking exception 11 [Hypervisor Call] on CPU 0")
    cpu1_hvc = events.count(b"Taking exception 11 [Hypervisor Call] on CPU 1")
    return {
        "coordinator_hvc_presence": cpu0_hvc == 0 if topology else cpu0_hvc > 0,
        "exact_peer_hvc_count": cpu1_hvc == (0 if negative or topology else 2),
        "only_admitted_exception_events": all(event in allowed_cpu0 | allowed_cpu1 for event in events),
        "trace_complete_and_bounded": bool(trace) and len(trace) <= MAX_LOG_BYTES and trace.endswith(b"\n"),
        "exact_terminal_cpu": bool(events) and events[-1] == expected and semihost == [expected],
        "terminal_semihost_exit": bool(lines) and lines[-1] == b"...handling as semihosting call 0x20",
        "no_unexpected_peer_fault_or_irq": not peer_faults,
    }


def validate(console, trace, result, symbols, negative=None):
    found, positions, errors = records(console)
    checks = {
        "launched": result.get("launch_error") is None,
        "within_deadline": not result.get("timed_out", True),
        "no_fatal_serial": b"HMEXCEPTION" not in console,
        "no_other_scheduling_report": b"HMPREEMPT" not in console,
    }
    checks.update(trace_checks(trace, negative=negative in NEGATIVES, topology=negative == "topology"))
    post = None
    parsed_sessions = []
    if negative == "topology":
        checks["expected_exit"] = result.get("returncode") == 1
        checks["topology_rejection_inventory"] = found == ["HMSMP REJECT TOPOLOGY"]
        checks["no_success_or_baseline"] = b"HMTEST" not in console and b"HMSMP PASS" not in console
    elif negative is not None:
        marker, expected_exit = NEGATIVES[negative]
        checks["expected_exit"] = result.get("returncode") == expected_exit
        checks["negative_inventory"] = (len(found) == 3 and found[0] == "HMSMP BEGIN 2"
                                        and valid_info(found[1])
                                        and found[2] == f"HMSMP EXPECT {marker}")
        checks["no_success_or_baseline"] = b"HMTEST" not in console and b"HMSMP PASS" not in console
    else:
        checks["expected_exit"] = result.get("returncode") == 0
        checks["lifecycle_inventory"] = (len(found) == 6 and found[0] == "HMSMP BEGIN 2"
                                         and valid_info(found[1])
                                         and found[-2:] == ["HMSMP PASS lifecycle", "HMSMP END 2"])
        root = symbols.get("arm_secondary_root")
        guard = symbols.get("__secondary_stack_guard")
        top = symbols.get("__secondary_stack_top")
        checks["kernel_symbols"] = (all(type(value) is int for value in (root, guard, top))
                                    and root % 4096 == 0 and guard % 4096 == 0
                                    and top - guard == 4096 + 65536)
        sessions_okay = checks["lifecycle_inventory"] and checks["kernel_symbols"]
        if sessions_okay:
            for generation, record in enumerate(found[2:4], start=1):
                match = SESSION.fullmatch(record)
                if not match:
                    sessions_okay = False
                    continue
                observed_generation, mask, observed_root, sp = map(int, match.groups())
                parsed_sessions.append({"generation": observed_generation, "checks": mask,
                                        "root": observed_root, "sp": sp})
                sessions_okay &= (observed_generation == generation and mask == CHECKS
                                  and observed_root == root and guard + 4096 <= sp < top
                                  and sp % 16 == 0)
        checks["exact_sessions_and_kernel_addresses"] = sessions_okay
        baseline_positions = [index for index, line in enumerate(console.split(b"\n")) if b"HMTEST" in line]
        checks["baseline_after_lifecycle"] = bool(baseline_positions and positions
                                                  and min(baseline_positions) > max(positions))
        post = validate_run(console, result.get("returncode"), arch="aarch64",
                            timed_out=result.get("timed_out", True),
                            launch_error=result.get("launch_error"))
        checks["post_shutdown_baseline"] = post["passed"]
    checks["strict_serial_records"] = not errors
    return {"passed": all(checks.values()), "checks": checks, "errors": errors,
            "observed_records": found, "sessions": parsed_sessions,
            "hvc_counts": {str(cpu): trace.splitlines().count(
                f"Taking exception 11 [Hypervisor Call] on CPU {cpu}".encode())
                for cpu in (0, 1)},
            "post_shutdown": post,
            "scope": "secondary CPU lifecycle and post-shutdown regression; no SMP scheduling claim"}


def read_symbols(kernel, output):
    command = ["readelf", "-sW", str(kernel)]
    write_json(output / "symbols.command.json", command)
    result = {"returncode": None, "error": None}
    found = {name: [] for name in SYMBOLS}
    try:
        proc = subprocess.run(command, capture_output=True, timeout=5)
        result["returncode"] = proc.returncode
        (output / "symbols.stdout.log").write_bytes(proc.stdout)
        (output / "symbols.stderr.log").write_bytes(proc.stderr)
        if proc.returncode == 0:
            for line in proc.stdout.decode("ascii").splitlines():
                fields = line.split()
                if len(fields) >= 8 and fields[-1] in found:
                    found[fields[-1]].append(int(fields[1], 16))
    except (OSError, subprocess.TimeoutExpired, UnicodeError, ValueError) as error:
        result["error"] = str(error)
    result["values"] = found
    result["passed"] = (result["error"] is None and result["returncode"] == 0
                        and all(len(values) == 1 for values in found.values()))
    write_json(output / "symbols.result.json", result)
    return {name: values[0] for name, values in found.items() if len(values) == 1} if result["passed"] else {}


def run_profile(kernel, output, *, memory=128, negative=None, cpus=2):
    if cpus not in (1, 2):
        raise ValueError("Unsupported lifecycle CPU count")
    output.mkdir()
    command = qemu_command(kernel, "tcg", "aarch64")
    command[command.index("-smp") + 1] = str(cpus)
    command[command.index("-accel") + 1] = "tcg,thread=multi"
    command[command.index("-m") + 1] = f"{memory}M"
    command += ["-d", "int", "-D", str(output / "exceptions.log")]
    write_json(output / "command.json", command)
    write_json(output / "input.json", {"kernel": str(kernel), "sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(),
                                      "arch": "aarch64", "accel": "tcg,thread=multi", "cpus": cpus,
                                      "ram_mb": memory, "negative": negative})
    symbols = read_symbols(kernel, output)
    started = time.monotonic()
    execution = {"started_utc": datetime.now(timezone.utc).isoformat(), "timeout_seconds": TIMEOUT_SECONDS,
                 "returncode": None, "timed_out": False, "launch_error": None}
    with (output / "serial.log").open("wb") as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=TIMEOUT_SECONDS)
                except subprocess.TimeoutExpired:
                    execution["timed_out"] = True
                    proc.kill()
                    proc.wait()
                execution["returncode"] = proc.returncode
        except OSError as error:
            execution["launch_error"] = str(error)
    execution["elapsed_seconds"] = time.monotonic() - started
    paths = {"serial": output / "serial.log", "exceptions": output / "exceptions.log"}
    artifacts = {name: artifact(path) for name, path in paths.items()}
    result = validate(bounded_bytes(paths["serial"]), bounded_bytes(paths["exceptions"]),
                      execution, symbols, negative)
    result["checks"]["artifacts_present_and_bounded"] = all(
        entry["exists"] and entry["within_size_limit"] for entry in artifacts.values())
    result["checks"]["symbols_extracted"] = len(symbols) == len(SYMBOLS)
    result["passed"] = all(result["checks"].values())
    result.update(execution, artifacts=artifacts, symbols=symbols)
    write_json(output / "result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--stale-generation-kernel", type=Path, required=True)
    parser.add_argument("--cpu-off-return-kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not all(kernel.is_file() for kernel in (args.kernel, args.stale_generation_kernel, args.cpu_off_return_kernel)):
        parser.error("all three kernel ELFs are required")
    output = args.output.resolve()
    if output.exists():
        parser.error("use a fresh evidence directory; preserve earlier results")
    output.mkdir(parents=True)
    profiles = {}
    for memory in (128, 64):
        profiles[f"lifecycle-{memory}m"] = run_profile(args.kernel.resolve(), output / f"lifecycle-{memory}m", memory=memory)
    profiles["topology"] = run_profile(args.kernel.resolve(), output / "topology", cpus=1, negative="topology")
    for name, kernel in (("stale-generation", args.stale_generation_kernel), ("cpu-off-return", args.cpu_off_return_kernel)):
        profiles[name] = run_profile(kernel.resolve(), output / name, negative=name)
    result = {"passed": all(profile["passed"] for profile in profiles.values()),
              "profiles": {name: profile["passed"] for name, profile in profiles.items()}}
    write_json(output / "result.json", result)
    print(result)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
