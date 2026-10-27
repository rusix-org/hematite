#!/usr/bin/env python3
"""Reproduce the ordinary 25-case task fixtures and two exact GNU-checked archives."""
import argparse
import json
from pathlib import Path
import struct
import sys

import build_initramfs as cpio
import build_tasks as base
from test_process import require, embedded_archive
from test_runtime import verify_layout

ROOT = base.ROOT
CANONICAL = ROOT / 'crates/hm-core/fixtures/task-cases'
FIXTURES = ROOT / 'crates/hm-core/fixtures'
CASES = ('supervisor', 'copy_inputs', 'wait_poll_ready', 'wait_budget_child', 'rights_console',
         'foreign_authority', 'stale_reuse', 'malformed_outer', 'malformed_descriptor', 'copy_faults',
         'child_fault', 'held_capacity', 'child_frame_cap', 'group_frame_cap', 'allocation_rollback',
         'parent_exit', 'parent_fault', 'parent_budget', 'parent_call_limit', 'parent_cancel_ready',
         'parent_cancel_waiting', 'limit_spawn', 'limit_wait', 'limit_cancel', 'drop_waiting')
ORDINARY_ROOTS = tuple(name for case in CASES if case != 'group_frame_cap'
                       for name in (('rights_console_policy', 'rights_console_wait_only') if case == 'rights_console' else (case,)))
APPS = ORDINARY_ROOTS + ('worker', 'loop', 'large_root', 'large_worker', 'large_loop')
EXTRA = ('raw_svc', 'overwrite_svc', 'fault', 'fault_self', 'descriptor_storage', 'input_storage', 'expected_child_frames')
ARCHIVE = 'task-cases-initramfs.cpio'
LARGE_ARCHIVE = 'task-cases-large-initramfs.cpio'


def path_for(name):
    return 'demo/loop' if name.endswith('loop') else 'bin/group_frame_cap' if name == 'large_root' else 'bin/worker' if name == 'large_worker' else 'bin/' + name


def read_at(data, segments, va, length, flags):
    candidates = [data[s['offset'] + va - s['va']:s['offset'] + va - s['va'] + length]
                  for s in segments if s['flags'] == flags and s['va'] <= va and va + length <= s['va'] + s['filesz']]
    require(len(candidates) == 1, 'Symbol lacks unique whole file-backed mapping')
    return candidates[0]


def inspect(data, text):
    result = base.inspect(data, text)
    symbols = result['symbols']
    for line in text.splitlines():
        words = line.split()
        if len(words) == 8 and words[-1] in EXTRA:
            require(words[-1] not in symbols, 'Duplicate task-case symbol')
            symbols[words[-1]] = int(words[1], 16)
    require(set(EXTRA) <= set(symbols), 'Missing dedicated fixture symbols')
    def insn(va): return struct.unpack('<I', read_at(data, result['segments'], va, 4, 5))[0]
    require(all(insn(symbols[name]) == 0xd4000001 for name in ('raw_svc', 'overwrite_svc')), 'Not actual SVC0')
    require(insn(symbols['fault']) == 0xf900013f and insn(symbols['fault'] - 4) == 0xd51b421f,
            'Fault must clear NZCV and store XZR through X9')
    adr = insn(symbols['fault'] - 8)
    require(adr & 0x9f00001f == 0x10000009, 'Fault target must be ADR X9')
    offset = ((adr >> 5) & 0x7ffff) << 2 | ((adr >> 29) & 3)
    if offset & (1 << 20): offset -= 1 << 21
    require(symbols['fault'] - 8 + offset == result['entry'], 'Fault does not target own RX entry')
    for name, length in (('descriptor_storage', 80), ('input_storage', 272)):
        require(sum(s['flags'] == 6 and s['va'] <= symbols[name] and symbols[name] + length <= s['va'] + s['memsz']
                    for s in result['segments']) == 1, 'Fixture storage not wholly owned RW/NX')
    result['child_frames'] = struct.unpack('<Q', read_at(data, result['segments'], symbols['expected_child_frames'], 8, 4))[0]
    return result


def entries(data, large=False):
    names = ('large_root', 'large_worker') if large else ORDINARY_ROOTS + ('worker',)
    return [('bin', 0o40755, b'')] + [(path_for(name), 0o100755, data[name]) for name in names] + [
        ('demo', 0o40755, b''), ('demo/loop', 0o100755, data['large_loop' if large else 'loop'])]


def archive_doc(raw, items, manifest, name):
    return dict(cpio.manifest(raw, items), archive=name, fixtures_manifest_sha256=cpio.sha(manifest))


