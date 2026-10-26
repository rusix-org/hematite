#!/usr/bin/env python3
"""Build deterministic newc inputs and cross-check GNU listing/payload extraction."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CANONICAL = ROOT / 'crates/hm-core/fixtures/initramfs.cpio'
CANONICAL_MANIFEST = CANONICAL.with_suffix('.manifest.json')
ELF = ROOT / 'crates/hm-core/fixtures/process/hello.elf'
BANNER = b'hematite native initramfs\n'
ORDER = ('bin', 'bin/hello', 'etc', 'etc/banner')
MAX_BYTES = 4 * 1024 * 1024
TIMEOUT = 20


def sha(data):
    return hashlib.sha256(data).hexdigest()


def payloads(elf):
    if not 64 <= len(elf) <= 1024 * 1024 or not elf.startswith(b'\x7fELF'):
        raise ValueError('require bounded native ELF input')
    return [('bin', 0o40755, b''), ('bin/hello', 0o100755, elf),
            ('etc', 0o40755, b''), ('etc/banner', 0o100644, BANNER)]


def process_payloads(source=None):
    from test_process import CASES
    source = source or ELF.parent / 'manifest.json'
    document = json.loads(source.read_text())
    if set(document['fixtures']) != set(CASES):
        raise ValueError('process fixture inventory mismatch')
    entries = [('bin', 0o40755, b'')]
    for case in CASES:
        item = document['fixtures'][case]
        data = (source.parent / (case + '.elf')).read_bytes()
        if item['file'] != case + '.elf' or item['sha256'] != sha(data):
            raise ValueError('process ELF hash mismatch: ' + case)
        payloads(data)  # Shared bounded ELF input check.
        entries.append(('bin/' + case, 0o100755, data))
    return entries + [('etc', 0o40755, b''), ('etc/banner', 0o100644, BANNER)]


def newc(entries):
    """Trusted fixture encoder, not a parser or extraction implementation."""
    if len(entries) > 64 or any(len(name.encode('ascii')) > 128 or len(data) > 1024 * 1024 for name, _, data in entries):
        raise ValueError('entry exceeds profile capacity')
    result = bytearray()
    for inode, (name, mode, content) in enumerate(entries, 1):
        encoded = name.encode('ascii') + b'\0'
        nlink = 2 if mode & 0o170000 == 0o40000 else 1
        values = (inode, mode, 0, 0, nlink, 0, len(content), 0, 0, 0, 0, len(encoded), 0)
        result += b'070701' + ''.join(f'{value:08x}' for value in values).encode('ascii') + encoded
        result += b'\0' * (-len(result) % 4)
        result += content
        result += b'\0' * (-len(result) % 4)
    values = (0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 11, 0)
    result += b'070701' + ''.join(f'{value:08x}' for value in values).encode('ascii') + b'TRAILER!!!\0'
    result += b'\0' * (-len(result) % 4)
    result += b'\0' * (-len(result) % 512)
    if len(result) > MAX_BYTES:
        raise ValueError('archive exceeds profile capacity')
    return bytes(result)


def manifest(archive, entries):
    return {'version': 1, 'archive': CANONICAL.name, 'sha256': sha(archive), 'bytes': len(archive),
            'entries': [{'path': path, 'kind': 'directory' if mode & 0o170000 == 0o40000 else 'file',
                         'mode': mode, 'size': len(data), 'sha256': sha(data)} for path, mode, data in entries]}


def validate_listing(raw, entries, verbose=False):
    try:
        text = raw.decode('ascii')
    except UnicodeDecodeError:
        return False
    if not text.endswith('\n') or len(raw) > MAX_BYTES:
        return False
    if not verbose:
        return text.splitlines() == [path for path, _, _ in entries]
    lines = text.splitlines()
    if len(lines) != len(entries):
        return False
    for line, (path, mode, data) in zip(lines, entries):
        expected_mode = {0o40755: 'drwxr-xr-x', 0o100755: '-rwxr-xr-x', 0o100644: '-rw-r--r--'}[mode]
        pattern = (r'([d-][rwx-]{9})\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)'
                   r'\s+Jan\s+1\s+1970\s+(.+)')
        match = re.fullmatch(pattern, line)
        if not match:
            return False
        nlink, uid, gid, size = map(int, match.groups()[1:5])
        if ((match[1], uid, gid, size, match[6]) != (expected_mode, 0, 0, len(data), path)
                or nlink < 1 or (mode != 0o40755 and nlink != 1)):
            return False
    return True


def run(command, output, name, *, cwd=ROOT, input_bytes=b'', stdout_path=None):
    stdout_path = stdout_path or output / (name + '.stdout.log')
    stderr_path = output / (name + '.stderr.log')
    input_path = output / (name + '.stdin')
    input_path.write_bytes(input_bytes)
    (output / (name + '.command.json')).write_text(json.dumps({
        'argv': command, 'cwd': str(cwd), 'stdin_sha256': sha(input_bytes),
        'stdout': str(stdout_path), 'env': {'LC_ALL': 'C', 'TZ': 'UTC'}, 'timeout_seconds': TIMEOUT}, indent=2) + '\n')
    result = {'returncode': None, 'timed_out': False, 'error': None}
    with input_path.open('rb') as stdin, stdout_path.open('wb') as stdout, stderr_path.open('wb') as stderr:
        try:
            with subprocess.Popen(command, cwd=cwd, stdin=stdin, stdout=stdout, stderr=stderr,
                                  env={**os.environ, 'LC_ALL': 'C', 'TZ': 'UTC'}) as proc:
                try:
                    proc.wait(timeout=TIMEOUT)
                except subprocess.TimeoutExpired:
                    result['timed_out'] = True
                    proc.kill()
                    proc.wait()
                result['returncode'] = proc.returncode
        except OSError as error:
            result['error'] = str(error)
    result['bounded'] = stdout_path.stat().st_size <= MAX_BYTES and stderr_path.stat().st_size <= MAX_BYTES
    result['passed'] = result['returncode'] == 0 and not result['timed_out'] and result['error'] is None and result['bounded']
    (output / (name + '.result.json')).write_text(json.dumps(result, indent=2) + '\n')
    if not result['passed']:
        raise ValueError(name + ' failed: ' + str(result))
    return stdout_path.read_bytes(), stderr_path.read_bytes()


def inspect_with_gnu(path, entries, output, label):
    for verbose in (False, True):
        name = label + ('-verbose-list' if verbose else '-list')
        command = ['cpio', '--list', '--quiet', '--file=' + str(path)]
        if verbose:
            command += ['--verbose', '--numeric-uid-gid']
        stdout, stderr = run(command, output, name)
        if stderr or not validate_listing(stdout, entries, verbose):
            raise ValueError('GNU listing mismatch: ' + name)
    for member, mode, content in entries:
        if mode & 0o170000 != 0o100000:
            continue
        stdout, stderr = run(['cpio', '--extract', '--to-stdout', '--quiet', '--file=' + str(path), member],
                             output, label + '-extract-' + member.replace('/', '-'))
        if stderr or stdout != content:
            raise ValueError('GNU extracted payload mismatch: ' + member)


def build(output, update=False, process=False):
    canonical = CANONICAL.with_name("process-initramfs.cpio") if process else CANONICAL
    canonical_manifest = canonical.with_suffix(".manifest.json")
    entries = process_payloads() if process else payloads(ELF.read_bytes())
    generated = newc(entries)
    expected = manifest(generated, entries)
    expected['archive'] = canonical.name
    if process:
        expected['process_manifest_sha256'] = sha((ELF.parent / 'manifest.json').read_bytes())
    archive = output / canonical.name
    archive.write_bytes(generated)
    version, stderr = run(['cpio', '--version'], output, 'cpio-version')
    if stderr or b'GNU cpio' not in version:
        raise ValueError('GNU cpio is required')
    inspect_with_gnu(archive, entries, output, 'canonical')
    staging = output / 'gnu-input'
    staging.mkdir()
    for member, mode, content in entries:
        path = staging / member
        if mode & 0o170000 == 0o40000:
            path.mkdir()
        else:
            path.write_bytes(content)
        path.chmod(mode & 0o777)
    # Set all timestamps after creating children, since directory mtimes change.
    for member, _, _ in entries:
        os.utime(staging / member, (0, 0))
    reference = output / 'gnu-reference.cpio'
    _, stderr = run(['cpio', '--create', '--format=newc', '--reproducible', '--owner=0:0', '--null', '--quiet'],
                    output, 'gnu-create', cwd=staging,
                    input_bytes=b''.join(member.encode('ascii') + b'\0' for member, _, _ in entries), stdout_path=reference)
    if stderr:
        raise ValueError('GNU archive creation emitted diagnostics')
    inspect_with_gnu(reference, entries, output, 'reference')
    if update:
        canonical.write_bytes(generated)
        canonical_manifest.write_text(json.dumps(expected, indent=2) + '\n')
    else:
        if not canonical.is_file() or canonical.read_bytes() != generated:
            raise ValueError('Canonical archive does not reproduce byte-for-byte')
        if not canonical_manifest.is_file() or canonical_manifest.read_bytes() != (json.dumps(expected, indent=2) + '\n').encode():
            raise ValueError('Canonical manifest does not reproduce')
    (output / 'manifest.json').write_text(json.dumps(expected, indent=2) + '\n')
    return {'passed': True, 'canonical_sha256': sha(generated), 'reference_sha256': sha(reference.read_bytes()),
            'reference': str(reference), 'entries': len(entries),
            'scope': 'deterministic fixture, GNU listing and byte-exact stdout extraction; Rust admission tested separately'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--update', action='store_true')
    parser.add_argument('--process', action='store_true', help='separate full process archive, preserving the small parser fixture')
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a fresh output directory')
    output.mkdir(parents=True)
    sources = (ELF, Path(__file__).resolve(), ROOT / 'docs/design/0024-initramfs.md')
    if args.process:
        sources += (ELF.parent / 'manifest.json', ROOT / 'docs/design/0025-archive-execution.md')
        sources += tuple(ELF.parent / (case + '.elf') for case in __import__('test_process').CASES)
    (output / 'input.json').write_text(json.dumps({'sources': {str(path.relative_to(ROOT)): sha(path.read_bytes())
        for path in sources}, 'update': args.update, 'process': args.process}, indent=2) + '\n')
    try:
        result = build(output, args.update, args.process)
    except (OSError, ValueError, UnicodeError, subprocess.TimeoutExpired) as error:
        result = {'passed': False, 'error': str(error)}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
