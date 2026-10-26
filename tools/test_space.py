#!/usr/bin/env python3
"""Strict owned-space loading and installed AT translation profiles; no EL0 execution."""
import argparse
from datetime import datetime, timezone
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import time

from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_preemption_negative import artifact, bounded_bytes, MAX_LOG_BYTES

CASES = ('fixture', 'budgets', 'pressure', 'geometry', 'forget')
SYMBOLS = ('arm_root', '__text_start', '__bss_start')
BASE = 0x100000000
PAGE_VAS = [BASE + n * 4096 for n in range(5)] + [BASE + n * 4096 for n in range(496, 512)]
FLAGS = [5, 4, 6, 6, 6] + [6] * 16
TIMEOUT_SECONDS = 25
NUMBER = r'(0|[1-9][0-9]{0,19})'


def fields(line, label, names):
    match = re.fullmatch('HMSPACE ' + label + ''.join(' ' + name + '=' + NUMBER for name in names), line)
    if not match:
        raise ValueError('Malformed ' + label + ' record')
    result = tuple(map(int, match.groups()))
    if any(value > 0xffffffffffffffff for value in result):
        raise ValueError('Integer outside u64')
    return result


def par_matches(par, address=None, fault=None):
    if fault is not None:
        return par & 1 == 1 and (par >> 1) & 63 == fault and par & 0x300 == 0
    return par & 1 == 0 and (par & 0x000fffffffffF000) | (address & 4095) == address


def records(console):
    found, positions, errors = [], [], []
    inside = False
    lines = console.split(b'\n')
    for index, raw in enumerate(lines):
        raw = raw.removesuffix(b'\r')
        if b'HMSPACE' in raw:
            if index == len(lines) - 1:
                errors.append('Unterminated space record')
            try:
                line = raw.decode('ascii')
            except UnicodeError:
                errors.append('Non-ASCII space record')
                continue
            found.append(line)
            positions.append(index)
            if line.startswith('HMSPACE BEGIN'):
                inside = True
            elif line.startswith('HMSPACE END'):
                inside = False
        elif inside:
            errors.append('Unexpected line inside space report')
    return found, positions, errors


