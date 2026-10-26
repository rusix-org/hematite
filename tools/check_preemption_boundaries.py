"""Check actual IRQ return PCs against linker-visible transition windows."""

import json
import re
import subprocess

from check_timer_trace import PREFIX


NAMES = ("start", "yield", "complete")


def validate(raw, ranges):
    hits = {name: 0 for name in NAMES}
    errors, active, saw_pc = [], False, False
    if set(ranges) != set(NAMES) or any(lo >= hi for lo, hi in ranges.values()):
        return {"passed": False, "errors": ["Invalid boundary symbol ranges"]}
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError:
        return {"passed": False, "errors": ["Non-ASCII trace"]}
    for original in lines:
        line = PREFIX.sub("", original)
        if line.startswith("Taking exception"):
            if active and not saw_pc:
                errors.append("IRQ missing interrupted PC")
            active = line == "Taking exception 5 [IRQ] on CPU 0"
            saw_pc = False
        elif active and line.startswith("...with ELR"):
            match = re.fullmatch(r"\.\.\.with ELR (0x[0-9a-fA-F]+)", line)
            if not match or saw_pc:
                errors.append("Malformed or duplicate IRQ interrupted PC")
                continue
            saw_pc = True
            pc = int(match[1], 16)
            for name, (lo, hi) in ranges.items():
                if lo <= pc < hi:
                    hits[name] += 1
    if active and not saw_pc:
        errors.append("Final IRQ missing interrupted PC")
    if any(count == 0 for count in hits.values()):
        errors.append("No observed IRQ inside one or more transition windows")
    return {"passed": not errors, "errors": errors, "hits": hits,
            "ranges": {name: list(bounds) for name, bounds in ranges.items()}}


def inspect(kernel, raw, output):
    command = ["readelf", "-sW", str(kernel)]
    (output / "boundary-symbols.command.json").write_text(json.dumps(command) + "\n")
    try:
        result = subprocess.run(command, capture_output=True, timeout=5)
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"passed": False, "errors": [f"Cannot read boundary symbols: {error}"]}
    (output / "boundary-symbols.stdout.log").write_bytes(result.stdout)
    (output / "boundary-symbols.stderr.log").write_bytes(result.stderr)
    symbols = {}
    for line in result.stdout.decode("ascii", errors="replace").splitlines():
        fields = line.split()
        if len(fields) == 8 and fields[-1].startswith("arm_preempt_boundary_"):
            if fields[-1] in symbols:
                return {"passed": False, "errors": ["Duplicate boundary symbol"]}
            try:
                symbols[fields[-1]] = int(fields[1], 16)
            except ValueError:
                return {"passed": False, "errors": ["Invalid boundary symbol value"]}
    if result.returncode != 0:
        return {"passed": False, "errors": ["readelf failed"]}
    try:
        ranges = {name: (symbols[f"arm_preempt_boundary_{name}_begin"],
                         symbols[f"arm_preempt_boundary_{name}_end"]) for name in NAMES}
    except KeyError:
        return {"passed": False, "errors": ["Missing boundary symbols"]}
    return validate(raw, ranges)
