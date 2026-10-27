#!/usr/bin/env python3
"""Bind the fixed idle leaf, real kernel, native hello and validation sources."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import build_apps
import build_initramfs as evidence
from test_launcher import SYMBOLS as BASE_SYMBOLS, load_bytes, fixup_table
from test_process import read_symbols, require

ROOT = Path(__file__).resolve().parents[1]
SYMBOLS = BASE_SYMBOLS + ('arm_root', 'arm_idle_wfi', 'arm_idle_resume', 'arm_idle_end', '__exception_stack_top')
LEAF = (0xd503207f, 0xd5033fdf, 0xd53be020, 0xd65f03c0)
SOURCES = ('crates/hm-core/src/idle.rs', 'crates/hm-core/src/idle-aarch64.S',
           'crates/hm-core/src/process.rs', 'crates/hm-core/src/process-aarch64.S',
           'crates/hm-core/src/boot-aarch64.S', 'crates/hm-core/src/aarch64.rs',
           'crates/hm-core/src/main.rs', 'crates/hm-core/src/launcher.rs',
           'crates/hm-core/src/user_space.rs', 'crates/hm-core/src/user-space-aarch64.S',
           'crates/hm-core/src/frames.rs', 'crates/hm-core/src/process_layout.rs',
           'crates/hm-core/Cargo.toml', 'crates/hm-kernel/src/native.rs',
           'tools/build_idle.py', 'tools/test_idle.py', 'Dockerfile', 'rust-toolchain.toml')


def inspect(kernel, symbols):
    require(len(kernel) >= 64 and kernel[:7] == b'\x7fELF\x02\x01\x01' and struct.unpack_from('<HHI',kernel,16) == (2,183,1), 'Invalid static AArch64 ELF')
    start, resume, end = (symbols[n] for n in ('arm_idle_wfi', 'arm_idle_resume', 'arm_idle_end'))
    require(start % 4 == 0 and resume == start + 4 and end == start + 16, 'Wrong isolated idle leaf geometry')
    require(load_bytes(kernel, start, 16, 5) == struct.pack('<4I', *LEAF), 'Wrong WFI/ISB/CNTPCT/RET opcodes')
    # Inspect direct incoming control flow, independently of symbol spelling.
    phoff = struct.unpack_from('<Q', kernel, 32)[0]
    phsize, count = struct.unpack_from('<HH', kernel, 54)
    require(phsize == 56 and 0 < count <= 32 and phoff + count * 56 <= len(kernel), 'Bad kernel program headers')
    calls = []
    for i in range(count):
        kind, flags, offset, va, _, filesz, _, _ = struct.unpack_from('<IIQQQQQQ', kernel, phoff + i * 56)
        if kind != 1 or not flags & 1:
            continue
        require(flags == 5 and offset + filesz <= len(kernel) and va % 4 == 0, 'Invalid executable kernel segment')
        for j in range(0, filesz - 3, 4):
            op = struct.unpack_from('<I', kernel, offset + j)[0]
            target = None
            if op & 0x7c000000 == 0x14000000:
                imm = op & 0x3ffffff
                target = va + j + ((imm - (1 << 26) if imm & (1 << 25) else imm) << 2)
                if target == start:
                    calls.append(va + j)
            elif op & 0xff000010 == 0x54000000 or op & 0x7e000000 == 0x34000000:
                imm = (op >> 5) & 0x7ffff
                target = va + j + ((imm - (1 << 19) if imm & (1 << 18) else imm) << 2)
            elif op & 0x7e000000 == 0x36000000:
                imm = (op >> 5) & 0x3fff
                target = va + j + ((imm - (1 << 14) if imm & (1 << 13) else imm) << 2)
            require(target is None or not resume <= target < end, 'Alternate direct branch into idle continuation')
    require(calls, 'No direct caller of isolated WFI entry')
    return {'entry': start, 'resume': resume, 'end': end, 'direct_callers': calls,
            'leaf_sha256': evidence.sha(load_bytes(kernel, start, 16, 5))}


def source_paths():
    """Complete workspace source/build inputs plus the loaded validator closure."""
    sources = set(SOURCES) | {'Cargo.toml', 'Cargo.lock', '.cargo/config.toml', 'tools/dev.sh'}
    for path in (ROOT / 'crates').rglob('*'):
        if path.is_file() and 'target' not in path.relative_to(ROOT).parts and (
                path.suffix in ('.rs', '.S', '.s', '.ld') or path.name in ('Cargo.toml', 'Cargo.lock')):
            sources.add(str(path.relative_to(ROOT)))
    for module in tuple(sys.modules.values()):
        filename = getattr(module, '__file__', None)
        if filename:
            path = Path(filename).resolve()
            if path.parent == ROOT / 'tools' and path.suffix == '.py': sources.add(str(path.relative_to(ROOT)))
    return sources


def verify(kernel, apps_path, archive_path, output):
    require(kernel.is_file() and 64 <= kernel.stat().st_size <= 32 * 1024 * 1024, 'Missing/bounded kernel')
    symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
    raw = kernel.read_bytes()
    leaf = inspect(raw, symbols)
    fixups = fixup_table(raw, symbols)
    apps, archive = build_apps.verify(apps_path, archive_path, output, kernel)
    evidence.run(['aarch64-linux-gnu-objdump', '-d', str(kernel)], output, 'kernel-disassembly')
    sources = source_paths()
    binding = dict(kernel_sha256=evidence.sha(raw), leaf=leaf, archive=archive,
                   apps_sha256=evidence.sha(apps_path.read_bytes()),
                   sources={p: evidence.sha((ROOT / p).read_bytes()) for p in sorted(sources)})
    (output / 'binding.json').write_text(json.dumps(binding, indent=2) + '\n')
    return symbols, fixups, apps, binding


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('kernel', 'apps', 'archive', 'output'): parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists(): parser.error('use a fresh output directory')
    args.output.mkdir(parents=True)
    try:
        _, _, _, binding = verify(args.kernel.resolve(), args.apps.resolve(), args.archive.resolve(), args.output)
        result = dict(passed=True, binding=binding, scope='artifact inspection only; no execution')
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        result = dict(passed=False, error=str(error))
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__': sys.exit(main())
