#!/usr/bin/env python3
"""Reproduce dedicated task-negative ELFs; artifact checks alone prove no hook execution."""
import argparse
import json
from pathlib import Path
import struct
import sys

import build_apps
import build_initramfs as cpio
import build_task_cases
import build_tasks
from test_process import require, embedded_archive
from test_runtime import verify_layout

ROOT = build_tasks.ROOT
FIXTURES = ROOT / 'crates/hm-core/fixtures'
CANONICAL = FIXTURES / 'task-negative-cases'
ARCHIVE = FIXTURES / 'task-negative-initramfs.cpio'
PROFILES = ('final_budget_spawn', 'final_budget_park', 'platform_reply', 'platform_park',
            'abandon_dispatch', 'wake_install_failure', 'capability_exhaustion', 'slot_exhaustion',
            'child_retire_ready', 'child_retire_waiting')
APPS = PROFILES + ('worker', 'legacy_hello', 'loop')
LEGACY = FIXTURES / 'apps/hello.elf'


def path_for(name): return 'demo/loop' if name == 'loop' else 'bin/' + name


def inspect(data, symbols, name):
    if name == 'legacy_hello':
        result = build_apps.inspect(data, symbols)
        require('message' in result['symbols'], 'Missing actual legacy hello output symbol')
        return result
    result = build_tasks.inspect(data, symbols, 'worker' if name == 'worker' else None)
    case = 100 if name == 'worker' else 101 if name == 'loop' else PROFILES.index(name)
    entries = [line.split() for line in symbols.splitlines() if len(line.split()) == 8 and line.split()[-1] == 'task_negative_main']
    require(len(entries) == 1, 'Missing/duplicate negative Rust entry')
    target = int(entries[0][1], 16)
    result['symbols']['task_negative_main'] = target
    raw = build_task_cases.read_at(data, result['segments'], result['entry'], 8, 5)
    mov, branch = struct.unpack('<II', raw)
    require(mov == 0xd2800001 | (case << 5) and branch & 0xfc000000 == 0x94000000,
            'Negative entry does not select fixed case and BL Rust entry')
    displacement = branch & 0x03ffffff
    if displacement & 0x02000000: displacement -= 0x04000000
    require(result['entry'] + 4 + displacement * 4 == target, 'Wrong negative Rust entry target')
    build_task_cases.read_at(data, result['segments'], target, 4, 5)
    return result


def entries(data):
    return [('bin', 0o40755, b'')] + [(path_for(name), 0o100755, data[name]) for name in PROFILES + ('worker', 'legacy_hello')] + [
        ('demo', 0o40755, b''), ('demo/loop', 0o100755, data['loop'])]


def archive_doc(raw, items, manifest):
    return dict(cpio.manifest(raw, items), archive=ARCHIVE.name, fixtures_manifest_sha256=cpio.sha(manifest))


def constants(layout, apps):
    loop = apps['loop']['symbols']
    return build_task_cases.layout_constants(layout) + (
        f'pub const LOOP_START: u64 = {loop["loop_start"]};\n'
        f'pub const LOOP_END: u64 = {loop["loop_end"]};\n').encode()


