#!/usr/bin/env python3
"""Run the exact safe boot-image integration inventory; this does not execute EL0."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import signal
from pathlib import Path
import re
import subprocess
import sys
import time

CASES = tuple(sorted(('canonical_native_image', 'missing_entry', 'directory_entry', 'invalid_elf_entry',
    'exact_lookup_only', 'corrupt_suffix_rejects_existing_image', 'corrupt_suffix_precedes_missing',
    'image_outlives_temporary_path')))
TIMEOUT_SECONDS = 60
MAX_LOG_BYTES = 4 * 1024 * 1024


def validate(raw):
    errors = []
    try:
        text = raw.decode('ascii')
    except UnicodeDecodeError:
        text = ''
        errors.append('Non-ASCII test output')
    if not raw.endswith(b'\n') or len(raw) > MAX_LOG_BYTES:
        errors.append('Incomplete or oversized test output')
    lines = [line for line in text.split('\n') if line]
    expected = ['running 8 tests'] + [f'test {case} ... ok' for case in CASES]
    if lines[:-1] != expected:
        errors.append('Test inventory/order/results differ from the exact eight cases')
    if not lines or not re.fullmatch(r'test result: ok\. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in [0-9]+\.[0-9]+s', lines[-1]):
        errors.append('Missing or unsuccessful complete summary')
    return {'passed': not errors, 'errors': errors, 'expected_cases': list(CASES), 'observed_lines': lines}


def validate_execution(raw, execution, artifacts):
    report = validate(raw)
    checks = {
        'exit_success': execution.get('returncode') == 0,
        'within_deadline': execution.get('timed_out') is False,
        'launched': execution.get('launch_error') is None,
        'complete_report': report['passed'],
        'bounded_logs': set(artifacts) == {'stdout', 'stderr'} and
            all(item.get('within_limit') is True for item in artifacts.values()),
    }
    return {'passed': all(checks.values()), 'checks': checks, 'execution': execution,
            'artifacts': artifacts, 'report': report,
            'scope': 'actual safe complete-archive admission and ELF composition; no execution'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a fresh evidence directory')
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    command = ['cargo', 'test', '-p', 'hm-kernel', '--test', 'boot_image', '--locked', '--offline',
               '--target', 'x86_64-unknown-linux-gnu', '--target-dir', 'target/host-boot-image',
               '--', '--test-threads=1']
    def write(name, value):
        (output / name).write_text(json.dumps(value, indent=2) + '\n')
    write('command.json', command)
    sources = ('rust-toolchain.toml', 'Cargo.lock', 'Cargo.toml', 'crates/hm-kernel/Cargo.toml',
               'crates/hm-kernel/src/lib.rs', 'crates/hm-kernel/src/initramfs.rs',
               'crates/hm-kernel/src/boot_image.rs', 'crates/hm-kernel/src/elf.rs',
               'crates/hm-kernel/tests/boot_image.rs', 'tools/check_boot_image.py',
               'crates/hm-core/fixtures/malformed-initramfs.cpio',
               'crates/hm-core/fixtures/initramfs.cpio', 'crates/hm-core/fixtures/process/hello.elf')
    write('input.json', {'cwd': str(root), 'expected_cases': list(CASES),
        'sources': {
        name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in sources}})
    started = time.monotonic()
    execution = {'started_utc': datetime.now(timezone.utc).isoformat(), 'timeout_seconds': TIMEOUT_SECONDS,
                 'returncode': None, 'timed_out': False, 'launch_error': None}
    with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
        try:
            with subprocess.Popen(command, cwd=root, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True,
                                  env=os.environ.copy()) as proc:
                try:
                    proc.wait(timeout=TIMEOUT_SECONDS)
                except subprocess.TimeoutExpired:
                    execution['timed_out'] = True
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                execution['returncode'] = proc.returncode
        except OSError as error:
            execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - started
    artifacts = {}
    for name in ('stdout', 'stderr'):
        path = output / f'{name}.log'
        bounded = path.stat().st_size <= MAX_LOG_BYTES
        artifacts[name] = {'bytes': path.stat().st_size, 'within_limit': bounded,
                           'sha256': hashlib.sha256(path.read_bytes()).hexdigest() if bounded else None}
    with (output / 'stdout.log').open('rb') as stream:
        result = validate_execution(stream.read(MAX_LOG_BYTES + 1), execution, artifacts)
    write('result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
