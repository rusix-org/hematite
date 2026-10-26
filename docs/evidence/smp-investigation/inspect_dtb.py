"""Read-only inspection of the pinned QEMU-generated DTB; not a kernel parser."""
import hashlib
import json
from pathlib import Path
import struct

directory = Path(__file__).resolve().parent
data = (directory / "two-cpu.dtb").read_bytes()
header = struct.unpack_from(">10I", data)
magic, total, structure, strings, _, version, _, boot_cpu, string_size, structure_size = header
assert magic == 0xD00DFEED and total <= len(data) and version == 17
assert structure + structure_size <= total and strings + string_size <= total
names = data[strings:strings + string_size]
cursor, end, stack, selected = structure, structure + structure_size, [], {}
while cursor < end:
    token, = struct.unpack_from(">I", data, cursor)
    cursor += 4
    if token == 1:
        stop = data.index(0, cursor, end)
        stack.append(data[cursor:stop].decode("ascii"))
        cursor = (stop + 4) & ~3
    elif token == 2:
        stack.pop()
    elif token == 3:
        length, offset = struct.unpack_from(">II", data, cursor)
        cursor += 8
        assert cursor + length <= end and offset < len(names)
        prop = names[offset:names.index(0, offset)].decode("ascii")
        value = data[cursor:cursor + length]
        cursor = (cursor + length + 3) & ~3
        path = "/".join(stack) or "/"
        if path.startswith(("/cpus", "/psci", "/intc", "/timer")):
            entry = {"hex": value.hex()}
            if prop in ("compatible", "method", "device_type", "enable-method", "status"):
                entry["strings"] = value.rstrip(b"\0").decode("ascii").split("\0")
            elif length and length % 4 == 0:
                entry["cells"] = list(struct.unpack(f">{length // 4}I", value))
            selected.setdefault(path, {})[prop] = entry
    elif token == 4:
        pass
    elif token == 9:
        assert not stack
        break
    else:
        raise ValueError(f"Unexpected FDT token {token}")
else:
    raise ValueError("Missing FDT end token")
result = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
          "boot_cpuid_phys": boot_cpu, "nodes": selected}
(directory / "topology.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
