#!/usr/bin/env python3
"""Build kernel-independent startup probes and their deterministic test archive."""
import argparse
import json
from pathlib import Path
import struct
import sys

import build_initramfs as cpio
from test_process import require, embedded_archive

ROOT = Path(__file__).resolve().parents[1]
CANONICAL = ROOT / 'crates/hm-core/fixtures/startup-input'
ARCHIVE = ROOT / 'crates/hm-core/fixtures/startup-input-initramfs.cpio'
APPS = ('absent', 'empty', 'one', 'bytes255', 'binary256', 'payload_write', 'payload_execute', 'input_loop')
PATHS = tuple('bin/' + name for name in APPS)
SYMBOLS = ('native_start', 'write_svc', 'newline_svc', 'exit_svc', 'fault_write', 'branch_payload', 'loop_start', 'loop_end', 'newline')
INPUTS = dict(zip(APPS, (
    {'present': False, 'hex': ''}, {'present': True, 'hex': ''},
    {'present': True, 'hex': '41'}, {'present': True, 'hex': bytes(range(255)).hex()},
    {'present': True, 'hex': bytes(range(256)).hex()}, {'present': True, 'hex': '57'},
    {'present': True, 'hex': 'c0035fd6'}, {'present': True, 'hex': '4c'},
)))


