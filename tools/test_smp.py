#!/usr/bin/env python3
"""Validate per-CPU ARM scheduling evidence without guessing untagged returns.

GIC events carry CPU indices. IRQ vector lines are associated only with their
preceding CPU-tagged exception block; an overlapping incomplete block fails.
QEMU's untagged ERET lines are never used to infer CPU identity. See QEMU v7.2.0
 target/arm/helper.c and the GIC trace provenance in check_timer_trace.py.
"""

import argparse
from datetime import datetime, timezone
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import time

from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_preemption_negative import artifact, bounded_bytes

from check_timer_trace import ACK, IRQ, MAX_TRACE_BYTES, PREFIX, VECTOR, WRITE


EXCEPTION = re.compile(r"Taking exception ([0-9]+) \[([^]]+)\] on CPU ([0-9]+)")


def validate_irq_trace(raw, expected=None, *, peer_fault=False, allow_partial=False, vectors=None):
    """Count exact IRQ/vector/ack/EOI per CPU; allow only a stopped final tail."""
    errors = []
    counts = [{"irq": 0, "vector": 0, "ack": 0, "eoi": 0} for _ in range(2)]
    phases = [None, None]
    block = None
    events = []
    if not raw or len(raw) > MAX_TRACE_BYTES or not raw.endswith(b"\n"):
        return {"passed": False, "errors": ["Missing, oversized or incomplete trace"], "counts": counts}
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeError:
        return {"passed": False, "errors": ["Non-ASCII trace"], "counts": counts}
    for number, original in enumerate(lines, 1):
        line = PREFIX.sub("", original)

        def reject(message):
            errors.append(f"Line {number}: {message}")

        if line.startswith("Taking exception"):
            match = EXCEPTION.fullmatch(line)
            if not match or int(match[3]) not in (0, 1):
                reject("Malformed or unsupported CPU exception")
                continue
            kind, name, cpu = int(match[1]), match[2], int(match[3])
            events.append({"kind": kind, "name": name, "cpu": cpu})
            if block is not None:
                reject("Overlapping untagged exception blocks prevent CPU attribution")
                block = None
            if phases[cpu] is not None:
                reject("Exception nested before this CPU completed its IRQ")
            if kind == 5 and name == "IRQ":
                counts[cpu]["irq"] += 1
                phases[cpu] = "entry"
                block = cpu
            elif (kind, name) in ((11, "Hypervisor Call"), (16, "Semihosting call")):
                pass
            elif (kind, name) in ((3, "Prefetch Abort"), (4, "Data Abort")):
                if cpu == 1 and not peer_fault:
                    reject("Unexpected peer fault")
            else:
                reject("Unsupported exception kind")
        elif line.startswith("...from") and block is not None:
            if phases[block] != "entry" or line != "...from EL1 to EL1":
                reject("Invalid IRQ source/target level")
            phases[block] = "from"
        elif line.startswith("...to") and block is not None:
            match = VECTOR.fullmatch(line)
            if (phases[block] != "from" or not match
                    or int(match[1], 16) & 0x7ff != 0x280
                    or int(match[2], 16) & 0x3df != 0x3c5):
                reject("Invalid per-CPU IRQ vector/state")
            if vectors is not None and (len(vectors) != 2 or type(vectors[block]) is not int
                                        or vectors[block] % 2048 != 0 or not match
                                        or int(match[1], 16) != vectors[block] + 0x280):
                reject("IRQ did not enter this CPU's declared vector table")
            counts[block]["vector"] += 1
            phases[block] = "vector"
            block = None
        elif "gic_acknowledge_irq" in line:
            match = ACK.fullmatch(line)
            if not match or match[1] != "cpu" or int(match[2]) not in (0, 1) or match[3] != "30":
                reject("Malformed or non-timer acknowledgment")
                continue
            cpu = int(match[2])
            if phases[cpu] != "vector":
                reject("Acknowledgment has no matching vector on this CPU")
            counts[cpu]["ack"] += 1
            phases[cpu] = "ack"
        elif "gic_cpu_write" in line:
            match = WRITE.fullmatch(line)
            if not match or match[1] != "cpu" or int(match[2]) not in (0, 1):
                reject("Malformed or unsupported CPU interface write")
                continue
            cpu, offset, value = int(match[2]), int(match[3], 16), int(match[4], 16)
            if offset == 0x10:
                if phases[cpu] != "ack" or value != 30:
                    reject("EOI has no matching PPI30 acknowledgment on this CPU")
                counts[cpu]["eoi"] += 1
                phases[cpu] = None
            elif offset in (0x24, 0x1000):
                reject("Unexpected alternate deactivation")
    for cpu, observed in enumerate(counts):
        if allow_partial:
            values = list(observed.values())
            if not (values == sorted(values, reverse=True) and values[0] - values[-1] <= 1):
                errors.append(f"CPU{cpu}: invalid partial transaction counts")
        elif phases[cpu] is not None or len(set(observed.values())) != 1:
            errors.append(f"CPU{cpu}: incomplete IRQ transaction")
        if expected is not None:
            if (len(expected) != 2 or type(expected[cpu]) is not int or expected[cpu] < 0
                    or any(value != expected[cpu] for value in observed.values())):
                errors.append(f"CPU{cpu}: counts do not match reported local IRQ total")
    if block is not None and not allow_partial:
        errors.append("Trace ended inside an untagged IRQ entry block")
    return {"passed": not errors, "errors": errors, "counts": counts,
            "events": events, "unfinished_cpu_phases": phases,
            "expected_interrupts": expected}


