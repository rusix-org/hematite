#!/usr/bin/env python3
"""Compare actual safe ELF plans with independently decoded linker output."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

from check_frame_types import run

PAGE = 4096
START = 0x1_0000_0000
END = START + 1024 * 1024
NUMBER = r"(0|[1-9][0-9]*)"
IMAGE = re.compile(rf"HMELF IMAGE entry={NUMBER} pages={NUMBER} segments={NUMBER}")
SEGMENT = re.compile(rf"HMELF LOAD va={NUMBER} filesz={NUMBER} memsz={NUMBER} flags={NUMBER} "
                     rf"page_start={NUMBER} page_end={NUMBER} file_hash={NUMBER}")
LOAD = re.compile(r"\s*LOAD\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+"
                  r"(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+"
                  r"([RWE ]+)\s+(0x[0-9a-fA-F]+)\s*")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fingerprint(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value


def check_reference(header, program, data):
    checks = {
        "elf64": bool(re.search(r"^\s*Class:\s+ELF64\s*$", header, re.M)),
        "little_endian": bool(re.search(r"^\s*Data:\s+2's complement, little endian\s*$", header, re.M)),
        "static_executable": bool(re.search(r"^\s*Type:\s+EXEC \(Executable file\)\s*$", header, re.M)),
        "aarch64": bool(re.search(r"^\s*Machine:\s+AArch64\s*$", header, re.M)),
    }
    entry_match = re.search(r"^\s*Entry point address:\s+(0x[0-9a-fA-F]+)\s*$", header, re.M)
    count_match = re.search(r"^\s*Number of program headers:\s+([0-9]+)\s*$", header, re.M)
    entry = int(entry_match[1], 16) if entry_match else None
    checks["entry_present"] = entry is not None and START <= entry < END and entry % 4 == 0
    segments = []
    for line in program.splitlines():
        match = LOAD.fullmatch(line)
        if match:
            offset, address, physical, file_size, memory_size = map(lambda v: int(v, 16), match.groups()[:5])
            flag_text, alignment = match[6].replace(" ", ""), int(match[7], 16)
            flags = sum({"R": 4, "W": 2, "E": 1}[value] for value in flag_text)
            file_bytes = data[offset:offset + file_size]
            segments.append({"offset": offset, "va": address, "physical": physical,
                             "filesz": file_size, "memsz": memory_size, "flags": flags,
                             "align": alignment, "page_start": address // PAGE * PAGE,
                             "page_end": (address + memory_size + PAGE - 1) // PAGE * PAGE,
                             "file_hash": fingerprint(file_bytes)})
    checks["three_explicit_loads"] = len(segments) == 3 and count_match is not None and int(count_match[1]) == 3
    checks["rx_r_rw"] = [item["flags"] for item in segments] == [5, 4, 6]
    checks["nonempty_bss"] = len(segments) == 3 and segments[-1]["memsz"] > segments[-1]["filesz"]
    checks["load_bounds"] = bool(segments) and all(
        0 < item["filesz"] <= item["memsz"]
        and item["offset"] + item["filesz"] <= len(data)
        and START <= item["page_start"] < item["page_end"] <= END
        and item["offset"] % PAGE == item["va"] % PAGE
        and item["align"] == PAGE for item in segments)
    checks["separate_pages"] = all(a["page_end"] <= b["page_start"] for a, b in zip(segments, segments[1:]))
    return {"passed": all(checks.values()), "checks": checks, "entry": entry,
            "segments": segments,
            "pages": sum((item["page_end"] - item["page_start"]) // PAGE for item in segments)}


def check_report(text, reference, fixture_size):
    lines = text.splitlines()
    checks = {"exact_line_count": len(lines) == 5, "complete_final_line": text.endswith("\n")}
    image = IMAGE.fullmatch(lines[0]) if lines else None
    checks["image_matches_readelf"] = bool(image and list(map(int, image.groups())) ==
                                           [reference["entry"], reference["pages"], 3])
    segments = []
    for line in lines[1:4]:
        match = SEGMENT.fullmatch(line)
        if match:
            segments.append(dict(zip(("va", "filesz", "memsz", "flags", "page_start", "page_end", "file_hash"),
                                     map(int, match.groups()))))
    expected = [{key: item[key] for key in ("va", "filesz", "memsz", "flags", "page_start", "page_end", "file_hash")}
                for item in reference["segments"]]
    checks["segments_match_readelf_and_file_bytes"] = segments == expected and len(segments) == 3
    checks["strict_test_inventory"] = bool(lines and lines[-1] ==
        f"HMELF HOST PASS cases=5 mutations=32 truncations={fixture_size + 1} random=4096")
    return {"passed": all(checks.values()), "checks": checks, "segments": segments}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file() or output.exists():
        parser.error("require the host hm-kernel library and a fresh evidence directory")
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    fixture_source = root / "crates/hm-core/fixtures/native-elf.rs"
    fixture_linker = root / "crates/hm-core/fixtures/native-elf.ld"
    canonical_fixture = root / "crates/hm-core/fixtures/native-elf"
    host_source = root / "tools/test_elf_host.rs"
    fixture, executable = output / "native-elf", output / "elf-host"
    source_paths = [fixture_source, fixture_linker, host_source, Path(__file__).resolve(), root / "crates/hm-kernel/src/elf.rs"]
    (output / "input.json").write_text(json.dumps({"library": str(library), "library_sha256": digest(library),
        "sources": {str(path.relative_to(root)): digest(path) for path in source_paths}}, indent=2) + "\n")
    results = {}
    command = ["rustc", "--edition=2024", "--crate-name", "native_elf", "--crate-type=bin",
               "--target", "aarch64-unknown-none", "-C", "panic=abort", "-C", "opt-level=2",
               "-C", "codegen-units=1", "-C", "relocation-model=static",
               "-C", f"link-arg=-T{fixture_linker}", "-C", "link-arg=--build-id=none",
               "-C", "link-arg=-z", "-C", "link-arg=max-page-size=4096",
               "-C", "link-arg=--no-dynamic-linker", str(fixture_source), "-o", str(fixture)]
    code, _ = run(command, output, "build-fixture")
    good_fixture = code == 0 and fixture.is_file() and 64 <= fixture.stat().st_size <= 1024 * 1024
    results["fixture"] = {"passed": good_fixture, "returncode": code,
                          "sha256": digest(fixture) if good_fixture else None}
    canonical_present = canonical_fixture.is_file()
    results["canonical_fixture"] = {
        "passed": good_fixture and canonical_present and fixture.read_bytes() == canonical_fixture.read_bytes(),
        "path": str(canonical_fixture.relative_to(root)),
        "sha256": digest(canonical_fixture) if canonical_present else None,
        "regenerated_sha256": digest(fixture) if good_fixture else None,
        "comparison": "byte-for-byte equality with regenerated pinned-toolchain fixture",
    }
    reference = None
    if good_fixture:
        readelf_okay = True
        for mode, name in [("-hW", "header"), ("-lW", "program"), ("-sW", "symbols")]:
            code, stderr = run(["readelf", mode, str(fixture)], output, f"readelf-{name}")
            readelf_okay &= code == 0 and not stderr
        results["readelf"] = {"passed": readelf_okay}
        if readelf_okay:
            reference = check_reference((output / "readelf-header.stdout.log").read_text(),
                                        (output / "readelf-program.stdout.log").read_text(), fixture.read_bytes())
            results["reference"] = reference
    if reference is not None and reference["passed"]:
        command = ["rustc", "--edition=2024", "--target", "x86_64-unknown-linux-gnu",
                   "-C", "panic=abort", "-C", "opt-level=2", "--extern", f"hm_kernel={library}",
                   "-L", f"dependency={library.parent / 'deps'}", str(host_source), "-o", str(executable)]
        code, _ = run(command, output, "build-host")
        results["host_compile"] = {"passed": code == 0, "returncode": code}
        if code == 0:
            code, stderr = run([str(executable), str(fixture)], output, "run-host")
            stdout = (output / "run-host.stdout.log").read_text() if (output / "run-host.stdout.log").is_file() else ""
            checked = check_report(stdout, reference, fixture.stat().st_size)
            results["execution"] = {"passed": code == 0 and not stderr and checked["passed"],
                                    "returncode": code, "stderr_empty": not stderr,
                                    "executable_sha256": digest(executable), "report": checked}
    result = {"passed": len(results) == 6 and all(item["passed"] for item in results.values()),
              "scope": "actual safe ELF admission and readelf comparison; no ELF loading or EL0 execution",
              "checks": results}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
