#!/usr/bin/env python3
"""Run the separate ARM scheduling inventory, preserving bounded raw evidence."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys
import time

from test_kernel import qemu_command, validate_run
from check_timer_trace import ACK, IRQ, PREFIX, VECTOR, WRITE, RETURN, MAX_TRACE_BYTES


CASES = ("preempt_progress", "preempt_mixed", "preempt_limits", "preempt_core")
TIMEOUT_SECONDS = 20
DIAGNOSTIC_IRQS = 16


def validate(console, returncode, timed_out=False, launch_error=None):
    expected = [f"HMPREEMPT BEGIN {len(CASES)}"]
    expected += [f"HMPREEMPT PASS {name}" for name in CASES]
    expected += [f"HMPREEMPT END {len(CASES)}"]
    records, errors, irq_counts = [], [], []
    in_report = False
    post_report_seen = False
    lines = console.split(b"\n")
    for number, raw in enumerate(lines):
        raw = raw.removesuffix(b"\r")
        if b"HMPREEMPT" in raw:
            count = re.fullmatch(rb"HMPREEMPT IRQ ([0-9a-f]{16})", raw)
            if count and not in_report and not post_report_seen and records == expected and number < len(lines) - 1:
                irq_counts.append(int(count[1], 16))
                continue
            if number == len(lines) - 1:
                errors.append("Unterminated report record")
            try:
                line = raw.decode("ascii")
            except UnicodeDecodeError:
                errors.append("Non-ASCII report record")
                continue
            records.append(line)
            if line.startswith("HMPREEMPT BEGIN"):
                in_report = True
            elif line.startswith("HMPREEMPT END"):
                in_report = False
        elif in_report:
            errors.append("Unexpected output inside report")
        elif b"HMTEST" in raw:
            if records != expected or len(irq_counts) != 1:
                errors.append("Bootstrap regression report precedes completed scheduling report")
            post_report_seen = True
        elif b"HMEXCEPTION" in raw:
            errors.append("Fatal exception output")
    if records != expected:
        errors.append("Report differs from exact scheduling inventory")
    if len(irq_counts) != 1 or irq_counts[0] == 0:
        errors.append("Missing, duplicate or zero scheduling IRQ count")
    if returncode != 0:
        errors.append(f"Expected successful ARM exit 0, got {returncode}")
    if timed_out:
        errors.append("Scheduling image exceeded host deadline")
    if launch_error is not None:
        errors.append(f"QEMU launch failed: {launch_error}")
    post = validate_run(console, returncode, arch="aarch64", timed_out=timed_out,
                        launch_error=launch_error)
    if not post["passed"]:
        errors.append("Post-scheduling bootstrap regression inventory failed")
    return {"passed": not errors, "errors": errors, "observed_records": records,
            "expected_cases": list(CASES), "returncode": returncode,
            "timed_out": timed_out, "launch_error": launch_error,
            "scheduler_irqs": irq_counts[0] if len(irq_counts) == 1 else None,
            "post_scheduling": post, "diagnostic_irqs": DIAGNOSTIC_IRQS,
            "scope": "single-CPU ARM timer-driven scheduling; not SMP"}


def validate_trace(raw, expected):
    """Scheduling ERET can enter a new task; IRQ/ack/EOI must still pair exactly."""
    errors, counts, stage = [], {"irq": 0, "vector": 0, "ack": 0, "eoi": 0}, None
    synchronous, fault_kind = None, None
    if len(raw) > MAX_TRACE_BYTES or not raw.endswith(b"\n"):
        return {"passed": False, "errors": ["Oversized or incomplete trace"]}
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError:
        return {"passed": False, "errors": ["Non-ASCII trace"]}
    for original in lines:
        line = PREFIX.sub("", original)
        if "[IRQ]" in line:
            match = IRQ.fullmatch(line)
            if not match or match[1] != "0" or stage is not None or synchronous is not None:
                errors.append("Malformed or nested IRQ")
            counts["irq"] += 1
            stage = "entry"
        elif line.startswith("...from") and stage is not None:
            if stage != "entry" or line != "...from EL1 to EL1":
                errors.append("Unexpected IRQ exception level")
            stage = "from"
        elif line.startswith("...to") and stage is not None:
            match = VECTOR.fullmatch(line)
            if (stage != "from" or not match or int(match[1], 16) & 0x7ff != 0x280
                    or int(match[2], 16) & 0x3df != 0x3c5):
                errors.append("Invalid IRQ vector/state")
            counts["vector"] += 1
            stage = "vector"
        elif "gic_acknowledge_irq" in line:
            match = ACK.fullmatch(line)
            if not match or match.groups() != ("cpu", "0", "30") or stage != "vector":
                errors.append("Invalid timer acknowledgment")
            counts["ack"] += 1
            stage = "ack"
        elif "gic_cpu_write" in line:
            match = WRITE.fullmatch(line)
            if not match or match.groups()[:2] != ("cpu", "0"):
                errors.append("Invalid CPU interface write")
                continue
            offset, value = int(match[3], 16), int(match[4], 16)
            if offset == 0x10:
                if stage != "ack" or value != 30:
                    errors.append("EOI without matching timer acknowledgment")
                counts["eoi"] += 1
                stage = None
            elif offset in (0x24, 0x1000):
                errors.append("Unexpected alternate deactivation")
        elif line.startswith("Taking exception"):
            if stage is not None or synchronous is not None:
                errors.append("Nested exception in scheduling run")
            fault = re.fullmatch(r"Taking exception (3 \[Prefetch Abort\]|4 \[Data Abort\]) on CPU 0", line)
            if fault:
                synchronous = "entry"
                fault_kind = "0x21" if fault[1].startswith("3") else "0x25"
            elif line != "Taking exception 16 [Semihosting call] on CPU 0":
                errors.append("Unexpected exception in scheduling run")
        elif synchronous is not None and line.startswith("..."):
            if synchronous == "entry" and line == "...from EL1 to EL1":
                synchronous = "from"
            elif synchronous == "from" and re.fullmatch(
                    rf"\.\.\.with ESR {fault_kind}/0x[0-9a-fA-F]+", line):
                synchronous = "esr"
            elif synchronous == "esr" and re.fullmatch(r"\.\.\.with FAR 0x[0-9a-fA-F]+", line):
                synchronous = "far"
            elif synchronous == "far" and re.fullmatch(r"\.\.\.with ELR 0x[0-9a-fA-F]+", line):
                synchronous = "elr"
            elif synchronous == "elr":
                match = VECTOR.fullmatch(line)
                if (not match or int(match[1], 16) & 0x7ff != 0x200
                        or int(match[2], 16) & 0x3df != 0x3c5):
                    errors.append("Invalid synchronous vector/state")
                synchronous = "vector"
            else:
                errors.append("Malformed synchronous fault transaction")
        elif line.startswith("Exception return"):
            if stage is not None or not RETURN.fullmatch(line):
                errors.append("Invalid resume or resume before IRQ deactivation")
            if synchronous is not None:
                if synchronous != "vector":
                    errors.append("Incomplete synchronous fault before resume")
                synchronous = None
    if stage is not None or synchronous is not None or not expected or any(count != expected for count in counts.values()):
        errors.append("Incomplete IRQ transactions or count mismatch")
    return {"passed": not errors, "errors": errors, "counts": counts,
            "expected_interrupts": expected}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--memory", type=int, choices=(64, 128), default=128)
    parser.add_argument("--boundaries", action="store_true")
    args = parser.parse_args()
    kernel, output = args.kernel.resolve(), args.output.resolve()
    if not kernel.is_file():
        parser.error("kernel ELF is missing")
    if output.exists():
        parser.error("use a fresh output directory; preserve earlier evidence")
    output.mkdir(parents=True)
    command = qemu_command(kernel, "tcg", "aarch64")
    command[command.index("-m") + 1] = f"{args.memory}M"
    command += ["-d", "int", "-D", str(output / "exceptions.log"),
                "-trace", "enable=gic_acknowledge_irq",
                "-trace", "enable=gic_cpu_write"]
    (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    (output / "input.json").write_text(json.dumps({
        "kernel": str(kernel), "sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(),
        "arch": "aarch64", "accel": "tcg", "ram_mb": args.memory,
    }, indent=2) + "\n")
    started = datetime.now(timezone.utc).isoformat()
    start = time.monotonic()
    returncode, timed_out, launch_error = None, False, None
    with (output / "serial.log").open("wb") as log:
        try:
            result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=log,
                                    stderr=subprocess.STDOUT, timeout=TIMEOUT_SECONDS)
            returncode = result.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
        except OSError as error:
            launch_error = str(error)
    result = validate((output / "serial.log").read_bytes(), returncode,
                      timed_out, launch_error)
    trace_path = output / "exceptions.log"
    try:
        with trace_path.open("rb") as trace:
            raw = trace.read(MAX_TRACE_BYTES + 1)
        expected = (result["scheduler_irqs"] + DIAGNOSTIC_IRQS
                    if result["scheduler_irqs"] is not None else None)
        trace_result = validate_trace(raw, expected)
        trace_result["sha256"] = hashlib.sha256(raw).hexdigest()
    except OSError as error:
        trace_result = {"passed": False, "errors": [str(error)]}
    result["trace"] = trace_result
    result["passed"] &= trace_result["passed"]
    if args.boundaries:
        from check_preemption_boundaries import inspect
        boundary_result = inspect(kernel, raw if "raw" in locals() else b"", output)
        result["boundaries"] = boundary_result
        result["passed"] &= boundary_result["passed"]
    result.update(started_utc=started, elapsed_seconds=time.monotonic() - start,
                  timeout_seconds=TIMEOUT_SECONDS)
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