def peer_overflow_trace(raw, guard, vector, probe=None):
    """Attribute FAR to the tagged peer abort, never to a racing CPU0 fault."""
    details, errors = [], []
    stage = None
    peer_events = []
    fault_count = 0
    probe_valid = (isinstance(probe, (tuple, list)) and len(probe) == 2
                   and all(type(value) is int for value in probe)
                   and 0 <= probe[0] < probe[1] <= 1 << 64)
    if not probe_valid:
        errors.append("Missing or invalid Rust overflow-probe symbol range")
    fault_pcs = []
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeError:
        return {"passed": False, "errors": ["Non-ASCII fault trace"]}
    for original in lines:
        line = PREFIX.sub("", original)
        event = EXCEPTION.fullmatch(line)
        if event:
            if stage is not None:
                errors.append("Overlapping abort block prevents peer FAR attribution")
                stage = None
            if event[3] == "1":
                peer_events.append((int(event[1]), event[2]))
                if event.groups()[:2] == ("4", "Data Abort"):
                    fault_count += 1
                    stage = "from"
            continue
        if stage is None or not line.startswith("..."):
            continue
        if stage == "from" and line == "...from EL1 to EL1":
            stage = "esr"
        elif stage == "esr" and line == "...with ESR 0x25/0x96000007":
            stage = "far"
        elif stage == "far" and (match := re.fullmatch(r"\.\.\.with FAR (0x[0-9a-fA-F]+)", line)):
            address = int(match[1], 16)
            details.append(address)
            if type(guard) is not int or not guard <= address < guard + 4096:
                errors.append("Peer FAR does not fall in its worker0 guard")
            stage = "elr"
        elif stage == "elr" and (match := re.fullmatch(r"\.\.\.with ELR (0x[0-9a-fA-F]+)", line)):
            pc = int(match[1], 16)
            fault_pcs.append(pc)
            if not probe_valid or not probe[0] <= pc < probe[1]:
                errors.append("Peer fault ELR is outside the actual Rust overflow probe")
            stage = "vector"
        elif stage == "vector" and (match := VECTOR.fullmatch(line)):
            if (type(vector) is not int or int(match[1], 16) != vector + 0x200
                    or int(match[2], 16) & 0x3df != 0x3c5):
                errors.append("Peer abort did not enter its fatal synchronous vector")
            stage = None
        else:
            errors.append("Malformed peer translation-abort transaction")
            stage = None
    if stage is not None or fault_count != 1 or len(details) != 1:
        errors.append("Missing, duplicate or incomplete peer guard fault")
    if (len(peer_events) < 2 or peer_events[-2:] != [(4, "Data Abort"), (16, "Semihosting call")]
            or any(event != (5, "IRQ") for event in peer_events[:-2])):
        errors.append("Peer must terminate directly after its sole guard fault")
    return {"passed": not errors, "errors": errors, "peer_fault_addresses": details,
            "peer_events": peer_events, "fault_pcs": fault_pcs, "probe_range": probe}


