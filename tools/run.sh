#!/usr/bin/env bash
# Interactive bootstrap monitor; no disks, networking, or host devices.
set -euo pipefail

repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$repository_root"
kernel_path=target/monitor/x86_64-unknown-none/release/hematite
if [[ ! -f $kernel_path ]]; then
    printf 'Kernel image missing. Run make build first.\n' >&2
    exit 1
fi

printf 'Starting the bootstrap monitor (30-minute limit; Ctrl-C exits).\n'
exec timeout --foreground --kill-after=5s 1800s \
    qemu-system-x86_64 \
    -machine microvm,pic=off,pit=off,rtc=off,isa-serial=on \
    -accel tcg -cpu max -m 128M -smp 1 \
    -nodefaults -no-user-config -display none -monitor none -serial stdio \
    -no-reboot -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -kernel "$kernel_path"