def validate(console, trace, execution, symbols, cpus=1):
    found, positions, errors = records(console)
    checks = {'launched': execution.get('launch_error') is None,
              'within_deadline': not execution.get('timed_out', True),
              'expected_exit': execution.get('returncode') == 0,
              'no_fatal_or_foreign_report': not any(marker in console for marker in
                  (b'HMEXCEPTION', b'HMSMP', b'HMPREEMPT', b'HMSHARED')),
              'symbols': set(symbols) == set(SYMBOLS) and all(type(v) is int and 0x40000000 <= v < 0x41000000 for v in symbols.values()),
              'cpu_profile': cpus in (1, 2)}
    mappings, metrics = [], []
    try:
        cursor = iter(found)
        if next(cursor) != 'HMSPACE BEGIN 5':
            raise ValueError('Wrong BEGIN')
        if fields(next(cursor), 'INFO', ('cpus', 'peer_off', 'image_pages', 'required')) != (cpus, 1, 5, 33):
            raise ValueError('Wrong INFO')
        arena_va, arena_pa = fields(next(cursor), 'ARENA', ('va', 'pa'))
        if arena_va != 0x80000000 or not aligned_frame(arena_pa):
            raise ValueError('Wrong ARENA')
        for va, flag in zip(PAGE_VAS, FLAGS):
            actual_va, pa, actual_flags = fields(next(cursor), 'MAP', ('va', 'pa', 'flags'))
            if actual_va != va or actual_flags != flag or not aligned_frame(pa):
                raise ValueError('Wrong MAP')
            mappings.append((va, pa, flag))
        if len({pa for _, pa, _ in mappings} | {arena_pa}) != 22:
            raise ValueError('Aliased MAP or ARENA')
        queries = []
        for va, pa, flag in mappings:
            queries.extend([(va + 17, 0, (None, 5), (pa + 17, None)),
                            (va + 17, 1, (None, 5), (pa + 17, None) if flag == 6 else (None, 15)),
                            (pa + 17, 3, (pa + 17, None), (None, 15) if flag == 5 else (pa + 17, None))])
        for va in (BASE + 0x1ef000, BASE + 0x100000, BASE + 0x10000):
            queries.append((va, 0, (None, 5), (None, 7)))
        for va in (symbols['__text_start'], symbols['__bss_start'], 0x9000000, 0x8000000):
            queries.append((va, 0, (None, 15), (None, 15)))
        queries.append((arena_va, 2, (arena_pa, None), (None, 5)))
        private_root = None
        for iteration in range(2):
            for index, (va, access, boot, active) in enumerate(queries):
                if index % 16 == 0:
                    pass_id, batch, root, restored, controls = fields(next(cursor), 'ROOT', ('pass', 'batch', 'active', 'restored', 'controls'))
                    if (pass_id, batch, restored, controls) != (iteration, index // 16, symbols['arm_root'], 1):
                        raise ValueError('Wrong ROOT restoration')
                    if not aligned_frame(root) or root in {pa for _, pa, _ in mappings} | {arena_pa, symbols['arm_root']}:
                        raise ValueError('Invalid private root')
                    if private_root is not None and root != private_root:
                        raise ValueError('Private root changed')
                    private_root = root
                values = fields(next(cursor), 'PROBE', ('pass', 'index', 'va', 'access', 'before', 'active', 'after'))
                if values[:4] != (iteration, index, va, access):
                    raise ValueError('Wrong PROBE inventory')
                for par, expected in zip(values[4:], (boot, active, boot)):
                    if not par_matches(par, *expected):
                        raise ValueError('Wrong PAR fault/status or physical translation')
        baseline = None
        for name in CASES:
            before, during, after, frames, attempts = fields(next(cursor), 'METRIC case=' + name, ('before', 'during', 'after', 'frames', 'attempts'))
            if baseline is None:
                baseline = before
            if before != baseline or before < 284 or before > 4096:
                raise ValueError('Invalid frame baseline')
            expected = {'fixture': (before - 33, before, 33, 1), 'budgets': (before, before, 33, 33),
                        'pressure': (8, before, 33, 1), 'geometry': (before - 284, before, 284, 2),
                        'forget': (before - 33, before - 33, 33, 1)}[name]
            if (during, after, frames, attempts) != expected:
                raise ValueError('Wrong resource accounting')
            metrics.append({'case': name, 'before': before, 'during': during, 'after': after, 'frames': frames, 'attempts': attempts})
            if next(cursor) != 'HMSPACE PASS ' + name:
                raise ValueError('Wrong PASS inventory')
        if next(cursor) != 'HMSPACE END 5' or next(cursor, None) is not None:
            raise ValueError('Wrong END or trailing record')
    except (StopIteration, KeyError, ValueError, TypeError) as error:
        errors.append(str(error) or 'Incomplete report')
    checks['strict_report'] = not errors
    baseline_positions = [i for i, line in enumerate(console.split(b'\n')) if b'HMTEST' in line]
    checks['baseline_after_report'] = bool(baseline_positions and positions and min(baseline_positions) > max(positions))
    post = validate_run(console, execution.get('returncode'), arch='aarch64',
                        timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    checks['post_probe_baseline'] = post['passed']
    events = [line for line in trace.splitlines() if line.startswith(b'Taking exception')]
    terminal = b'Taking exception 16 [Semihosting call] on CPU 0'
    allowed = {terminal, b'Taking exception 11 [Hypervisor Call] on CPU 0',
               b'Taking exception 3 [Prefetch Abort] on CPU 0', b'Taking exception 4 [Data Abort] on CPU 0',
               b'Taking exception 5 [IRQ] on CPU 0'}
    checks['trace_complete'] = bool(trace) and len(trace) <= MAX_LOG_BYTES and trace.endswith(b'\n')
    checks['only_cpu0_admitted_events'] = all(event in allowed for event in events)
    checks['terminal_exit'] = bool(events) and events[-1] == terminal and events.count(terminal) == 1 and trace.splitlines()[-1] == b'...handling as semihosting call 0x20'
    checks['peer_off_hvc_evidence'] = cpus == 1 or b'Taking exception 11 [Hypervisor Call] on CPU 0' in events
    return {'passed': all(checks.values()), 'checks': checks, 'errors': errors, 'metrics': metrics,
            'observed_records': found, 'post_probe': post,
            'scope': 'owned ELF loading and installed AT data translations; no EL0 execution or execute-permission claim'}


def aligned_frame(value):
    return 0x40000000 <= value < 0x41000000 and value % 4096 == 0


def read_symbols(kernel, output):
    command = ['readelf', '-sW', str(kernel)]
    write_json(output / 'symbols.command.json', command)
    found = {name: [] for name in SYMBOLS}
    result = {'returncode': None, 'error': None}
    try:
        proc = subprocess.run(command, capture_output=True, timeout=5)
        result['returncode'] = proc.returncode
        (output / 'symbols.stdout.log').write_bytes(proc.stdout)
        (output / 'symbols.stderr.log').write_bytes(proc.stderr)
        if proc.returncode == 0:
            for line in proc.stdout.decode('ascii').splitlines():
                parts = line.split()
                if len(parts) >= 8 and parts[-1] in found:
                    found[parts[-1]].append(int(parts[1], 16))
    except (OSError, ValueError, UnicodeError, subprocess.TimeoutExpired) as error:
        result['error'] = str(error)
    result['values'] = found
    result['passed'] = result['error'] is None and result['returncode'] == 0 and all(len(v) == 1 for v in found.values())
    write_json(output / 'symbols.result.json', result)
    return {name: values[0] for name, values in found.items()} if result['passed'] else {}


def run_profile(kernel, output, memory=128, cpus=1):
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-smp') + 1] = str(cpus)
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-accel') + 1] = 'tcg,thread=multi'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log')]
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', {'kernel': str(kernel), 'sha256': hashlib.sha256(kernel.read_bytes()).hexdigest(), 'cpus': cpus, 'ram_mb': memory, 'accel': 'tcg,thread=multi'})
    symbols = read_symbols(kernel, output)
    started = time.monotonic()
    execution = {'started_utc': datetime.now(timezone.utc).isoformat(), 'timeout_seconds': TIMEOUT_SECONDS,
                 'returncode': None, 'timed_out': False, 'launch_error': None}
    with (output / 'serial.log').open('wb') as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT) as proc:
                try:
                    proc.wait(timeout=TIMEOUT_SECONDS)
                except subprocess.TimeoutExpired:
                    execution['timed_out'] = True
                    proc.kill()
                    proc.wait()
                execution['returncode'] = proc.returncode
        except OSError as error:
            execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - started
    paths = {name: output / file for name, file in (('serial', 'serial.log'), ('exceptions', 'exceptions.log'))}
    artifacts = {name: artifact(path) for name, path in paths.items()}
    result = validate(bounded_bytes(paths['serial']), bounded_bytes(paths['exceptions']), execution, symbols, cpus)
    result['checks']['artifacts_present_and_bounded'] = all(a['exists'] and a['within_size_limit'] for a in artifacts.values())
    result['passed'] = all(result['checks'].values())
    result.update(execution, artifacts=artifacts, symbols=symbols)
    write_json(output / 'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output, kernel = args.output.resolve(), args.kernel.resolve()
    if not kernel.is_file() or output.exists():
        parser.error('require built kernel and fresh evidence directory')
    output.mkdir(parents=True)
    profiles = {}
    for memory, cpus in ((64, 1), (128, 1), (128, 2)):
        name = f'space-{memory}m-{cpus}cpu'
        profiles[name] = run_profile(kernel, output / name, memory, cpus)['passed']
    result = {'passed': all(profiles.values()), 'profiles': profiles}
    write_json(output / 'result.json', result)
    print(result)
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