CASES = (
    ("progress_contexts", 2, 2), ("mutex", 4, 4), ("mixed_truncation", 4, 4),
    ("event_before", 1, 1), ("event_during", 1, 1), ("event_after_broadcast", 1, 4),
    ("event_unrelated", 1, 2), ("limits_reuse", 0, 0),
)
NUMBER = r"(0|[1-9][0-9]{0,19})"
SESSION = re.compile(
    rf"HMSCHED SESSION generation={NUMBER} cpu0=0 cpu1=1 "
    rf"completed0={NUMBER} completed1={NUMBER} irq0={NUMBER} irq1={NUMBER} "
    rf"off=1 checks=255 root={NUMBER} sp={NUMBER} worker0={NUMBER} worker1={NUMBER}")


def serial_records(console):
    records, positions, errors = [], [], []
    inside = False
    lines = console.split(b"\n")
    for index, raw in enumerate(lines):
        raw = raw.removesuffix(b"\r")
        if b"HMSCHED" in raw:
            if index == len(lines) - 1:
                errors.append("Unterminated SMP record")
            try:
                value = raw.decode("ascii")
            except UnicodeError:
                errors.append("Non-ASCII SMP record")
                continue
            records.append(value)
            positions.append(index)
            if value.startswith("HMSCHED BEGIN"):
                inside = True
            elif value.startswith("HMSCHED END"):
                inside = False
        elif inside and (raw or index < len(lines) - 1):
            errors.append("Unexpected output inside SMP report")
    return records, positions, errors


def stack_contains(symbols, prefix, sp, usable=65536):
    guard, top = symbols.get(prefix + "_guard"), symbols.get(prefix + "_top")
    return (type(guard) is int and type(top) is int and guard % 4096 == 0
            and top - guard == usable + 4096 and type(sp) is int and sp % 16 == 0
            and guard + 4096 <= sp < top)


