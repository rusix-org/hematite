#!/usr/bin/env python3
"""Run an upstream Asterinas ISO smoke test; this is not a hematite phase gate."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--iso", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    checkout, iso, output = (p.resolve() for p in
                             (args.checkout, args.iso, args.output))
    output.mkdir(parents=True, exist_ok=True)
    result_path = output / "result.json"
    if result_path.exists():
        parser.error("use a fresh output directory to preserve earlier results")
    if not iso.is_file():
        parser.error("ISO does not exist")
    env = dict(os.environ, MEM="1G", SMP="1", NETDEV="none",
               QEMU_DISPLAY="none", CONSOLE="hvc0", VIRTIOFS="off",
               VSOCK="off", VHOST="off", OVMF="on",
               BOOT_METHOD="grub-rescue-iso", BOOT_PROTOCOL="multiboot2",
               INITRAMFS="on", XFSTESTS_NEEDS_BLOCK_DEVICES="false")
    generated = subprocess.check_output(
        ["bash", "tools/qemu_args.sh", "normal"], cwd=checkout, env=env,
        text=True, timeout=10)
    command = ["qemu-system-x86_64", "-drive",
               f"file={iso},format=raw,index=2,media=cdrom",
               *shlex.split(generated), "-accel", "kvm"]
    (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    result = {"scope": "upstream Asterinas boot marker only", "passed": False,
              "timeout_seconds": 120, "timed_out": False, "returncode": None}
    with (output / "console.log").open("wb") as log:
        try:
            completed = subprocess.run(command, cwd=checkout, env=env,
                                       stdin=subprocess.DEVNULL, stdout=log,
                                       stderr=subprocess.STDOUT, timeout=120)
            result["returncode"] = completed.returncode
        except subprocess.TimeoutExpired:
            result["timed_out"] = True
        except OSError as exc:
            result["launch_error"] = str(exc)
    console = (output / "console.log").read_bytes()
    # Match complete lines, allowing terminal CRLF. Never infer success from
    # QEMU exit status alone or from a marker echoed as part of a command line.
    result["boot_markers"] = console.splitlines().count(b"Successfully booted.")
    # Upstream OSDK accepts normal poweroff (0) or its debug-exit success
    # encoding (0x10 << 1 | 1 == 33). QEMU launch failure 1 is not success.
    result["passed"] = (result["returncode"] in (0, 33)
                        and not result["timed_out"]
                        and result["boot_markers"] == 1)
    for name in ("qemu.log", "qemu-serial.log"):
        source = checkout / name
        if source.exists():
            (output / name).write_bytes(source.read_bytes())
    result_path.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result), flush=True)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
