#!/usr/bin/env python3
"""Check rejected CPU profiles and the live serial monitor in bounded QEMU runs."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time

from test_kernel import CASES, qemu_command


DEADLINE_SECONDS = 20
REJECT_EXITS = {"aarch64": 1, "x86_64": 35}
PROMPT = b"hm> "


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def prepare(output, kernel, cpu, arch="aarch64", ram_mb=128):
    output.mkdir()
    command = qemu_command(kernel, "tcg", arch)
    command[command.index("-cpu") + 1] = cpu
    command[command.index("-m") + 1] = f"{ram_mb}M"
    write_json(output / "command.json", command)
    write_json(output / "input.json", {
        "kernel": str(kernel),
        "sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(),
        "accel": "tcg", "cpu": cpu, "arch": arch, "ram_mb": ram_mb,
    })
    return command


def rejection_checks(console, result, arch="aarch64"):
    checks = {
        "explicit_boot_rejection_exit": result["returncode"] == REJECT_EXITS[arch],
        "no_test_report": b"HMTEST" not in console,
        "within_deadline": not result["timed_out"],
        "launched": result["launch_error"] is None,
    }
    if arch == "aarch64":
        # QEMU itself exits 1 on invalid configuration. Require a complete
        # guest-issued marker so a launch/configuration failure cannot pass.
        lines = [line.removesuffix(b"\r") for line in console.split(b"\n")[:-1]]
        checks["explicit_pan_rejection_marker"] = lines.count(b"HMBOOT REJECT PAN") == 1
    return checks


def run_rejected_cpu(output, kernel, feature, arch="aarch64"):
    cpu = "cortex-a72" if arch == "aarch64" else f"max,-{feature}"
    command = prepare(output, kernel, cpu, arch)
    started = time.monotonic()
    result = {
        "scope": f"bootstrap rejects CPU without {feature}",
        "arch": arch,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "timeout_seconds": DEADLINE_SECONDS,
        "returncode": None, "timed_out": False, "launch_error": None,
        "checks": {}, "errors": [],
    }
    with (output / "serial.log").open("wb") as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                  stdout=log, stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=DEADLINE_SECONDS)
                except subprocess.TimeoutExpired:
                    result["timed_out"] = True
                    proc.kill()
                    proc.wait()
                result["returncode"] = proc.returncode
        except OSError as error:
            result["launch_error"] = str(error)
    console = (output / "serial.log").read_bytes()
    result["checks"] = rejection_checks(console, result, arch)
    result["errors"] = [name for name, passed in result["checks"].items() if not passed]
    result["passed"] = not result["errors"]
    result["elapsed_seconds"] = time.monotonic() - started
    write_json(output / "result.json", result)
    return result


def monitor_steps():
    help_body = (b"help   - show commands\nstatus - inspect boot protections\n"
                 b"test   - run bootstrap tests\n")
    status_body = b"".join(name + b": OK\n" for name in (
        b"write protection", b"NX", b"user execution protection",
        b"user access protection", b"W^X mappings",
        b"stack guard", b"null guard",
    )) + b"Bootstrap only: one CPU; no userspace, storage, or networking.\n"
    test_lines = [f"HMTEST BEGIN {len(CASES)}"]
    test_lines += [f"HMTEST PASS {name}" for name in CASES]
    test_lines += [f"HMTEST END {len(CASES)}", "Bootstrap tests passed."]
    test_body = ("\n".join(test_lines) + "\n").encode("ascii")
    return (
        ("help", b"help\n", b"help\n" + help_body),
        ("status", b"status\n", b"status\n" + status_body),
        ("tests", b"test\n", b"test\n" + test_body),
        ("oversized_line", b"x" * 65 + b"\n",
         b"x" * 64 + b"\nRejected: use at most 64 printable ASCII bytes.\n"),
        ("unknown_command", b"unknown\n", b"unknown\nUnknown command; type help.\n"),
        ("help_after_rejection", b"help\n", b"help\n" + help_body),
    )


def run_monitor(output, kernel, arch="aarch64", ram_mb=128):
    command = prepare(output, kernel, "max", arch, ram_mb)
    started = time.monotonic()
    deadline = started + DEADLINE_SECONDS
    result = {
        "scope": "live bootstrap monitor; host stops VM after serial interactions",
        "arch": arch,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "timeout_seconds": DEADLINE_SECONDS,
        "returncode": None, "timed_out": False, "launch_error": None,
        "host_terminated": False, "host_killed": False,
        "checks": {}, "errors": [], "interactions": [],
    }
    proc = None
    pending = bytearray()
    with (output / "serial.log").open("wb") as log, selectors.DefaultSelector() as selector:
        def read_prompt():
            while True:
                position = pending.find(PROMPT)
                if position >= 0:
                    response = bytes(pending[:position])
                    del pending[:position + len(PROMPT)]
                    return response
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("monitor interaction deadline exceeded")
                events = selector.select(remaining)
                if not events:
                    raise TimeoutError("monitor interaction deadline exceeded")
                data = os.read(proc.stdout.fileno(), 4096)
                if not data:
                    raise RuntimeError("QEMU output closed before the expected prompt")
                log.write(data)
                log.flush()
                pending.extend(data)

        try:
            proc = subprocess.Popen(command, stdin=subprocess.PIPE,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    bufsize=0)
            selector.register(proc.stdout, selectors.EVENT_READ)
            banner = read_prompt()
            result["checks"]["initial_prompt_and_banner"] = (
                b"Hematite bootstrap monitor (local working name)" in banner
                and b"Commands: help, status, test. This is not a Linux shell." in banner
            )
            if arch == "aarch64":
                expected_memory = (f"Boot RAM bytes: 0x{ram_mb * 1024 * 1024:016x} "
                                   "(discovered; not allocated)").encode("ascii")
                lines = banner.replace(b"\r\n", b"\n").split(b"\n")
                result["checks"]["discovered_ram_matches_vm"] = lines.count(expected_memory) == 1
            for name, request, expected in monitor_steps():
                # No input is sent until the kernel has initialized its UART
                # and emitted a prompt. Each command awaits its own next prompt.
                if proc.stdin.write(request) != len(request):
                    raise RuntimeError("short write to QEMU input")
                response = read_prompt()
                normalized = response.replace(b"\r\n", b"\n")
                result["checks"][name] = normalized == expected
                result["interactions"].append({
                    "name": name, "input_hex": request.hex(),
                    "expected_response_hex": expected.hex(),
                    "response_hex": response.hex(),
                })
            # An unexpected guest exit is a failure, even if its last prompt
            # was already in the output pipe. A normal monitor remains alive.
            result["checks"]["monitor_still_running"] = proc.poll() is None
        except TimeoutError as error:
            result["timed_out"] = True
            result["errors"].append(str(error))
        except OSError as error:
            result["launch_error"] = str(error)
            result["errors"].append(str(error))
        except RuntimeError as error:
            result["errors"].append(str(error))
        finally:
            if proc is not None:
                if proc.poll() is None:
                    result["host_terminated"] = True
                    proc.terminate()
                try:
                    tail, _ = proc.communicate(timeout=2)
                except subprocess.TimeoutExpired:
                    result["host_killed"] = True
                    proc.kill()
                    tail, _ = proc.communicate()
                if tail:
                    log.write(tail)
                result["returncode"] = proc.returncode
    records = []
    for raw in (output / "serial.log").read_bytes().split(b"\n"):
        if b"HMTEST" in raw:
            records.append(raw.removesuffix(b"\r"))
    expected_records = [f"HMTEST BEGIN {len(CASES)}".encode("ascii")]
    expected_records += [f"HMTEST PASS {name}".encode("ascii") for name in CASES]
    expected_records += [f"HMTEST END {len(CASES)}".encode("ascii")]
    result["checks"]["complete_test_inventory"] = records == expected_records
    expected_checks = {"initial_prompt_and_banner", "monitor_still_running"}
    if arch == "aarch64":
        expected_checks.add("discovered_ram_matches_vm")
    expected_checks.add("complete_test_inventory")
    expected_checks.update(name for name, _, _ in monitor_steps())
    for name in sorted(expected_checks):
        if not result["checks"].get(name, False):
            result["errors"].append(f"missing or failed check: {name}")
    result["passed"] = not result["errors"]
    result["elapsed_seconds"] = time.monotonic() - started
    write_json(output / "result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-kernel", type=Path, required=True)
    parser.add_argument("--monitor-kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--arch", choices=("aarch64", "x86_64"), default="aarch64")
    args = parser.parse_args()
    test_kernel, monitor_kernel = args.test_kernel.resolve(), args.monitor_kernel.resolve()
    output = args.output.resolve()
    for kernel in (test_kernel, monitor_kernel):
        if not kernel.is_file():
            parser.error(f"kernel ELF not found: {kernel}")
    if output.exists() and (not output.is_dir() or any(output.iterdir())):
        parser.error("output must be new or empty; preserve earlier evidence")
    output.mkdir(parents=True, exist_ok=True)
    results = {}
    features = ("pan",) if args.arch == "aarch64" else ("nx", "smep", "smap")
    for feature in features:
        name = f"missing-{feature}"
        results[name] = run_rejected_cpu(output / name, test_kernel, feature, args.arch)
    results["monitor"] = run_monitor(output / "monitor", monitor_kernel, args.arch)
    if args.arch == "aarch64":
        results["memory-64m"] = run_monitor(output / "memory-64m", monitor_kernel, args.arch, 64)
    summary = {
        "passed": all(result["passed"] for result in results.values()),
        "scope": "bootstrap CPU rejection and serial monitor; no Linux ABI claims",
        "profiles": {name: result["passed"] for name, result in results.items()},
        "arch": args.arch,
    }
    write_json(output / "result.json", summary)
    print(json.dumps(summary, indent=2))
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