def validate(console, trace, execution, symbols, negative=None):
    found, positions, errors = serial_records(console)
    checks = {
        "launched": execution.get("launch_error") is None,
        "no_foreign_report": b"HMSMP" not in console and b"HMPREEMPT" not in console,
    }
    if negative is not None:
        checks.update({"strict_serial_records": not errors,
                       "no_normal_report": b"HMTEST" not in console})
        timeout = negative in ("no-peer-timer", "all-blocked")
        checks["expected_termination"] = (execution.get("timed_out") is True and execution.get("returncode") == -9
                                          if timeout else not execution.get("timed_out", True)
                                          and execution.get("returncode") == 5)
        irq_trace = validate_irq_trace(trace, peer_fault=negative == "peer-overflow", allow_partial=True,
                                       vectors=(symbols.get("arm_vectors"), symbols.get("arm_secondary_vectors")))
        checks["per_cpu_irq_trace"] = irq_trace["passed"]
        events = irq_trace.get("events", [])
        hvc_counts = [sum(event["kind"] == 11 and event["cpu"] == cpu for event in events) for cpu in (0, 1)]
        checks["admitted_without_peer_shutdown"] = hvc_counts[0] > 0 and hvc_counts[1] == 0
        fault = None
        if negative == "no-peer-timer":
            match = re.fullmatch(rf"HMSCHED STARTED cpu=1 sp={NUMBER}", found[1]) if len(found) == 2 else None
            checks["negative_inventory"] = (len(found) == 2 and found[0] == "HMSCHED EXPECT NO-PEER-TIMER"
                                             and match is not None
                                             and stack_contains(symbols, "__secondary_thread0", int(match[1])))
            checks["withheld_only_peer_timer"] = irq_trace["counts"][0]["irq"] > 0 and irq_trace["counts"][1]["irq"] == 0
        elif negative == "all-blocked":
            checks["negative_inventory"] = found == ["HMSCHED EXPECT ALL-BLOCKED", "HMSCHED BLOCKED cpu0=1 cpu1=1"]
        elif negative == "peer-overflow":
            checks["negative_inventory"] = found == ["HMSCHED EXPECT PEER-OVERFLOW"]
            fault = peer_overflow_trace(trace, symbols.get("__secondary_thread0_guard"),
                                        symbols.get("arm_secondary_vectors"),
                                        symbols.get("arm_smp_large_frame_range"))
            checks["peer_guard_fault"] = fault["passed"]
            checks["coordinator_not_faulting"] = all(event["kind"] in (5, 11) for event in events if event["cpu"] == 0)
            exits = [event for event in events if event["kind"] == 16]
            checks["terminal_peer_exit"] = (len(exits) == 1 and exits[0]["cpu"] == 1
                                            and bool(events) and events[-1] == exits[0]
                                            and trace.endswith(b"...handling as semihosting call 0x20\n"))
        else:
            checks["known_negative"] = False
        if timeout:
            checks["retained_without_fault_or_exit"] = all(event["kind"] in (5, 11) for event in events)
            checks["no_fatal_serial"] = b"HMEXCEPTION" not in console
        return {"passed": all(checks.values()), "checks": checks, "errors": errors,
                "observed_records": found, "trace": irq_trace, "hvc_counts": hvc_counts,
                "peer_fault": fault, "scope": "bounded negative SMP ownership profile"}
    checks.update({
        "within_deadline": not execution.get("timed_out", True),
        "successful_exit": execution.get("returncode") == 0,
        "no_fatal_serial": b"HMEXCEPTION" not in console,
        "strict_serial_records": not errors,
        "complete_inventory": (len(found) == 2 + 2 * len(CASES)
                               and found[0] == f"HMSCHED BEGIN {len(CASES)}"
                               and found[-1] == f"HMSCHED END {len(CASES)}"),
    })
    sessions = []
    admitted = checks["complete_inventory"]
    root = symbols.get("arm_secondary_root")
    checks["secondary_root_symbol"] = type(root) is int and root % 4096 == 0
    if admitted:
        for index, (name, completed0, completed1) in enumerate(CASES):
            match = SESSION.fullmatch(found[index * 2 + 1])
            if not match or found[index * 2 + 2] != f"HMSCHED PASS {name}":
                admitted = False
                continue
            generation, count0, count1, irq0, irq1, reported_root, sp, worker0, worker1 = map(int, match.groups())
            item = {"case": name, "generation": generation, "completed": [count0, count1],
                    "irqs": [irq0, irq1], "root": reported_root, "sp": sp,
                    "workers": [worker0, worker1]}
            sessions.append(item)
            good = (generation == index + 1 and (count0, count1) == (completed0, completed1)
                    and reported_root == root and stack_contains(symbols, "__secondary_stack", sp))
            for cpu, count, worker in ((0, count0, worker0), (1, count1, worker1)):
                prefix = "__secondary_thread0" if cpu else "__thread0"
                good &= worker == 0 if count == 0 else stack_contains(symbols, prefix, worker)
            if name in ("progress_contexts", "mixed_truncation"):
                good &= irq0 > 0 and irq1 > 0
            elif name == "mutex":
                good &= irq0 > 0
            elif name == "limits_reuse":
                good &= irq0 == 0 and irq1 == 0
            admitted &= good
    checks["exact_sessions_and_elf_addresses"] = admitted and checks["secondary_root_symbol"]
    expected = (sum(item["irqs"][0] for item in sessions) + 16,
                sum(item["irqs"][1] for item in sessions)) if len(sessions) == len(CASES) else None
    irq_trace = validate_irq_trace(trace, expected,
                                   vectors=(symbols.get("arm_vectors"), symbols.get("arm_secondary_vectors")))
    checks["per_cpu_irq_trace"] = irq_trace["passed"] and expected is not None
    events = irq_trace.get("events", [])
    hvc_counts = [sum(event["kind"] == 11 and event["cpu"] == cpu for event in events) for cpu in (0, 1)]
    checks["observed_repeated_peer_shutdown"] = hvc_counts[0] > 0 and hvc_counts[1] == len(CASES)
    exits = [event for event in events if event["kind"] == 16]
    checks["terminal_coordinator_exit"] = (len(exits) == 1 and exits[0]["cpu"] == 0
                                           and bool(events) and events[-1] == exits[0]
                                           and trace.endswith(b"...handling as semihosting call 0x20\n"))
    baseline_positions = [i for i, line in enumerate(console.split(b"\n")) if b"HMTEST" in line]
    checks["baseline_after_join"] = bool(baseline_positions and positions
                                         and min(baseline_positions) > max(positions))
    post = validate_run(console, execution.get("returncode"), arch="aarch64",
                        timed_out=execution.get("timed_out", True),
                        launch_error=execution.get("launch_error"))
    checks["post_shutdown_baseline"] = post["passed"]
    return {"passed": all(checks.values()), "checks": checks, "errors": errors,
            "observed_records": found, "sessions": sessions, "trace": irq_trace,
            "post_shutdown": post, "hvc_counts": hvc_counts,
            "scope": "two-CPU timer-driven callbacks and post-join regression on pinned TCG"}


SYMBOLS = ("arm_vectors", "arm_secondary_vectors", "arm_preempt_start",
           "arm_secondary_root", "__secondary_stack_guard", "__secondary_stack_top") + tuple(
    f"__{prefix}thread{worker}_{boundary}" for prefix in ("", "secondary_")
    for worker in range(4) for boundary in ("guard", "top"))


