#!/usr/bin/env python3
"""Strict inactive owned-copy evidence; no active EL0 or interactive READ claim."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import struct
import sys
import time
import build_rw_copy as builder
from test_process import require, read_symbols
from test_launcher import TraceReader, fixup_table, SYMBOLS as OLD_SYMBOLS
from test_space import par_matches
from test_kernel import qemu_command, validate_run
from test_preemption_negative import artifact, bounded_bytes, MAX_LOG_BYTES
from test_boot_profiles import write_json

CASES = ('lengths', 'page_end', 'cross_rw', 'cross_stack', 'tail_ro', 'tail_rx',
         'tail_absent', 'protected', 'holes', 'bounds', 'startup', 'translations')
SYMBOLS = tuple(n for n in OLD_SYMBOLS if n != 'arm_process_vectors') + ('arm_root',)
SLOTS = (2, 3, 4, 1, 256, 495)
# Both diagnostic profiles accepted in docs/reviews/0031-owned-rw-copy.md.
TRACE_GRAMMAR_REVIEWED = True
TIMEOUT_SECONDS = 30
LOG_POLL_SECONDS = 0.05
DEC = r'(0|[1-9][0-9]*)'
HEX = r'(0x(?:0|[1-9a-f][0-9a-f]*))'


def validate_serial(raw, symbols):
    require(raw and len(raw) <= MAX_LOG_BYTES and raw.endswith(b'\n'), 'Missing/bounded serial')
    lines = raw.decode('ascii').replace('\r\n', '\n').splitlines()
    selected = [i for i, line in enumerate(lines) if 'HMRWCOPY' in line]
    require(selected and selected == list(range(selected[0], selected[-1] + 1)), 'Interrupted copy inventory')
    require(not any('HMTEST' in line for line in lines[:selected[-1] + 1]), 'Baseline before copy END')
    require(not any(marker in raw for marker in (b'HMPROC', b'HMRT', b'HMTASK', b'HMSPACE', b'HMPREEMPT', b'HMSMP')), 'Foreign profile report')
    records = iter(lines[selected[0]:selected[-1] + 1])
    def exact(line): require(next(records, None) == line, 'Missing/duplicate/reordered copy record: ' + line)
    def record(kind, pattern):
        line = next(records, '')
        match = re.fullmatch('HMRWCOPY ' + kind + ' ' + pattern, line)
        require(match is not None, 'Malformed ' + kind + ' record')
        values = tuple(int(value, 16 if value.startswith('0x') else 10) for value in match.groups())
        require(all(value <= 0xffffffffffffffff for value in values), 'Integer outside u64')
        return values
    def frame(value): return (symbols['__kernel_end'] + 4095) & ~4095 <= value < 0x41000000 and value % 4096 == 0
    exact('HMRWCOPY BEGIN passes=2 cases=12')
    baseline = None
    for p in range(2):
        for name in CASES:
            (before,) = record('CASE', f'pass={p} name={name} before=' + DEC)
            require(35 <= before <= 4096 and (baseline is None or before == baseline), 'Changed free baseline')
            baseline = before
            if name == 'translations':
                prior, owner = None, None
                for stage in range(2):
                    root, restored = record('ROOT', f'pass={p} stage={stage} during=' + HEX + ' restored=' + HEX + ' controls=1')
                    require(frame(root) and root != symbols['arm_root'] and restored == symbols['arm_root'] and (owner is None or owner == root), 'Invalid installed/restored root')
                    owner = root
                    observations, physicals = [], {}
                    for index in range(12):
                        slot, access = SLOTS[index // 2], 'rw'[index % 2]
                        va, boot, active, after, physical = record('PROBE', f'pass={p} stage={stage} index={index} va=' + HEX + f' access={access} before=' + HEX + ' active=' + HEX + ' after=' + HEX + ' physical=' + HEX)
                        require(va == builder.BASE + slot * 4096, 'Wrong AT query address')
                        require(par_matches(boot, fault=5) and par_matches(after, fault=5), 'Boot VA unexpectedly accessible')
                        if slot == 495:
                            require(physical == 0 and par_matches(active, fault=7), 'Guard translation differs')
                        else:
                            require(frame(physical) and physical != root, 'Invalid reported backing frame')
                            require(slot not in physicals or physicals[slot] == physical, 'Read/write backing mismatch')
                            physicals[slot] = physical
                            require(par_matches(active, address=physical) if access == 'r' or slot in (2, 3) else par_matches(active, fault=15), 'Wrong active permission/PAR')
                        observations.append((va, physical, boot, active, after))
                    require(len(set(physicals.values())) == 5, 'Aliased reported backing pages')
                    require(prior is None or prior == observations, 'Copy changed translations')
                    prior = observations
            exact(f'HMRWCOPY PASS pass={p} name={name} frames=35 during={before - 35} after={before}')
    exact('HMRWCOPY END cases=24')
    require(next(records, None) is None, 'Extra copy records')
    return dict(cases=24, probes=48, free=baseline)


def validate_trace(raw, symbols, fixups):
    reader = TraceReader(raw, symbols, fixups)
    reader.baseline()
    event = reader.take()
    require(event['kind'] == 'exception' and event['number'] == 16 and event['source'] == event['target'] == 1, 'Missing privileged final exit')
    reader.done()
    return dict(baseline_irqs=16, baseline_faults=24, el0=0, hvc=0)


def validate(raw, trace, execution, symbols, fixups):
    errors, serial_report, traced = [], None, None
    try:
        serial_report = validate_serial(raw, symbols)
        traced = validate_trace(trace, symbols, fixups)
    except (ValueError, UnicodeError, KeyError, TypeError, StopIteration, IndexError) as error:
        errors.append(str(error))
    baseline = validate_run(raw, execution.get('returncode'), arch='aarch64', timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    checks = dict(strict_evidence=not errors, baseline=baseline['passed'], exit_success=execution.get('returncode') == 0,
                  deadline=execution.get('timed_out') is False, launched=execution.get('launch_error') is None,
                  trace_reviewed=TRACE_GRAMMAR_REVIEWED, log_limit=execution.get('log_limit_exceeded', False) is False)
    return dict(passed=all(checks.values()), checks=checks, errors=errors, serial=serial_report, trace=traced, baseline=baseline,
                scope='inactive owned RW copy and installed AT permissions; no EL0 execution or UART input')


def run_profile(kernel, output, symbols, fixups, memory, binding):
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-smp') + 1] = '1'
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'), '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    # Bind the actual imported validator/helper closure, including transitive
    # builder imports, rather than identifying mutable command paths alone.
    sources = {builder.ROOT / name for name in builder.SOURCES}
    sources.add(Path(__file__).resolve())
    for module in tuple(sys.modules.values()):
        filename = getattr(module, '__file__', None)
        if filename:
            source = Path(filename).resolve()
            if source.parent == builder.ROOT / 'tools' and source.suffix == '.py':
                sources.add(source)
    source_hashes = {str(path.relative_to(builder.ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                     for path in sorted(sources)}
    write_json(output / 'input.json', dict(kernel_sha256=hashlib.sha256(kernel.read_bytes()).hexdigest(),
               memory=memory, cpus=1, binding=binding, validator_sources=source_hashes,
               max_log_bytes=MAX_LOG_BYTES, log_poll_seconds=LOG_POLL_SECONDS))
    execution = dict(started_utc=datetime.now(timezone.utc).isoformat(), timeout_seconds=TIMEOUT_SECONDS, returncode=None, timed_out=False, launch_error=None, log_limit_exceeded=False)
    paths = {name: output / filename for name, filename in (('serial', 'serial.log'), ('trace', 'exceptions.log'))}
    start = time.monotonic()
    with (output / 'serial.log').open('wb') as log:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT) as proc:
                while True:
                    oversized = [name for name, path in paths.items()
                                 if path.exists() and path.stat().st_size > MAX_LOG_BYTES]
                    if oversized:
                        execution['log_limit_exceeded'] = True
                        execution['oversized_logs'] = oversized
                        proc.kill(); proc.wait()
                        break
                    if proc.poll() is not None:
                        break
                    remaining = TIMEOUT_SECONDS - (time.monotonic() - start)
                    if remaining <= 0:
                        execution['timed_out'] = True
                        proc.kill(); proc.wait()
                        break
                    try:
                        proc.wait(timeout=min(LOG_POLL_SECONDS, remaining))
                    except subprocess.TimeoutExpired:
                        pass
                execution['returncode'] = proc.returncode
        except OSError as error: execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - start
    artifacts = {name: artifact(path) for name, path in paths.items()}
    result = validate(bounded_bytes(paths['serial']), bounded_bytes(paths['trace']), execution, symbols, fixups)
    result['checks']['artifacts'] = all(item['exists'] and item['within_size_limit'] for item in artifacts.values())
    result.update(passed=all(result['checks'].values()), execution=execution, artifacts=artifacts)
    write_json(output / 'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, default=builder.FIXTURE)
    parser.add_argument('--manifest', type=Path, default=builder.MANIFEST)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output, kernel = args.output.resolve(), args.kernel.resolve()
    if output.exists() or not kernel.is_file(): parser.error('require built kernel and fresh output')
    output.mkdir(parents=True)
    try:
        builder.verify(args.fixture, args.manifest, output, kernel)
        symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
        fixups = fixup_table(kernel.read_bytes(), symbols)
        binding = dict(fixture_sha256=hashlib.sha256(args.fixture.read_bytes()).hexdigest(), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest())
        profiles = {str(memory): run_profile(kernel, output / f'rw-copy-{memory}m', symbols, fixups, memory, binding)['passed'] for memory in (64, 128)}
        result = dict(passed=all(profiles.values()), profiles=profiles)
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        result = dict(passed=False, error=str(error))
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
