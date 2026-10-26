#!/usr/bin/env python3
"""Validate pinned QEMU ARM IRQ/acknowledgment/EOI trace ordering.

This checks emulator observations, not native-hardware behavior. Trace event
formats were inspected in the pinned QEMU 7.2.22 binary; the cpu/vcpu distinction
was cross-checked in QEMU's v7.2.0 hw/intc/arm_gic.c (not Linux code).
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


MAX_TRACE_BYTES = 16 * 1024 * 1024
PREFIX = re.compile(r"^\d+@\d+\.\d{6}:")
IRQ = re.compile(r"Taking exception 5 \[IRQ\] on CPU (\d+)")
VECTOR = re.compile(r"\.\.\.to EL1 PC (0x[0-9a-fA-F]+) PSTATE (0x[0-9a-fA-F]+)")
ACK = re.compile(r"gic_acknowledge_irq (cpu|vcpu) (\d+) acknowledged irq (\d+)")
WRITE = re.compile(r"gic_cpu_write (cpu|vcpu) (\d+) iface write at (0x[0-9a-fA-F]{8}) (0x[0-9a-fA-F]{8})")
RETURN = re.compile(r"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x[0-9a-fA-F]+")


def validate_trace(raw, expected):
    """Require one complete CPU0 timer interrupt transaction per expected tick."""
    errors = []
    counts = {"irq_entries": 0, "irq_vectors": 0, "acknowledgments": 0,
              "eois": 0, "irq_returns": 0}
    if not isinstance(expected, int) or isinstance(expected, bool) or expected <= 0:
        errors.append("Expected interrupt count must be a positive integer")
    if len(raw) > MAX_TRACE_BYTES:
        errors.append("Trace exceeds the bounded input size")
        raw = b""
    try:
        content = raw.decode("ascii")
    except UnicodeDecodeError:
        errors.append("Trace contains non-ASCII data")
        content = ""
    if raw and not raw.endswith(b"\n"):
        errors.append("Trace ends with an unterminated record")
    stage = None
    from_el1 = False
    other_exception = False
    for number, original in enumerate(content.splitlines(), 1):
        line = PREFIX.sub("", original)

        def error(message):
            errors.append(f"Line {number}: {message}")

        if "[IRQ]" in line:
            match = IRQ.fullmatch(line)
            if not match:
                error("Malformed IRQ entry")
                continue
            counts["irq_entries"] += 1
            if int(match[1]) != 0:
                error("IRQ delivered to a CPU other than CPU0")
            if stage is not None:
                error("New IRQ before the preceding interrupt returned")
            if other_exception:
                error("IRQ nested inside an unreturned non-IRQ exception")
            stage, from_el1 = "entry", False
        elif line.startswith("Taking exception"):
            if stage is not None:
                error("Unexpected nested exception inside timer IRQ")
            else:
                other_exception = True
        elif stage is not None and line.startswith("...from"):
            if line != "...from EL1 to EL1" or stage != "entry" or from_el1:
                error("Unexpected IRQ source/target EL record")
            from_el1 = True
        elif stage is not None and line.startswith("...to"):
            match = VECTOR.fullmatch(line)
            if stage != "entry" or not from_el1 or not match:
                error("Malformed, duplicate or out-of-order IRQ vector")
                continue
            counts["irq_vectors"] += 1
            if int(match[1], 16) & 0x7ff != 0x280:
                error("IRQ did not enter the current-EL SPx IRQ vector")
            if int(match[2], 16) & 0x3df != 0x3c5:
                error("IRQ entry did not mask asynchronous exceptions at EL1h")
            stage = "vector"
        elif "gic_acknowledge_irq" in line:
            match = ACK.fullmatch(line)
            if not match:
                error("Malformed GIC acknowledgment")
                continue
            counts["acknowledgments"] += 1
            if match[1] != "cpu" or int(match[2]) != 0 or int(match[3]) != 30:
                error("Acknowledgment is not physical CPU0 timer ID30")
            if stage != "vector":
                error("Acknowledgment without exactly one preceding IRQ vector")
            stage = "ack"
        elif "gic_cpu_write" in line:
            match = WRITE.fullmatch(line)
            if not match:
                error("Malformed GIC CPU-interface write")
                continue
            if match[1] != "cpu" or int(match[2]) != 0:
                error("Write targets an unexpected CPU interface")
            offset, value = int(match[3], 16), int(match[4], 16)
            if offset == 0x10:
                counts["eois"] += 1
                if value != 30:
                    error("EOIR does not contain the complete expected ID30 acknowledgment")
                if stage != "ack":
                    error("EOI without exactly one preceding acknowledgment")
                stage = "eoi"
            elif offset in (0x24, 0x1000):
                error("Unexpected alternate EOI/deactivation operation")
        elif line.startswith("Exception return"):
            if stage is not None:
                if not RETURN.fullmatch(line) or stage != "eoi":
                    error("IRQ return without complete vector/acknowledgment/EOI sequence")
                counts["irq_returns"] += 1
                stage = None
            elif other_exception:
                other_exception = False
            else:
                error("Exception return without a preceding exception")
    if stage is not None:
        errors.append("Trace ended inside an incomplete IRQ transaction")
    for name, count in counts.items():
        if count != expected:
            errors.append(f"Expected {expected} {name}, observed {count}")
    return {"passed": not errors, "scope": "QEMU CPU0 physical timer IRQ/EOI trace",
            "expected_interrupts": expected, "counts": counts, "errors": errors}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--expected", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True,
                        help="new JSON result file; existing evidence is never overwritten")
    args = parser.parse_args()
    if args.expected <= 0:
        parser.error("--expected must be positive")
    if args.output.exists():
        parser.error("use a new output file; preserve prior results")
    try:
        with args.trace.open("rb") as trace:
            raw = trace.read(MAX_TRACE_BYTES + 1)
        result = validate_trace(raw, args.expected)
        result["trace_sha256"] = (hashlib.sha256(raw).hexdigest()
                                  if len(raw) <= MAX_TRACE_BYTES else None)
    except OSError as error:
        result = {"passed": False, "expected_interrupts": args.expected,
                  "errors": [f"Cannot read trace: {error}"]}
    result["trace"] = str(args.trace)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
