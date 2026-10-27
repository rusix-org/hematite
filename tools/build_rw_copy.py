#!/usr/bin/env python3
"""Reproduce and independently inspect the parse-only owned RW copy ELF."""
import argparse
import json
from pathlib import Path
import struct
import sys
import build_initramfs as evidence
from test_process import require, embedded_archive

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'crates/hm-core/fixtures/rw-copy.elf'
MANIFEST = FIXTURE.with_suffix('.manifest.json')
BASE = 0x100000000
GEOMETRY = ((0, 6, 4096), (1, 5, 4096), (2, 6, 8192), (4, 4, 4096), (6, 6, 4096))
SOURCES = ('crates/hm-core/fixtures/rw_copy.S', 'crates/hm-core/fixtures/rw_copy.ld',
           'tools/build_rw_copy.py', 'tools/build_initramfs.py', 'tools/test_process.py',
           'Dockerfile', 'rust-toolchain.toml')


def inspect(data):
    require(64 <= len(data) <= 128 * 1024 and data[:7] == b'\x7fELF\x02\x01\x01', 'Invalid bounded ELF64')
    require(struct.unpack_from('<HHI', data, 16) == (2, 183, 1), 'Not AArch64 static ELF')
    entry, phoff = struct.unpack_from('<QQ', data, 24)
    phsize, count = struct.unpack_from('<HH', data, 54)
    require(entry == BASE + 4096 and phsize == 56 and count == 5 and phoff >= 64 and phoff + 280 <= len(data), 'Wrong entry/LOAD inventory')
    segments, spans = [], []
    for index, (slot, expected_flags, size) in enumerate(GEOMETRY):
        kind, flags, offset, va, pa, filesz, memsz, align = struct.unpack_from('<IIQQQQQQ', data, phoff + index * 56)
        require((kind, flags, va, pa, filesz, memsz, align) == (1, expected_flags, BASE + slot * 4096, BASE + slot * 4096, size, size, 4096), 'Wrong exact LOAD geometry')
        require(offset % 4096 == 0 and offset >= phoff + 280 and offset + size <= len(data), 'Invalid LOAD file bounds')
        require(all(offset + size <= a or offset >= b for a, b in spans), 'Overlapping LOAD storage')
        spans.append((offset, offset + size))
        expected = {0: b'\x10' * 4096, 1: b'\0\0\0\x14' + b'\x21' * 4092,
                    2: b'\x32' * 4096 + b'\x43' * 4096, 4: b'\x54' * 4096, 6: b'\x76' * 4096}[slot]
        require(data[offset:offset + size] == expected, 'Fixture page bytes differ')
        segments.append(dict(flags=flags, offset=offset, va=va, filesz=filesz, memsz=memsz, align=align))
    return dict(entry=entry, pages=6, segments=segments)


def document(data):
    return dict(version=1, file=FIXTURE.name, sha256=evidence.sha(data), **inspect(data),
                sources={name: evidence.sha((ROOT / name).read_bytes()) for name in SOURCES})


def verify(fixture, manifest, output, kernel=None):
    data = fixture.read_bytes()
    require(json.loads(manifest.read_text()) == document(data), 'Fixture manifest/source mismatch')
    stdout, stderr = evidence.run(['readelf', '-hW', '-lW', '-sW', str(fixture)], output, 'fixture-readelf')
    require(not stderr and b'AArch64' in stdout and b'native_start' in stdout, 'Independent readelf inspection failed')
    if kernel is not None:
        embedded_archive(kernel.read_bytes(), data, [])
    return inspect(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--update', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists(): parser.error('use a fresh evidence directory')
    output.mkdir(parents=True)
    try:
        obj, elf = output / 'rw-copy.o', output / FIXTURE.name
        evidence.run(['aarch64-linux-gnu-as', '-o', str(obj), str(ROOT / 'crates/hm-core/fixtures/rw_copy.S')], output, 'assemble')
        evidence.run(['aarch64-linux-gnu-ld', '--build-id=none', '-z', 'max-page-size=4096', '-T', str(ROOT / 'crates/hm-core/fixtures/rw_copy.ld'), '-o', str(elf), str(obj)], output, 'link')
        raw = elf.read_bytes()
        manifest = (json.dumps(document(raw), indent=2) + '\n').encode()
        (output / MANIFEST.name).write_bytes(manifest)
        verify(elf, output / MANIFEST.name, output)
        for canonical, content in ((FIXTURE, raw), (MANIFEST, manifest)):
            if args.update: canonical.write_bytes(content)
            else: require(canonical.is_file() and canonical.read_bytes() == content, 'Canonical fixture does not reproduce')
        result = dict(passed=True, sha256=evidence.sha(raw), scope='parse-only six-page ELF; no execution')
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        result = dict(passed=False, error=str(error))
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
