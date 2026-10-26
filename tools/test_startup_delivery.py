#!/usr/bin/env python3
"""Verify real bounded startup-input delivery, faults and reclamation on ARM64."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import sys
import time

from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_launcher import TraceReader, SYMBOLS as LAUNCHER_SYMBOLS, SLICE_SETUP, SLICE_RESTORE, fixup_table
from test_process import fields as native_fields, read_symbols, require, NUMBER
from test_preemption_negative import artifact, bounded_bytes, MAX_LOG_BYTES

STARTUP = 0x100100000
PAYLOAD = STARTUP + 88
CASES = ('absent', 'empty', 'one', 'bytes255', 'binary256', 'after_binary_absent', 'payload_write',
         'after_write_absent', 'payload_execute', 'after_execute_absent', 'input_loop', 'after_loop_absent')
INPUTS = {'absent': None, 'empty': b'', 'one': b'A', 'bytes255': bytes(range(255)), 'binary256': bytes(range(256)),
          'payload_write': b'W', 'payload_execute': b'\xc0\x03\x5f\xd6', 'input_loop': b'L'}
SYMBOLS = tuple(dict.fromkeys(LAUNCHER_SYMBOLS + ('arm_root',)))
FIELDS = {
    'REJECT': 'phase error initialized generation before after',
    'PREFLIGHT': 'checks irqs',
    'INFO': 'wire extension max_input cpus timer_frequency timer_ticks',
    'RUN': 'index pass case fixture present length generation entry root before during owned',
    'READBACK': 'index base extension zero_tail source_detached',
    'STOP': 'index reason code calls written esr far elr spsr irqs',
    'RESTORE': 'index root vbar controls inactive before after',
    'PASS': 'index pass case',
    'PREPARE': 'budget exhausted drop generation before after',
}
TEXT = {'phase', 'error', 'case', 'fixture', 'reason'}
TIMEOUT = 60


def fields(line, kind):
    names = FIELDS[kind].split()
    pattern = 'HMINPUT ' + kind + ''.join(' ' + name + '=' + ('([a-z0-9_]+)' if name in TEXT else NUMBER) for name in names)
    match = re.fullmatch(pattern, line)
    require(match is not None, 'Malformed input record: ' + kind)
    result = {key: value if key in TEXT else int(value) for key, value in zip(names, match.groups())}
    require(all(value < 2**64 for value in result.values() if type(value) is int), 'Input record number outside u64')
    return result


def fixture_name(case):
    return 'absent' if case.startswith('after_') else case


def check_fixtures(fixtures):
    require(set(fixtures) == set(INPUTS), 'Wrong startup probe inventory')
    for name, payload in INPUTS.items():
        require(fixtures[name]['input'] == {'present': payload is not None, 'hex': (payload or b'').hex()} and
                type(fixtures[name]['input']['present']) is bool, 'Probe input expectation differs from independent case contract')


def expected_calls(name, fixture, index):
    if name in ('payload_write', 'payload_execute', 'input_loop'):
        return []
    payload = INPUTS[name]
    code = 10 if payload is None else 11 if not payload else 12
    handle = ((index + 1) << 32) | 1
    common = {'version': 1}
    return [dict(common, pc=fixture['symbols']['write_svc'] + 4, operation=1, arg1=handle, arg2=PAYLOAD,
                 arg3=len(payload or b''), output=payload or b'', status=0, bytes=len(payload or b'')),
            dict(common, pc=fixture['symbols']['newline_svc'] + 4, operation=1, arg1=handle,
                 arg2=fixture['symbols']['newline'], arg3=1, output=b'\n', status=0, bytes=1),
            dict(common, pc=fixture['symbols']['exit_svc'] + 4, operation=2, arg1=code, arg2=0, arg3=0,
                 output=b'', status=None, bytes=0)]


def validate_serial(raw, fixtures, symbols):
    check_fixtures(fixtures)
    found, positions, inside = [], [], False
    lines = raw.split(b'\n')
    for position, raw_line in enumerate(lines):
        line = raw_line.removesuffix(b'\r')
        if b'HMINPUT' in line or b'HMPROC' in line:
            require(position < len(lines) - 1, 'Unterminated startup report')
            found.append(line.decode('ascii')); positions.append(position)
            if line.startswith(b'HMINPUT REJECT phase=before'):
                inside = True
            if line == b'HMINPUT END 24':
                inside = False
        elif inside:
            raise ValueError('Foreign output inside startup report')
    cursor = iter(found)
    reject = fields(next(cursor), 'REJECT')
    baseline = reject['before']
    require(285 <= baseline <= 4096 and reject == {'phase': 'before', 'error': 'length', 'initialized': 0,
            'generation': 0, 'before': baseline, 'after': baseline}, 'Input rejection changed initial state')
    require(fields(next(cursor), 'PREFLIGHT') == {'checks': 15, 'irqs': 8}, 'Wrong startup preflight')
    require(next(cursor) == 'HMINPUT BEGIN 24', 'Wrong startup BEGIN')
    info = fields(next(cursor), 'INFO')
    require({key: value for key, value in info.items() if key not in ('timer_frequency', 'timer_ticks')} ==
            {'wire': 1, 'extension': 1, 'max_input': 256, 'cpus': 1}, 'Wrong startup INFO')
    require(1000 <= info['timer_frequency'] <= 1000000000 and info['timer_ticks'] == info['timer_frequency'] // 10, 'Wrong deadline')
    runs = []
    for index in range(24):
        case = CASES[index % 12]
        name = fixture_name(case)
        fixture, payload = fixtures[name], INPUTS[name]
        row = fields(next(cursor), 'RUN')
        root = row['root']
        require(root % 4096 == 0 and 0x40000000 <= root < 0x41000000 and root != symbols['arm_root'], 'Invalid private root')
        require(row == {'index': index, 'pass': index // 12, 'case': case, 'fixture': name,
            'present': int(payload is not None), 'length': len(payload or b''), 'generation': index + 1,
            'entry': fixture['entry'], 'root': root, 'before': baseline, 'during': baseline - fixture['pages'] - 29,
            'owned': fixture['pages'] + 29}, 'Wrong startup run or resource count')
        require(fields(next(cursor), 'READBACK') == {'index': index, 'base': 1, 'extension': 1,
            'zero_tail': 1, 'source_detached': 1}, 'Missing complete inactive startup readback')
        calls = expected_calls(name, fixture, index)
        written = 0
        for ordinal, call in enumerate(calls, 1):
            require(native_fields(next(cursor), 'CALL') == {'index': index, 'ordinal': ordinal,
                    **{key: call[key] for key in ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')}}, 'Wrong actual input CALL')
            if call['output']:
                require(native_fields(next(cursor), 'OUTPUT') == {'index': index, 'ordinal': ordinal,
                        'length': len(call['output']), 'hex': call['output'].hex()}, 'Wrong delivered byte output')
                written += len(call['output'])
            if call['status'] is not None:
                require(native_fields(next(cursor), 'RESULT') == {'index': index, 'ordinal': ordinal,
                        'status': 0, 'bytes': call['bytes']}, 'Wrong native input result')
        stop = fields(next(cursor), 'STOP')
        reason = 'exit' if calls else 'deadline' if name == 'input_loop' else 'fault'
        code = calls[-1]['arg1'] if calls else 0
        require({key: stop[key] for key in ('index', 'reason', 'code', 'calls', 'written', 'irqs')} ==
                {'index': index, 'reason': reason, 'code': code, 'calls': len(calls), 'written': written,
                 'irqs': int(reason == 'deadline')}, 'Wrong input proof outcome')
        expected_pstate = 0x00400740 if name == 'payload_execute' else 0x00400340
        observed_pstate = stop['spsr'] & ~0xf0000000 if calls else stop['spsr']
        require(observed_pstate == expected_pstate, 'Wrong saved PSTATE/BTYPE or uncleared demo NZCV')
        if calls:
            require((stop['esr'], stop['far'], stop['elr']) == (0x56000000, 0, calls[-1]['pc']), 'Wrong exit proof SVC')
        elif name == 'payload_write':
            require((stop['esr'], stop['far'], stop['elr']) == (0x9200004f, PAYLOAD, fixture['symbols']['fault_write']), 'Wrong payload write fault')
        elif name == 'payload_execute':
            require((stop['esr'], stop['far'], stop['elr']) == (0x8200000f, PAYLOAD, PAYLOAD), 'Wrong payload execute fault')
        else:
            require(stop['esr'] == stop['far'] == 0 and fixture['symbols']['loop_start'] <= stop['elr'] < fixture['symbols']['loop_end'], 'Wrong interrupted input loop')
        require(fields(next(cursor), 'RESTORE') == {'index': index, 'root': symbols['arm_root'], 'vbar': symbols['arm_vectors'],
                'controls': 1, 'inactive': 1, 'before': baseline, 'after': baseline}, 'Startup teardown incomplete')
        require(fields(next(cursor), 'PASS') == {'index': index, 'pass': index // 12, 'case': case}, 'Wrong input PASS')
        runs.append({'name': name, 'entry': fixture['entry'], 'calls': calls, 'stop': stop})
    require(fields(next(cursor), 'REJECT') == {'phase': 'after', 'error': 'length', 'initialized': 1,
            'generation': 24, 'before': baseline, 'after': baseline}, 'Late rejected input consumed state')
    require(fields(next(cursor), 'PREPARE') == {'budget': 1, 'exhausted': 1, 'drop': 1, 'generation': 27,
            'before': baseline, 'after': baseline}, 'Preparation rollback/generation/drop proof missing')
    require(next(cursor) == 'HMINPUT END 24' and next(cursor, None) is None, 'Wrong input END/extra records')
    baseline_positions = [i for i, line in enumerate(raw.split(b'\n')) if b'HMTEST' in line]
    require(baseline_positions and min(baseline_positions) > max(positions), 'Baseline precedes completed input teardown')
    return {'runs': runs, 'baseline': baseline, 'records': len(found)}


def validate_trace(raw, report, symbols, fixups):
    reader = TraceReader(raw, symbols, fixups)
    reader.timer_group()
    for run in report['runs']:
        reader.writes(SLICE_SETUP)
        reader.returned(0, run['entry'])
        for ordinal, call in enumerate(run['calls'], 1):
            event = reader.take()
            require(event['kind'] == 'exception' and event['number'] == 2 and event['source'] == 0 and event['target'] == 1 and
                    event['esr'] == 0x56000000 and event.get('far') is None and event['elr'] == call['pc'] and
                    event['vector'] == symbols['arm_process_vectors'] + 0x400, 'Wrong input EL0 SVC')
            reader.writes(SLICE_RESTORE)
            if ordinal < len(run['calls']):
                reader.writes(SLICE_SETUP); reader.returned(0, call['pc'])
        if not run['calls']:
            event, stop = reader.take(), run['stop']
            require(event['kind'] == 'exception' and event['source'] == 0 and event['target'] == 1 and event['elr'] == stop['elr'], 'Missing input lower-EL exception')
            irq = run['name'] == 'input_loop'
            require(event['vector'] == symbols['arm_process_vectors'] + (0x480 if irq else 0x400), 'Wrong input vector')
            if irq:
                require(event['number'] == 5, 'Input loop did not expire through timer IRQ')
                ack = reader.take()
                require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong input IRQ acknowledgment')
                reader.writes(((16, 30),))
            else:
                require(event['number'] == (3 if run['name'] == 'payload_execute' else 4) and
                        event['esr'] == stop['esr'] and event.get('far') == stop['far'], 'Wrong actual startup permission fault')
            reader.writes(SLICE_RESTORE)
    reader.baseline()
    event = reader.take()
    require(event['kind'] == 'exception' and event['number'] == 16, 'Missing final privileged test exit')
    reader.done()
    return {'runs': 24, 'slices': 60, 'el0_svcs': 54, 'el0_faults': 4, 'el0_timer_irqs': 2,
            'preflight_irqs': 8, 'baseline_irqs': 16}


def validate(console, trace, execution, fixtures, symbols, fixups):
    errors, report, traced = [], None, None
    try:
        require(execution.get('returncode') == 0 and execution.get('timed_out') is False and
                execution.get('launch_error') is None, 'Input guest did not exit successfully within deadline')
        require(not any(marker in console for marker in (b'HMSPACE', b'HMEXCEPTION', b'HMSMP', b'HMPREEMPT')), 'Foreign profile report')
        report = validate_serial(console, fixtures, symbols)
        traced = validate_trace(trace, report, symbols, fixups)
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration, IndexError) as error:
        errors.append(str(error) or 'Incomplete startup report')
    baseline = validate_run(console, execution.get('returncode'), arch='aarch64', timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    return {'passed': not errors and baseline['passed'], 'errors': errors, 'trace': traced,
            'records': report['records'] if report else 0, 'baseline': baseline,
            'scope': 'actual native startup bytes/ownership/RO-NX containment and reclamation on pinned QEMU'}


def run_profile(output, kernel, fixtures, symbols, fixups, memory):
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'), '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', {'kernel_sha256': hashlib.sha256(kernel.read_bytes()).hexdigest(), 'ram_mb': memory, 'timeout_seconds': TIMEOUT})
    execution = {'returncode': None, 'timed_out': False, 'launch_error': None}
    started = time.monotonic()
    with (output / 'serial.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr) as proc:
                while proc.poll() is None:
                    if any(path.exists() and path.stat().st_size > MAX_LOG_BYTES for path in
                           (output / 'serial.log', output / 'stderr.log', output / 'exceptions.log')):
                        execution['launch_error'] = 'Runtime artifact exceeded bounded size'
                        proc.kill(); proc.wait(); break
                    remaining = TIMEOUT - (time.monotonic() - started)
                    if remaining <= 0:
                        execution['timed_out'] = True
                        proc.kill(); proc.wait(); break
                    try:
                        proc.wait(timeout=min(remaining, 0.25))
                    except subprocess.TimeoutExpired:
                        pass
                execution['returncode'] = proc.returncode
        except OSError as error:
            execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - started
    result = validate(bounded_bytes(output / 'serial.log'), bounded_bytes(output / 'exceptions.log'), execution, fixtures, symbols, fixups)
    result['execution'] = execution
    result['artifacts'] = {name: artifact(output / name) for name in ('serial.log', 'exceptions.log', 'stderr.log')}
    result['passed'] &= all(item['within_size_limit'] for item in result['artifacts'].values())
    write_json(output / 'result.json', result)
    return result


def main():
    import build_startup_probes
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists(): parser.error('preserve evidence; use a fresh directory')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        kernel = args.kernel.resolve()
        fixtures, archive = build_startup_probes.verify(args.fixtures.resolve(), args.archive.resolve(), output, kernel)
        check_fixtures(fixtures)
        symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
        fixups = fixup_table(kernel.read_bytes(), symbols)
        sources = ('test_startup_delivery.py', 'test_launcher.py', 'test_process.py', 'build_startup_probes.py')
        write_json(output / 'metadata.json', {'fixtures': fixtures, 'archive': archive, 'symbols': symbols, 'fixups': fixups,
            'sources': {name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest() for name in sources}})
        for memory in (64, 128):
            name = f'startup-input-{memory}m'
            profiles[name] = run_profile(output / name, kernel, fixtures, symbols, fixups, memory)['passed']
    except (OSError, ValueError, KeyError, TypeError, UnicodeError, struct.error) as caught:
        error = str(caught)
    result = {'passed': error is None and len(profiles) == 2 and all(profiles.values()), 'profiles': profiles, 'error': error}
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