def read_symbols(kernel, output, *, require_overflow=False):
    command = ["readelf", "-sW", str(kernel)]
    write_json(output / "symbols.command.json", command)
    result = {"returncode": None, "error": None}
    found = {name: [] for name in SYMBOLS}
    probe_symbols = []
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
                elif len(fields) >= 8 and fields[-1] == "arm_smp_large_frame":
                    probe_symbols.append({"value": int(fields[1], 16), "size": int(fields[2], 0)})
    except (OSError, subprocess.TimeoutExpired, UnicodeError, ValueError) as error:
        result["error"] = str(error)
    result["values"] = found
    result["overflow_probe_symbols"] = probe_symbols
    probe_range = None
    if len(probe_symbols) == 1:
        start, size = probe_symbols[0]["value"], probe_symbols[0]["size"]
        if size > 0 and 0 <= start < start + size <= 1 << 64:
            probe_range = [start, start + size]
    result["overflow_probe_range"] = probe_range
    result["overflow_probe_required"] = require_overflow
    result["passed"] = (result["error"] is None and result["returncode"] == 0
                        and all(len(values) == 1 for values in found.values())
                        and (not require_overflow or probe_range is not None))
    write_json(output / "symbols.result.json", result)
    if not result["passed"]:
        return {}
    values = {name: matches[0] for name, matches in found.items()}
    if probe_range is not None:
        values["arm_smp_large_frame_range"] = probe_range
    return values


def run_profile(kernel, output, *, memory=128, negative=None):
    output.mkdir()
    command = qemu_command(kernel, "tcg", "aarch64")
    command[command.index("-smp") + 1] = "2"
    command[command.index("-accel") + 1] = "tcg,thread=multi"
    command[command.index("-m") + 1] = f"{memory}M"
    command += ["-d", "int", "-D", str(output / "exceptions.log"),
                "-trace", "enable=gic_acknowledge_irq", "-trace", "enable=gic_cpu_write"]
    write_json(output / "command.json", command)
    write_json(output / "input.json", {"kernel": str(kernel), "sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(),
                                      "arch": "aarch64", "accel": "tcg,thread=multi", "cpus": 2,
                                      "ram_mb": memory, "negative": negative})
    symbols = read_symbols(kernel, output, require_overflow=negative == "peer-overflow")
    deadline = 2 if negative in ("no-peer-timer", "all-blocked") else 10 if negative == "peer-overflow" else 20
    started = time.monotonic()
    execution = {"started_utc": datetime.now(timezone.utc).isoformat(), "timeout_seconds": deadline,
                 "returncode": None, "timed_out": False, "launch_error": None}
    with (output / "serial.log").open("wb") as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=deadline)
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
    result["checks"]["symbols_extracted"] = (all(name in symbols for name in SYMBOLS)
                                               and (negative != "peer-overflow"
                                                    or "arm_smp_large_frame_range" in symbols))
    result["passed"] = all(result["checks"].values())
    result.update(execution, artifacts=artifacts, symbols=symbols)
    write_json(output / "result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--no-peer-timer-kernel", type=Path, required=True)
    parser.add_argument("--peer-overflow-kernel", type=Path, required=True)
    parser.add_argument("--all-blocked-kernel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    kernels = (args.kernel, args.no_peer_timer_kernel, args.peer_overflow_kernel, args.all_blocked_kernel)
    if not all(kernel.is_file() for kernel in kernels):
        parser.error("all four kernel ELFs are required")
    output = args.output.resolve()
    if output.exists():
        parser.error("use a fresh evidence directory; preserve earlier results")
    output.mkdir(parents=True)
    profiles = {}
    for memory in (128, 64):
        profiles[f"smp-{memory}m"] = run_profile(args.kernel.resolve(), output / f"smp-{memory}m", memory=memory)
    for name, kernel in (("no-peer-timer", args.no_peer_timer_kernel),
                         ("peer-overflow", args.peer_overflow_kernel), ("all-blocked", args.all_blocked_kernel)):
        profiles[name] = run_profile(kernel.resolve(), output / name, negative=name)
    result = {"passed": all(profile["passed"] for profile in profiles.values()),
              "profiles": {name: profile["passed"] for name, profile in profiles.items()}}
    write_json(output / "result.json", result)
    print(result)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
