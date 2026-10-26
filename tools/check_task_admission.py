#!/usr/bin/env python3
"""Run the exact safe task-admission integration inventory; this does not execute EL0."""
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

CASES = tuple(sorted((
    'all_task_operations',
    'byte_budget_survives_owned_transitions',
    'dropped_guard_is_session_local',
    'guard_at_attempt_63',
    'host_borrow_ends_at_admission',
    'legacy_dispatch_parity',
    'legacy_limit_parity',
    'malformed_limit_preserves_legacy_reply',
    'malformed_task_arguments_are_deferred',
    'non_wait_park_returns_guard',
    'parked_drop_is_session_local',
    'reply_preserves_session',
    'shared_attempt_counter',
    'stopped_session_is_sticky',
    'valid_task_limit_has_no_host_effect',
    'wait_park_wake_preserves_session',
)))
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
    expected = ['running 16 tests'] + [f'test {case} ... ok' for case in CASES]
    if lines[:-1] != expected:
        errors.append('Test inventory/order/results differ from the exact sixteen cases')
    if not lines or not re.fullmatch(r'test result: ok\. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in [0-9]+\.[0-9]+s', lines[-1]):
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
            'scope': 'actual owned Session policy; no capability issuance, core parking or EL0 task creation'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a fresh evidence directory')
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    command = ['cargo', 'test', '-p', 'hm-kernel', '--test', 'task_admission', '--locked', '--offline',
               '--target', 'x86_64-unknown-linux-gnu', '--target-dir', 'target/host-task-admission',
               '--', '--test-threads=1']
    def write(name, value):
        (output / name).write_text(json.dumps(value, indent=2) + '\n')
    write('command.json', command)
    sources = ('rust-toolchain.toml', 'Cargo.lock', 'Cargo.toml', 'crates/hm-kernel/Cargo.toml',
               'crates/hm-kernel/src/lib.rs', 'crates/hm-kernel/src/task_wire.rs',
               'crates/hm-kernel/src/startup_input.rs', 'crates/hm-kernel/src/native.rs',
               'crates/hm-kernel/tests/task_admission.rs', 'tools/check_task_admission.py',
               'docs/design/0029-session-admission.md')
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