def verify(manifest_path, archive_path, layout_path, output, kernel=None):
    _, layout = verify_layout(layout_path)
    negative_layout = layout['result'].get('feature') == 'task-negative-tests'
    if kernel is not None:
        require(negative_layout, 'Ordinary bootstrap layout is not negative guest evidence')
        require('crates/hm-core/src/process_task_negative.rs' in layout['sources'], 'Negative layout omits actual hook source')
        for name in ('library', 'sizes'):
            argv = json.loads((layout_path.parent / (name + '.command.json')).read_text())['argv']
            require(any(argv[i:i+2] == ['--cfg', 'feature="task-negative-tests"'] for i in range(len(argv)-1)),
                    'Negative layout marker lacks actual compiler feature')
    document = json.loads(manifest_path.read_text())
    require(set(document) == {'version', 'layout_sha256', 'fixtures'} and document['version'] == 1 and
            document['layout_sha256'] == cpio.sha(layout_path.read_bytes()) and set(document['fixtures']) == set(APPS),
            'Wrong negative fixture inventory/layout witness')
    apps, data = document['fixtures'], {}
    for name in APPS:
        elf = manifest_path.parent / (name + '.elf')
        require(elf.is_file() and 64 <= elf.stat().st_size <= 1024 * 1024, 'Missing/oversized negative ELF')
        data[name] = elf.read_bytes()
        symbols, errors = cpio.run(['readelf', '-sW', str(elf)], output, name + '-symbols')
        require(not errors, 'readelf diagnostics')
        actual = inspect(data[name], symbols.decode('ascii'), name)
        require(apps[name] == dict(actual, file=name + '.elf', path=path_for(name), sha256=cpio.sha(data[name])), 'Negative ELF metadata/hash differs')
        cpio.run(['aarch64-linux-gnu-objdump', '-d', str(elf)], output, name + '-disassembly')
    require(data['legacy_hello'] == LEGACY.read_bytes(), 'Legacy hello differs from canonical ordinary executable')
    k = layout['result']['fields']['box_backing_pages']
    metadata = layout['result']['task_fields']['metadata_pages']
    require(apps['worker']['pages'] + 29 + k <= 128 and apps['loop']['pages'] + 29 + k <= 128 and
            all(metadata + apps[name]['pages'] + 29 + k + apps['worker']['pages'] + 29 + k + apps['loop']['pages'] + 29 + k <= 512
                for name in PROFILES), 'Negative fixture geometry cannot hold expected owners')
    require((manifest_path.parent / 'layout.rs').read_bytes() == constants(layout, apps), 'Negative layout/poll constants differ')
    require(archive_path.is_file() and archive_path.stat().st_size <= cpio.MAX_BYTES, 'Missing/oversized negative archive')
    raw, items = archive_path.read_bytes(), entries(data)
    require(len(items) == 15 and raw == cpio.newc(items), 'Negative archive inventory/bytes differ')
    require(archive_path.with_suffix('.manifest.json').read_bytes() == build_tasks.json_bytes(archive_doc(raw, items, manifest_path.read_bytes())), 'Negative archive manifest differs')
    cpio.inspect_with_gnu(archive_path, items, output, 'negative-archive')
    report = dict(bytes=len(raw), sha256=cpio.sha(raw), entries=15, files=13,
                  layout_scope='task-negative-tests' if negative_layout else 'provisional ordinary bootstrap; not guest evidence')
    if kernel is not None:
        # A single archive contains every dedicated root/catalog executable.
        # The shared legacy hello can also exist in the independently checked
        # normal-app archive; only that exact file-backed member span is allowed.
        kernel_bytes = kernel.read_bytes()
        start = embedded_archive(kernel_bytes, raw, [payload for name, payload in data.items() if name != 'legacy_hello'])
        allowed = [(start, start + len(raw))]
        ordinary = FIXTURES / 'apps-initramfs.cpio'
        ordinary_raw = ordinary.read_bytes()
        # The ordinary archive need not be linked into a dedicated feature image.
        if ordinary_raw in kernel_bytes:
            nested = output / 'ordinary-binding'; nested.mkdir()
            build_apps.verify(FIXTURES / 'apps/manifest.json', ordinary, nested)
            normal_start = embedded_archive(kernel_bytes, ordinary_raw, [])
            position = ordinary_raw.find(data['legacy_hello'])
            require(position >= 0 and ordinary_raw.find(data['legacy_hello'], position+1) < 0, 'Legacy hello not unique in ordinary archive')
            allowed.append((normal_start+position, normal_start+position+len(data['legacy_hello'])))
        phoff = struct.unpack_from('<Q', kernel_bytes, 32)[0]
        count = struct.unpack_from('<H', kernel_bytes, 56)[0]
        payload = data['legacy_hello']
        for index in range(count):
            kind, _, offset, _, _, size = struct.unpack_from('<IIQQQQ', kernel_bytes, phoff+index*56)
            if kind != 1: continue
            position = kernel_bytes.find(payload, offset, offset+size)
            while position >= 0:
                require(any(left <= position and position+len(payload) <= right for left, right in allowed), 'Standalone/unbound legacy ELF in kernel')
                position = kernel_bytes.find(payload, position+1, offset+size)
        report['kernel_file_offset'] = start
    return apps, report


