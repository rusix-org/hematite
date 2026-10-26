#!/usr/bin/env bash
# Interactive bootstrap monitor; no disks, networking, or host devices.
set -euo pipefail

repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$repository_root"
kernel_arch=${1:-aarch64}
if (( $# > 1 )); then
    printf 'Usage: %s [aarch64|x86_64]\n' "$0" >&2
    exit 2
fi
case "$kernel_arch" in
    aarch64)
        qemu_command=(qemu-system-aarch64
            -machine virt,virtualization=off,secure=off,gic-version=2
            -semihosting-config enable=on,target=native)
        ;;
    x86_64)
        qemu_command=(qemu-system-x86_64
            -machine microvm,pic=off,pit=off,rtc=off,isa-serial=on
            -device isa-debug-exit,iobase=0xf4,iosize=0x04)
        ;;
    *)
        printf 'Unsupported architecture: %s; use aarch64 or x86_64.\n' "$kernel_arch" >&2
        exit 2
        ;;
esac
kernel_path="target/${kernel_arch}/monitor/${kernel_arch}-unknown-none/release/hematite"
if [[ ! -f $kernel_path ]]; then
    printf 'Kernel image missing. Run make build ARCH=%s first.\n' "$kernel_arch" >&2
    exit 1
fi

printf 'Starting the bootstrap monitor (30-minute limit; Ctrl-C exits).\n'
exec timeout --foreground --kill-after=5s 1800s \
    "${qemu_command[@]}" \
    -accel tcg -cpu max -m 128M -smp 1 \
    -nodefaults -no-user-config -display none -monitor none -serial stdio \
    -no-reboot -kernel "$kernel_path"