def inspect(data, symbol_text):
    require(64 <= len(data) <= 1024 * 1024 and data[:7] == b'\x7fELF\x02\x01\x01', 'Invalid bounded ELF64')
    require(struct.unpack_from('<HHI', data, 16) == (2, 183, 1), 'Not static AArch64 ELF')
    entry, phoff = struct.unpack_from('<QQ', data, 24)
    phsize, count = struct.unpack_from('<HH', data, 54)
    require(phsize == 56 and 1 <= count <= 16 and phoff <= len(data) and count * phsize <= len(data) - phoff, 'Invalid ELF program table')
    segments = []
    for i in range(count):
        kind, flags, offset, va, _, filesz, memsz, align = struct.unpack_from('<IIQQQQQQ', data, phoff + i * phsize)
        require(kind == 1 and flags in (4, 5, 6) and 0 < memsz and filesz <= memsz and
                offset <= len(data) and filesz <= len(data) - offset and align == 4096 and offset % 4096 == va % 4096,
                'Invalid app LOAD')
        start, end = va & ~4095, (va + memsz + 4095) & ~4095
        require(0x100000000 <= start < end <= 0x100100000 and
                (not segments or segments[-1]['end'] <= start), 'Invalid app address interval')
        segments.append(dict(flags=flags, offset=offset, va=va, filesz=filesz, memsz=memsz, align=align, start=start, end=end))
    symbols = {}
    probe_end = None
    for line in symbol_text.splitlines():
        columns = line.split()
        if len(columns) != 8 or not columns[0].endswith(':'):
            continue
        name = columns[-1]
        require(name not in ('kernel_text', 'kernel_data') and not
                (columns[6] == 'ABS' and 0x40000000 <= int(columns[1], 16) < 0x41000000), 'App contains kernel absolute symbol')
        require(columns[6] != 'UND', 'App contains undefined named symbol')
        if name == 'probe_end':
            require(probe_end is None, 'Duplicate probe_end symbol')
            probe_end = int(columns[1], 16)
        if name in SYMBOLS:
            require(name not in symbols, 'Duplicate app symbol')
            symbols[name] = int(columns[1], 16)
    require(set(SYMBOLS) == set(symbols) and entry == symbols['native_start'] and entry % 4 == 0, 'Missing app entry/symbols')
    def instruction_at(va):
        for segment in segments:
            if segment['flags'] == 5 and segment['va'] <= va and va + 4 <= segment['va'] + segment['filesz']:
                return struct.unpack_from('<I', data, segment['offset'] + va - segment['va'])[0]
        raise ValueError('Probe instruction outside RX file storage: ' + str(va))
    def instruction(name):
        return instruction_at(symbols[name])
    require(probe_end is not None, 'Missing probe_end symbol')
    require([instruction_at(probe_end + offset) for offset in (0, 4, 8)] ==
            [0xd2800b09, 0xf2a00209, 0xf2c00029], 'Probe X9 target is not fixed STARTUP+88')
    for name in ('write_svc', 'newline_svc', 'exit_svc'):
        require(instruction(name) == 0xd4000001, 'Probe syscall is not SVC0')
    require(instruction('loop_start') == 0x14000000 and symbols['loop_end'] == symbols['loop_start'] + 4, 'Probe loop is not named self branch')
    require(instruction('fault_write') == 0x3900013f, 'Probe write is not STRB WZR,[X9]')
    require(instruction('branch_payload') == 0xd61f0120, 'Probe execute is not BR X9')
    for name in ('fault_write', 'branch_payload'):
        va = symbols[name] - 4
        matches = [struct.unpack_from('<I', data, s['offset'] + va - s['va'])[0]
                   for s in segments if s['flags'] == 5 and s['va'] <= va and va + 4 <= s['va'] + s['filesz']]
        require(matches == [0xd51b421f], 'Probe does not clear NZCV before fault')
    va = symbols['newline']
    matches = [data[s['offset'] + va - s['va']:s['offset'] + va - s['va'] + 1]
               for s in segments if s['flags'] == 4 and s['va'] <= va < s['va'] + s['filesz']]
    require(matches == [b'\n'], 'Invalid immutable newline')
    instruction('native_start')
    return {'entry': entry, 'pages': sum((s['end'] - s['start']) // 4096 for s in segments),
            'symbols': symbols, 'segments': segments}


def archive_entries(data):
    return [('bin', 0o40755, b'')] + [(path, 0o100755, data[name]) for name, path in zip(APPS, PATHS)]


def archive_manifest(raw, entries, manifest_bytes):
    result = cpio.manifest(raw, entries)
    result['archive'] = ARCHIVE.name
    result['probes_manifest_sha256'] = cpio.sha(manifest_bytes)
    return result


def json_bytes(value):
    return (json.dumps(value, indent=2) + '\n').encode()


def verify(manifest_path, archive_path, output, kernel=None):
    version, stderr = cpio.run(['cpio', '--version'], output, 'apps-cpio-version')
    require(not stderr and b'GNU cpio' in version, 'GNU cpio required')
    document = json.loads(manifest_path.read_text())
    require(set(document) == {'version', 'probes'} and document['version'] == 1 and type(document['version']) is int and
            set(document['probes']) == set(APPS), 'Wrong app manifest inventory')
    data = {}
    for name, path in zip(APPS, PATHS):
        item = document['probes'][name]
        require(set(item) == {'file', 'path', 'sha256', 'entry', 'pages', 'symbols', 'segments', 'input'} and
                item['file'] == name + '.elf' and item['path'] == path, 'Wrong app manifest fields')
        elf = manifest_path.parent / item['file']
        require(elf.is_file() and 64 <= elf.stat().st_size <= 1024 * 1024, 'Missing or oversized app')
        data[name] = elf.read_bytes()
        symbols, stderr = cpio.run(['readelf', '-sW', str(elf)], output, name + '-symbols')
        require(not stderr, 'readelf emitted diagnostics')
        actual = inspect(data[name], symbols.decode('ascii'))
        require(item == dict(actual, file=name + '.elf', path=path, sha256=cpio.sha(data[name]), input=INPUTS[name]), 'App metadata/hash differs from ELF')
        cpio.run(['aarch64-linux-gnu-objdump', '-d', str(elf)], output, name + '-disassembly')
    entries = archive_entries(data)
    require(archive_path.is_file() and archive_path.stat().st_size <= cpio.MAX_BYTES, 'Missing/oversized app archive')
    raw = archive_path.read_bytes()
    require(raw == cpio.newc(entries), 'App archive does not reproduce exact inputs')
    require(archive_path.with_suffix('.manifest.json').read_bytes() ==
            json_bytes(archive_manifest(raw, entries, manifest_path.read_bytes())), 'App archive manifest mismatch')
    cpio.inspect_with_gnu(archive_path, entries, output, 'apps-archive')
    report = {'bytes': len(raw), 'sha256': cpio.sha(raw), 'entries': 9, 'files': 8}
    if kernel is not None:
        report['kernel_file_offset'] = embedded_archive(kernel.read_bytes(), raw, list(data.values()))
    return document['probes'], report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--update', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a fresh evidence directory')
    output.mkdir(parents=True)
    sources = ['crates/hm-core/fixtures/' + name for name in ('startup-input.rs', 'startup-input.S', 'native-elf.ld')]
    sources += ['crates/hm-kernel/src/startup_input.rs', 'tools/build_startup_probes.py', 'tools/build_initramfs.py', 'tools/test_process.py', 'docs/design/0027-native-startup-input.md']
    (output / 'input.json').write_bytes(json_bytes({'sources': {p: cpio.sha((ROOT / p).read_bytes()) for p in sources}, 'update': args.update}))
    try:
        data, apps = {}, {}
        for name, path in zip(APPS, PATHS):
            elf = output / (name + '.elf')
            command = ['rustc', '--edition=2024', '--crate-name', 'startup_probes', '--crate-type=bin', '--target', 'aarch64-unknown-none',
                       '--cfg', f'probe="{name}"', '-C', 'panic=abort', '-C', 'opt-level=2', '-C', 'strip=debuginfo', '-C', 'codegen-units=1',
                       '-C', 'relocation-model=static', '-C', f'link-arg=-T{ROOT / "crates/hm-core/fixtures/native-elf.ld"}',
                       '-C', 'link-arg=--build-id=none', '-C', 'link-arg=-z', '-C', 'link-arg=max-page-size=4096',
                       '-C', 'link-arg=--no-dynamic-linker', str(ROOT / 'crates/hm-core/fixtures/startup-input.rs'), '-o', str(elf)]
            cpio.run(command, output, name + '-build')
            data[name] = elf.read_bytes()
            symbols, stderr = cpio.run(['readelf', '-sW', str(elf)], output, name + '-build-symbols')
            require(not stderr, 'readelf emitted diagnostics')
            apps[name] = dict(inspect(data[name], symbols.decode('ascii')), file=name + '.elf', path=path, sha256=cpio.sha(data[name]), input=INPUTS[name])
        manifest = json_bytes({'version': 1, 'probes': apps})
        (output / 'manifest.json').write_bytes(manifest)
        entries = archive_entries(data)
        archive = cpio.newc(entries)
        (output / ARCHIVE.name).write_bytes(archive)
        archive_doc = json_bytes(archive_manifest(archive, entries, manifest))
        (output / ARCHIVE.with_suffix('.manifest.json').name).write_bytes(archive_doc)
        _, report = verify(output / 'manifest.json', output / ARCHIVE.name, output)
        files = [(CANONICAL / (name + '.elf'), data[name]) for name in APPS]
        files += [(CANONICAL / 'manifest.json', manifest), (ARCHIVE, archive), (ARCHIVE.with_suffix('.manifest.json'), archive_doc)]
        for path, content in files:
            if args.update:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            else:
                require(path.is_file() and path.read_bytes() == content, 'Canonical app artifact does not reproduce: ' + str(path))
        result = {'passed': True, 'archive': report, 'scope': 'kernel-independent startup probes, GNU exact archive listing/extraction; no execution'}
    except (OSError, ValueError, UnicodeError, KeyError, TypeError, struct.error, StopIteration) as error:
        result = {'passed': False, 'error': str(error)}
    (output / 'result.json').write_bytes(json_bytes(result))
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