def compile_command(source, linker, elf, crate, extra):
    return ['rustc', '--edition=2024', '--crate-name', crate, '--crate-type=bin', '--target', 'aarch64-unknown-none', *extra,
            '-C', 'panic=abort', '-C', 'opt-level=2', '-C', 'codegen-units=1', '-C', 'relocation-model=static',
            '-C', f'link-arg=-T{linker}', '-C', 'link-arg=--build-id=none', '-C', 'link-arg=-z',
            '-C', 'link-arg=max-page-size=4096', '-C', 'link-arg=--no-dynamic-linker', str(source), '-o', str(elf)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--layout', type=Path, required=True)
    parser.add_argument('--update', action='store_true')
    args = parser.parse_args(); output = args.output.resolve()
    if output.exists(): parser.error('preserve evidence; use fresh output directory')
    output.mkdir(parents=True)
    sources = ['crates/hm-core/fixtures/task_negative_cases.' + suffix for suffix in ('rs', 'S', 'ld')]
    sources += ['crates/hm-core/fixtures/' + name for name in ('apps.rs', 'apps.S', 'native-elf.ld', 'apps/hello.elf')]
    sources += ['crates/hm-kernel/src/startup_input.rs', 'crates/hm-kernel/src/task_wire.rs', 'tools/build_task_negative_cases.py',
                'tools/build_tasks.py', 'tools/build_task_cases.py', 'tools/build_apps.py', 'tools/build_initramfs.py',
                'tools/test_runtime.py', 'tools/test_process.py']
    (output / 'input.json').write_bytes(build_tasks.json_bytes(dict(update=args.update, layout=str(args.layout.resolve()),
        sources={name: cpio.sha((ROOT / name).read_bytes()) for name in sources})))
    try:
        _, layout = verify_layout(args.layout.resolve())
        data, apps = {}, {}
        for name in APPS:
            elf = output / (name + '.elf')
            if name == 'legacy_hello':
                command = compile_command(FIXTURES / 'apps.rs', FIXTURES / 'native-elf.ld', elf, 'native_apps', ['--cfg', 'app="hello"'])
            else:
                case = 100 if name == 'worker' else 101 if name == 'loop' else PROFILES.index(name)
                command = ['env', f'HM_TASK_NEGATIVE_CASE={case}'] + compile_command(FIXTURES / 'task_negative_cases.rs',
                    FIXTURES / 'task_negative_cases.ld', elf, 'task_negative_cases', ['-C', 'strip=debuginfo'])
            cpio.run(command, output, name + '-build')
            data[name] = elf.read_bytes()
            if name == 'legacy_hello': require(data[name] == LEGACY.read_bytes(), 'Rebuilt legacy hello differs from existing canonical bytes')
            symbols, errors = cpio.run(['readelf', '-sW', str(elf)], output, name + '-build-symbols')
            require(not errors, 'readelf diagnostics')
            apps[name] = dict(inspect(data[name], symbols.decode('ascii'), name), file=name + '.elf', path=path_for(name), sha256=cpio.sha(data[name]))
        manifest = build_tasks.json_bytes(dict(version=1, layout_sha256=cpio.sha(args.layout.read_bytes()), fixtures=apps))
        files = {'manifest.json': manifest, 'layout.rs': constants(layout, apps)} | {name+'.elf': data[name] for name in APPS}
        items = entries(data); archive = cpio.newc(items)
        archives = {ARCHIVE.name: archive, ARCHIVE.with_suffix('.manifest.json').name: build_tasks.json_bytes(archive_doc(archive, items, manifest))}
        for name, content in files.items(): (output / name).write_bytes(content)
        for name, content in archives.items(): (output / name).write_bytes(content)
        _, report = verify(output / 'manifest.json', output / ARCHIVE.name, args.layout.resolve(), output)
        for folder, values in ((CANONICAL, files), (FIXTURES, archives)):
            for name, content in values.items():
                path = folder / name
                if args.update: path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(content)
                else: require(path.is_file() and path.read_bytes() == content, 'Canonical negative artifact changed: ' + str(path))
        result = dict(passed=True, archive=report, scope='dedicated negative fixture artifacts only; no hook or execution evidence')
    except (OSError, ValueError, KeyError, TypeError, UnicodeError, struct.error, StopIteration) as error:
        result = dict(passed=False, error=str(error))
    (output / 'result.json').write_bytes(build_tasks.json_bytes(result)); print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    if sys.argv[1:] == ['--list-profiles']:
        print(' '.join(PROFILES))
    else:
        sys.exit(main())