def layout_constants(layout):
    fields, task = layout['result']['fields'], layout['result']['task_fields']
    return ('// Generated from the independently compiled layout witness; do not edit.\n' +
            ''.join(f'pub const {name}: usize = {value};\n' for name, value in (
                ('PROCESS_BYTES', fields['process_bytes']), ('PROCESS_PAGES', fields['box_backing_pages']),
                ('IMAGE_BYTES', task['image_bytes']), ('IMAGE_PAGES', task['image_pages']),
                ('AUTHORITY_BYTES', task['authority_bytes']), ('AUTHORITY_PAGES', task['authority_pages']),
                ('METADATA_PAGES', task['metadata_pages'])))).encode()


def verify(manifest_path, archive_path, large_path, layout_path, output, kernel=None):
    _, layout = verify_layout(layout_path)
    document = json.loads(manifest_path.read_text())
    require(set(document) == {'version', 'layout_sha256', 'fixtures'} and document['version'] == 1 and
            document['layout_sha256'] == cpio.sha(layout_path.read_bytes()) and set(document['fixtures']) == set(APPS),
            'Wrong task case manifest or layout witness')
    data, apps = {}, document['fixtures']
    for name in APPS:
        item = apps[name]
        elf = manifest_path.parent / (name + '.elf')
        require(elf.is_file() and 64 <= elf.stat().st_size <= 1024 * 1024, 'Missing/oversized task fixture')
        data[name] = elf.read_bytes()
        text, errors = cpio.run(['readelf', '-sW', str(elf)], output, name + '-symbols')
        require(not errors, 'readelf diagnostics')
        actual = inspect(data[name], text.decode('ascii'))
        require(item == dict(actual, file=name + '.elf', path=path_for(name), sha256=cpio.sha(data[name])),
                'Task fixture metadata does not match actual ELF')
        cpio.run(['aarch64-linux-gnu-objdump', '-d', str(elf)], output, name + '-disassembly')
    k = layout['result']['fields']['box_backing_pages']
    metadata = layout['result']['task_fields']['metadata_pages']
    charge = apps['worker']['pages'] + 29 + k
    require(apps['child_frame_cap']['child_frames'] == charge and 1 < charge <= 128, 'Wrong literal child frame charge')
    large_root = apps['large_root']['pages'] + 29 + k
    large_child = apps['large_worker']['pages'] + 29 + k
    require(apps['large_root']['pages'] == 228 and apps['large_worker']['pages'] == 96 and
            large_child <= 128 and metadata + large_root <= 512 and metadata + large_root + large_child <= 512 <
            metadata + large_root + 2 * large_child, 'Large image geometry does not prove logical cap failure')
    require((manifest_path.parent / 'layout.rs').read_bytes() == layout_constants(layout), 'Driver layout constants mismatch')
    reports, spans = {}, []
    kernel_bytes = kernel.read_bytes() if kernel is not None else None
    for large, path in ((False, archive_path), (True, large_path)):
        require(path.is_file() and path.stat().st_size <= cpio.MAX_BYTES, 'Missing/oversized task archive')
        raw, items = path.read_bytes(), entries(data, large)
        name = LARGE_ARCHIVE if large else ARCHIVE
        require(raw == cpio.newc(items) and len(items) == (5 if large else 29), 'Archive exact inventory/bytes mismatch')
        require(path.with_suffix('.manifest.json').read_bytes() == base.json_bytes(archive_doc(raw, items, manifest_path.read_bytes(), name)),
                'Archive manifest mismatch')
        cpio.inspect_with_gnu(path, items, output, 'large-archive' if large else 'archive')
        reports[name] = {'sha256': cpio.sha(raw), 'bytes': len(raw), 'entries': len(items)}
        if kernel is not None:
            start = embedded_archive(kernel_bytes, raw, [])
            reports[name]['kernel_file_offset'] = start
            spans.append((start, start + len(raw)))
    if kernel_bytes is not None:
        require(len(spans) == 2 and (spans[0][1] <= spans[1][0] or spans[1][1] <= spans[0][0]), 'Overlapping task archives')
        phoff = struct.unpack_from('<Q', kernel_bytes, 32)[0]
        count = struct.unpack_from('<H', kernel_bytes, 56)[0]
        for payload in set(data.values()):
            for index in range(count):
                kind, _, offset, _, _, filesz = struct.unpack_from('<IIQQQQ', kernel_bytes, phoff + index * 56)
                if kind != 1: continue
                position = kernel_bytes.find(payload, offset, offset + filesz)
                while position >= 0:
                    require(any(left <= position and position + len(payload) <= right for left, right in spans),
                            'Raw dedicated fixture outside both canonical archives')
                    position = kernel_bytes.find(payload, position + 1, offset + filesz)
    return apps, reports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--layout', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--update', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists(): parser.error('preserve evidence; use fresh output')
    output.mkdir(parents=True)
    sources = ['crates/hm-core/fixtures/task_cases.' + suffix for suffix in ('rs', 'S', 'ld')] + [
        'crates/hm-kernel/src/task_wire.rs', 'crates/hm-kernel/src/startup_input.rs', 'tools/build_task_cases.py',
        'tools/build_tasks.py', 'tools/build_initramfs.py', 'tools/test_runtime.py', 'tools/test_process.py']
    (output / 'input.json').write_bytes(base.json_bytes({'sources': {name: cpio.sha((ROOT / name).read_bytes()) for name in sources},
        'layout': str(args.layout.resolve()), 'update': args.update}))
    try:
        _, layout = verify_layout(args.layout.resolve())
        k = layout['result']['fields']['box_backing_pages']
        data, apps = {}, {}
        # Build catalog first; child cap uses independently inspected ELF geometry.
        for name in ('worker', 'loop', 'large_worker', 'large_loop') + ORDINARY_ROOTS + ('large_root',):
            case = 100 if name.endswith('worker') else 101 if name.endswith('loop') else 13 if name == 'large_root' else 4 if name == 'rights_console_policy' else 25 if name == 'rights_console_wait_only' else CASES.index(name)
            charge = apps['worker']['pages'] + 29 + k if 'worker' in apps else 1
            elf = output / (name + '.elf')
            command = ['env', f'HM_TASK_CASE={case}', f'HM_TASK_CHILD_FRAMES={charge}', 'rustc', '--edition=2024',
                       '--crate-name', 'task_cases', '--crate-type=bin', '--target', 'aarch64-unknown-none', '-C', 'panic=abort',
                       '-C', 'opt-level=2', '-C', 'strip=debuginfo', '-C', 'codegen-units=1', '-C', 'relocation-model=static',
                       '-C', f'link-arg=-T{FIXTURES / "task_cases.ld"}', '-C', 'link-arg=--build-id=none',
                       '-C', 'link-arg=-z', '-C', 'link-arg=max-page-size=4096', '-C', 'link-arg=--no-dynamic-linker']
            if name in ('large_root', 'large_worker'):
                pages = 228 if name == 'large_root' else 96
                command += ['-C', f'link-arg=--defsym=fixture_bss_end={0x100000000 + pages * 4096}']
            command += [str(FIXTURES / 'task_cases.rs'), '-o', str(elf)]
            cpio.run(command, output, name + '-build')
            data[name] = elf.read_bytes()
            text, errors = cpio.run(['readelf', '-sW', str(elf)], output, name + '-build-symbols')
            require(not errors, 'readelf diagnostics')
            apps[name] = dict(inspect(data[name], text.decode('ascii')), file=name + '.elf', path=path_for(name), sha256=cpio.sha(data[name]))
        manifest = base.json_bytes({'version': 1, 'layout_sha256': cpio.sha(args.layout.read_bytes()), 'fixtures': apps})
        files = {'manifest.json': manifest, 'layout.rs': layout_constants(layout)}
        files.update({name + '.elf': data[name] for name in APPS})
        for name, value in files.items(): (output / name).write_bytes(value)
        archives = {}
        for large, name in ((False, ARCHIVE), (True, LARGE_ARCHIVE)):
            items = entries(data, large)
            raw = cpio.newc(items)
            archives[name] = raw
            archives[str(Path(name).with_suffix('.manifest.json'))] = base.json_bytes(archive_doc(raw, items, manifest, name))
        for name, value in archives.items(): (output / name).write_bytes(value)
        _, reports = verify(output / 'manifest.json', output / ARCHIVE, output / LARGE_ARCHIVE, args.layout.resolve(), output)
        for folder, values in ((CANONICAL, files), (FIXTURES, archives)):
            for name, value in values.items():
                path = folder / name
                if args.update:
                    path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(value)
                else: require(path.is_file() and path.read_bytes() == value, 'Canonical task artifact changed: ' + str(path))
        result = {'passed': True, 'archives': reports, 'scope': 'ordinary task artifacts; no execution or negative hooks'}
    except (OSError, ValueError, KeyError, TypeError, UnicodeError, struct.error, StopIteration) as error:
        result = {'passed': False, 'error': str(error)}
    (output / 'result.json').write_bytes(base.json_bytes(result)); print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__': sys.exit(main())
