#!/usr/bin/env python3
"""Strict native process evidence; grammar reviewed against pinned QEMU 7.2.22 EL0 traces."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import sys
import time

from check_timer_trace import PREFIX, validate_trace as validate_timer
from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_preemption_negative import artifact, bounded_bytes, MAX_LOG_BYTES

CASES = ('hello', 'badptr_null', 'badptr_overflow', 'badptr_crossing', 'oversize',
         'badhandle', 'badversion', 'badopcode', 'rxwrite', 'rwexecute', 'stackexecute',
         'kernelread', 'kernelwrite', 'uartread', 'gicread', 'cntpdisable', 'daifmask',
         'stackguard', 'loop', 'zerowrite', 'maxwrite', 'bytebudget', 'calllimit',
         'invalidstorm', 'exit64', 'invalidexit', 'stalehandle', 'nogrant', 'svcimm',
         'registers', 'badsp', 'startupwrite', 'semihost')
SYMBOLS = ('arm_root', 'arm_vectors', 'arm_process_vectors', 'arm_timer_window_begin',
           'arm_timer_window_end', '__text_start', '__bss_start')
BASE, STACK, MAX_U64 = 0x100000000, 0x100200000, (1 << 64) - 1
TIMEOUT_SECONDS = 60
NUMBER = r'(0|[1-9][0-9]{0,19})'
FIELDS = {
    'PREFLIGHT': 'checks irqs',
    'ARCHIVE': 'bytes entries files',
    'ADMISSION': 'case error before after',
    'INFO': 'wire cpus peer_off abi max_write byte_budget call_limit timer_frequency timer_ticks',
    'RUN': 'index pass case fixture entry root sp handle before during owned',
    'CALL': 'index ordinal pc version operation arg1 arg2 arg3',
    'OUTPUT': 'index ordinal length hex',
    'RESULT': 'index ordinal status bytes',
    'STOP': 'index reason code calls written esr far elr spsr irqs',
    'RESTORE': 'index root vbar controls inactive before after',
    'PASS': 'index pass case',
    'PREPARE': 'budget exhausted drop before after',
}
TEXT_FIELDS = {'case', 'fixture', 'reason', 'hex', 'error'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def fields(line, kind):
    names = FIELDS[kind].split()
    pieces = []
    for name in names:
        value = r'([a-z0-9_]+)' if name in TEXT_FIELDS else NUMBER
        pieces.append(f' {name}={value}')
    match = re.fullmatch('HMPROC ' + kind + ''.join(pieces), line)
    require(match is not None, 'Malformed ' + kind)
    result = {name: value if name in TEXT_FIELDS else int(value)
              for name, value in zip(names, match.groups())}
    require(all(value <= MAX_U64 for value in result.values() if type(value) is int), 'Integer outside u64')
    return result


def serial_records(raw):
    found, positions = [], []
    inside = False
    lines = raw.split(b'\n')
    for i, line in enumerate(lines):
        line = line.removesuffix(b'\r')
        if b'HMPROC' in line:
            require(i != len(lines) - 1, 'Unterminated process report')
            found.append(line.decode('ascii'))
            positions.append(i)
            if line.startswith(b'HMPROC PREFLIGHT'):
                inside = True
            elif line.startswith(b'HMPROC END'):
                inside = False
        elif inside:
            raise ValueError('Unexpected output inside process report')
    return found, positions


def expected_calls(case, fixture, handle, index):
    symbols = fixture['symbols']
    def write(length=0, address=MAX_U64, status=0, token=None, version=1, operation=1, output=b''):
        return {'pc': symbols['write_svc'] + 4, 'version': version, 'operation': operation,
                'arg1': handle if token is None else token, 'arg2': address, 'arg3': length,
                'status': status, 'bytes': len(output), 'output': output}
    def leave(code=0, address=0, length=0, status=None):
        return {'pc': symbols['exit_svc'] + 4, 'version': 1, 'operation': 2,
                'arg1': code, 'arg2': address, 'arg3': length,
                'status': status, 'bytes': 0, 'output': b''}
    if case == 'hello':
        return [write(15, symbols['message'], output=b'hello from EL0\n'), leave()]
    malformed = {
        'badptr_null': lambda: write(1, 0, 4),
        'badptr_overflow': lambda: write(8, MAX_U64 - 3, 4),
        'badptr_crossing': lambda: write(2, STACK - 1, 4),
        'oversize': lambda: write(257, symbols['data'], 5),
        'badhandle': lambda: write(status=3, token=handle ^ 1),
        'badversion': lambda: write(status=1, version=2),
        'badopcode': lambda: write(status=2, operation=255),
        'stalehandle': lambda: write(status=3, token=(index << 32) | 1),
        'nogrant': lambda: write(status=3, token=1),
    }
    if case in malformed:
        return [malformed[case](), leave()]
    if case == 'zerowrite':
        return [write(), leave()]
    if case == 'maxwrite':
        return [write(256, symbols['data'], output=bytes(range(256))), leave()]
    if case == 'bytebudget':
        return [write(256, symbols['data'], output=bytes(range(256))) for _ in range(16)] + [write(1, symbols['data'], 6), write(), leave()]
    if case == 'calllimit':
        return [write() for _ in range(64)]
    if case == 'invalidstorm':
        return [write(status=1, version=2) for _ in range(64)]
    if case == 'exit64':
        return [write() for _ in range(63)] + [leave()]
    if case == 'invalidexit':
        return [leave(256, status=7), leave(address=1, status=7), leave(length=1, status=7), leave()]
    if case == 'registers':
        calls = [write() for _ in range(3)]
        for j, call in enumerate(calls, 1):
            call['pc'] = symbols[f'register_svc_{j}'] + 4
        return calls + [leave()]
    if case == 'badsp':
        return [leave()]
    return []


def fault_contract(case, fixture, symbols):
    fs = fixture['symbols']
    target = {'rxwrite': fixture['entry'], 'rwexecute': fs.get('data'),
              'stackexecute': STACK - 16, 'kernelread': symbols['__text_start'],
              'kernelwrite': symbols['__bss_start'], 'uartread': 0x9000000,
              'gicread': 0x8000000, 'stackguard': BASE + 0x1ef000,
              'startupwrite': BASE + 0x100038}.get(case)
    if target is not None:
        execute = case in ('rwexecute', 'stackexecute')
        write = case in ('rxwrite', 'kernelwrite', 'stackguard', 'startupwrite')
        return {'ec': 0x20 if execute else 0x24, 'fsc': 7 if case == 'stackguard' else 15,
                'write': write, 'far': target, 'elr': target if execute else fs['fault']}
    # QEMU 7.2.22 daif_check swaps the encoded op1/op2 ISS fields. Require
    # its exact observed syndrome; architectural 0x620cd3e4 is not an alternative.
    # Provenance and independent review: docs/reviews/0023-arm-native-process.md.
    exact = {'cntpdisable': 0x6232fbe4, 'daifmask': 0x620793e4,
             'svcimm': 0x56000001, 'semihost': 0x02000000}
    require(case in exact, 'No terminal fault contract')
    return {'exact': exact[case], 'far': 0, 'elr': fs['fault'] + (4 if case == 'svcimm' else 0)}


def check_fault(esr, expected):
    if 'exact' in expected:
        return esr == expected['exact']
    return (esr >> 26 == expected['ec'] and esr & (1 << 25) != 0 and
            esr & 63 == expected['fsc'] and bool(esr & 64) == expected['write'] and
            esr & 0x780 == 0)  # S1PTW, EA, FnV and reserved status bits must be clear.


def validate_serial(raw, fixtures, symbols, cpus, archive_bytes):
    found, positions = serial_records(raw)
    cursor = iter(found)
    require(fields(next(cursor), 'PREFLIGHT') == {'checks': 15, 'irqs': 8}, 'Wrong timer preflight')
    require(fields(next(cursor), 'ARCHIVE') == {'bytes': archive_bytes, 'entries': 36, 'files': 34}, 'Wrong archive inventory')
    admission_baseline = None
    for case, error in (('malformed', 'archive'), ('missing', 'missing'), ('directory', 'directory'), ('invalidelf', 'elf')):
        row = fields(next(cursor), 'ADMISSION')
        if admission_baseline is None:
            admission_baseline = row['before']
        require(row == {'case': case, 'error': error, 'before': admission_baseline, 'after': admission_baseline}, 'Admission changed resources or outcome')
    require(next(cursor) == 'HMPROC BEGIN 66', 'Wrong BEGIN')
    info = fields(next(cursor), 'INFO')
    require({k: v for k, v in info.items() if k not in ('timer_frequency', 'timer_ticks')} == {
        'wire': 1, 'cpus': cpus, 'peer_off': 1, 'abi': 1, 'max_write': 256, 'byte_budget': 4096, 'call_limit': 64}, 'Wrong INFO')
    require(cpus in (1, 2) and 1000 <= info['timer_frequency'] <= 1000000000 and
            info['timer_ticks'] == info['timer_frequency'] // 10, 'Wrong timer budget')
    baseline, runs = admission_baseline, []
    for index in range(66):
        case = CASES[index % 33]
        fixture = fixtures[case]
        run = fields(next(cursor), 'RUN')
        handle = 0 if case == 'nogrant' else ((index + 1) << 32) | 1
        before, root = run['before'], run['root']
        if baseline is None:
            baseline = before
        require(run == {'index': index, 'pass': index // 33, 'case': case, 'fixture': case,
                        'entry': fixture['entry'], 'root': root, 'sp': STACK, 'handle': handle,
                        'before': baseline, 'during': before - fixture['pages'] - 29,
                        'owned': fixture['pages'] + 29}, 'Wrong RUN or resource accounting')
        require(285 <= before <= 4096 and 0x40000000 <= root < 0x41000000 and
                root % 4096 == 0 and root != symbols['arm_root'], 'Invalid root/free baseline')
        calls = expected_calls(case, fixture, handle, index)
        written = 0
        for ordinal, expected in enumerate(calls, 1):
            call = fields(next(cursor), 'CALL')
            require(call == {'index': index, 'ordinal': ordinal, **{k: expected[k] for k in
                    ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')}}, 'Wrong native CALL')
            if expected['output']:
                output = fields(next(cursor), 'OUTPUT')
                require(output == {'index': index, 'ordinal': ordinal, 'length': len(expected['output']),
                                    'hex': expected['output'].hex()}, 'Wrong actual console payload')
                written += len(expected['output'])
            if expected['status'] is not None:
                require(fields(next(cursor), 'RESULT') == {'index': index, 'ordinal': ordinal,
                    'status': expected['status'], 'bytes': expected['bytes']}, 'Wrong RESULT')
        stop = fields(next(cursor), 'STOP')
        reason = 'calllimit' if case in ('calllimit', 'invalidstorm') else 'deadline' if case == 'loop' else 'exit' if calls else 'fault'
        require({k: stop[k] for k in ('index', 'reason', 'code', 'calls', 'written', 'irqs')} == {
            'index': index, 'reason': reason, 'code': 0, 'calls': len(calls), 'written': written,
            'irqs': int(case == 'loop')}, 'Wrong process outcome')
        # BR from this unguarded source sets BTYPE=01 before the NX fetch fault.
        expected_pstate = 0x00400740 if case in ('rwexecute', 'stackexecute') else 0x00400340
        require(stop['spsr'] & ~0xf0000000 == expected_pstate, 'Unsafe saved user PSTATE')
        if reason in ('exit', 'calllimit'):
            require((stop['esr'], stop['far'], stop['elr']) == (0x56000000, 0, calls[-1]['pc']), 'Wrong terminal SVC state')
        elif reason == 'deadline':
            require(stop['esr'] == stop['far'] == 0 and
                    fixture['symbols']['loop_start'] <= stop['elr'] < fixture['symbols']['loop_end'], 'Wrong interrupted loop')
        else:
            expected = fault_contract(case, fixture, symbols)
            require(check_fault(stop['esr'], expected) and stop['far'] == expected['far'] and
                    stop['elr'] == expected['elr'], 'Wrong contained fault syndrome/address')
        require(fields(next(cursor), 'RESTORE') == {'index': index, 'root': symbols['arm_root'],
            'vbar': symbols['arm_vectors'], 'controls': 1, 'inactive': 1,
            'before': baseline, 'after': baseline}, 'Incomplete control/resource restoration')
        require(fields(next(cursor), 'PASS') == {'index': index, 'pass': index // 33, 'case': case}, 'Wrong PASS')
        runs.append({'index': index, 'case': case, 'entry': fixture['entry'], 'calls': calls, 'stop': stop})
    require(fields(next(cursor), 'PREPARE') == {'budget': 1, 'exhausted': 1, 'drop': 1,
        'before': baseline, 'after': baseline}, 'Preparation rollback/drop evidence missing or incorrect')
    require(next(cursor) == 'HMPROC END 66' and next(cursor, None) is None, 'Wrong END or extra report')
    baseline_positions = [i for i, line in enumerate(raw.split(b'\n')) if b'HMTEST' in line]
    require(baseline_positions and min(baseline_positions) > max(positions), 'Baseline missing or before process teardown')
    return {'runs': runs, 'info': info, 'record_count': len(found)}


# This exact grammar was independently reviewed against process-first-qemu raw traces.
def parse_trace(raw):
    require(raw and len(raw) <= MAX_LOG_BYTES and raw.endswith(b'\n'), 'Missing/incomplete/oversized trace')
    lines = [PREFIX.sub('', line) for line in raw.decode('ascii').splitlines()]
    events, i = [], 0
    while i < len(lines):
        line = lines[i]
        match = re.fullmatch(r'Exception return from AArch64 EL1 to AArch64 EL([01]) PC (0x[0-9a-f]+)', line)
        if match:
            events.append({'kind': 'return', 'el': int(match[1]), 'pc': int(match[2], 16), 'line': i})
            i += 1
            continue
        match = re.fullmatch(r'gic_cpu_write cpu 0 iface write at (0x[0-9a-f]+) (0x[0-9a-f]+)', line)
        if match:
            events.append({'kind': 'write', 'offset': int(match[1], 16), 'value': int(match[2], 16), 'line': i})
            i += 1
            continue
        match = re.fullmatch(r'gic_acknowledge_irq cpu 0 acknowledged irq ([0-9]+)', line)
        if match:
            events.append({'kind': 'ack', 'id': int(match[1]), 'line': i})
            i += 1
            continue
        match = re.fullmatch(r'Taking exception ([0-9]+) \[([^\]]+)\] on CPU 0', line)
        require(match is not None, 'Unknown trace line or non-CPU0 event: ' + line[:160])
        event = {'kind': 'exception', 'number': int(match[1]), 'name': match[2], 'line': i}
        require({1: 'Undefined Instruction', 2: 'SVC', 3: 'Prefetch Abort', 4: 'Data Abort',
                 5: 'IRQ', 11: 'Hypervisor Call', 16: 'Semihosting call'}.get(event['number']) == event['name'],
                'Unknown or inconsistent exception number/name')
        i += 1
        require(i < len(lines), 'Incomplete exception source')
        source = re.fullmatch(r'\.\.\.from EL([01]) to EL([12])', lines[i])
        require(source is not None, 'Missing exception source')
        event.update(source=int(source[1]), target=int(source[2]))
        i += 1
        if event['number'] == 16:
            require(event['name'] == 'Semihosting call' and event['source'] == event['target'] == 1 and
                    i < len(lines) and lines[i] == '...handling as semihosting call 0x20', 'Invalid semihosting event')
            i += 1
            events.append(event)
            continue
        require(i < len(lines), 'Incomplete ESR')
        esr = re.fullmatch(r'\.\.\.with ESR (0x[0-9a-f]+)/(0x[0-9a-f]+)', lines[i])
        require(esr is not None, 'Missing ESR')
        event.update(ec=int(esr[1], 16), esr=int(esr[2], 16))
        require(event['ec'] == event['esr'] >> 26, 'ESR class mismatch')
        i += 1
        if event['number'] == 11:
            require(event['name'] == 'Hypervisor Call' and event['source'] == 1 and event['target'] == 2 and
                    event['esr'] == 0x5a000000 and i < len(lines) and lines[i] == '...handled as PSCI call', 'Invalid PSCI transaction')
            i += 1
            events.append(event)
            continue
        if i < len(lines) and lines[i].startswith('...with FAR'):
            far = re.fullmatch(r'\.\.\.with FAR (0x[0-9a-f]+)', lines[i])
            require(far is not None, 'Malformed FAR')
            event['far'] = int(far[1], 16)
            i += 1
        require(i + 1 < len(lines), 'Incomplete ELR/vector')
        elr = re.fullmatch(r'\.\.\.with ELR (0x[0-9a-f]+)', lines[i])
        vector = re.fullmatch(r'\.\.\.to EL1 PC (0x[0-9a-f]+) PSTATE (0x[0-9a-f]+)', lines[i + 1])
        require(elr is not None and vector is not None and event['target'] == 1, 'Wrong ELR/vector')
        event.update(elr=int(elr[1], 16), vector=int(vector[1], 16), pstate=int(vector[2], 16))
        require(event['pstate'] & 0x3df == 0x3c5, 'Unsafe exception target PSTATE')
        i += 2
        events.append(event)
    return events, lines


def validate_process_trace(raw, report, symbols, cpus):
    events, lines = parse_trace(raw)
    position = 0
    def take():
        nonlocal position
        require(position < len(events), 'Incomplete trace inventory')
        value = events[position]
        position += 1
        return value
    def writes(sequence):
        for offset, value in sequence:
            event = take()
            require(event['kind'] == 'write' and (event['offset'], event['value']) == (offset, value), 'Wrong GIC write sequence')
    setup = [(0, 0), (4, 255), (8, 0), (0, 1)]
    writes(setup)
    for _ in range(8):
        event = take()
        require(event['kind'] == 'exception' and event['number'] == 5 and event['name'] == 'IRQ' and event['source'] == 1 and
                event['vector'] == symbols['arm_vectors'] + 0x280 and
                symbols['arm_timer_window_begin'] <= event['elr'] < symbols['arm_timer_window_end'], 'Wrong preflight IRQ')
        ack = take()
        require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong preflight acknowledge')
        writes([(16, 30)])
        returned = take()
        require(returned['kind'] == 'return' and returned['el'] == 1 and returned['pc'] == event['elr'], 'Wrong preflight return')
    writes([(0, 0)] + setup + [(0, 0)])
    preflight_end = events[position - 1]['line'] + 1
    require(validate_timer(('\n'.join(lines[:preflight_end]) + '\n').encode(), 8)['passed'], 'Preflight timer transaction gate failed')
    slice_setup = [(4, 255), (8, 0), (0, 1)]
    slice_restore = [(0, 0), (4, 255), (8, 0), (0, 0)]
    process_hvcs = 0
    for run in report['runs']:
        if cpus == 2:
            for _ in range(2):
                event = take()
                require(event['kind'] == 'exception' and event['number'] == 11, 'Missing peer-OFF PSCI transaction')
                process_hvcs += 1
        writes(slice_setup)
        returned = take()
        require(returned['kind'] == 'return' and returned['el'] == 0 and returned['pc'] == run['entry'], 'Missing real EL0 entry')
        for ordinal, call in enumerate(run['calls'], 1):
            event = take()
            require(event['kind'] == 'exception' and event['name'] == 'SVC' and event['source'] == 0 and
                    event['esr'] == 0x56000000 and event.get('far') is None and event['elr'] == call['pc'] and
                    event['vector'] == symbols['arm_process_vectors'] + 0x400, 'Wrong EL0 SVC transaction')
            writes(slice_restore)
            if ordinal < len(run['calls']):
                writes(slice_setup)
                returned = take()
                require(returned['kind'] == 'return' and returned['el'] == 0 and returned['pc'] == call['pc'], 'Missing exact SVC resume')
        if not run['calls']:
            event = take()
            stop = run['stop']
            require(event['kind'] == 'exception' and event['source'] == 0 and event.get('elr') == stop['elr'], 'Missing EL0 terminal exception')
            irq = run['case'] == 'loop'
            require(event['vector'] == symbols['arm_process_vectors'] + (0x480 if irq else 0x400), 'Wrong lower-EL vector')
            if irq:
                require(event['name'] == 'IRQ' and event['number'] == 5, 'Deadline without EL0 IRQ')
                ack = take()
                require(ack['kind'] == 'ack' and ack['id'] == 30, 'Missing deadline timer acknowledge')
                writes([(16, 30)])
            else:
                expected_name = 'Prefetch Abort' if run['case'] in ('rwexecute', 'stackexecute') else (
                    'Data Abort' if stop['esr'] >> 26 == 0x24 else 'SVC' if run['case'] == 'svcimm' else 'Undefined Instruction')
                require(event['name'] == expected_name and event['esr'] == stop['esr'], 'Wrong terminal trace syndrome')
                require(event.get('far', 0) == stop['far'], 'Trace/serial FAR mismatch')
            writes(slice_restore)
    # Remaining events belong exclusively to the existing EL1 baseline and final exit.
    baseline_start = events[position]['line'] if position < len(events) else len(lines)
    remainder = events[position:]
    require(remainder and remainder[-1]['kind'] == 'exception' and remainder[-1]['number'] == 16, 'Missing final kernel semihost exit')
    for event in remainder[:-1]:
        if event['kind'] == 'exception':
            require(event['source'] == 1 and event['target'] == 1 and event['number'] in (3, 4, 5) and
                    event['vector'] == symbols['arm_vectors'] + (0x280 if event['number'] == 5 else 0x200), 'Unexpected post-process exception')
        elif event['kind'] == 'return':
            require(event['el'] == 1, 'User resumed after terminal result')
        elif event['kind'] == 'ack':
            require(event['id'] == 30, 'Unexpected baseline interrupt')
    baseline = ('\n'.join(lines[baseline_start:]) + '\n').encode()
    require(validate_timer(baseline, 16)['passed'], 'Post-process baseline timer trace failed')
    return {'preflight_irqs': 8, 'process_hvcs': process_hvcs, 'process_runs': len(report['runs']),
            'el0_timer_irqs': 2, 'baseline_irqs': 16, 'trace_grammar': 'QEMU 7.2.22; independently reviewed process-first-qemu traces'}


def validate(console, trace, execution, fixtures, symbols, cpus=1, archive_bytes=None):
    errors, report, trace_report = [], None, None
    checks = {'launched': execution.get('launch_error') is None,
              'within_deadline': execution.get('timed_out') is False,
              'expected_exit': execution.get('returncode') == 0,
              'no_foreign_report': not any(marker in console for marker in
                  (b'HMEXCEPTION', b'HMSPACE', b'HMSMP', b'HMSCHED', b'HMPREEMPT'))}
    try:
        require(type(archive_bytes) is int and 0 < archive_bytes <= 4 * 1024 * 1024, 'Missing verified archive length')
        require(set(fixtures) == set(CASES), 'Wrong fixture inventory')
        require(set(symbols) == set(SYMBOLS) and all(type(v) is int and 0x40000000 <= v < 0x41000000 for v in symbols.values()), 'Invalid kernel symbols')
        require(symbols['arm_root'] % 4096 == 0 and symbols['arm_vectors'] % 2048 == 0 and
                symbols['arm_process_vectors'] % 2048 == 0 and symbols['arm_timer_window_begin'] < symbols['arm_timer_window_end'], 'Invalid kernel symbol geometry')
        report = validate_serial(console, fixtures, symbols, cpus, archive_bytes)
        trace_report = validate_process_trace(trace, report, symbols, cpus)
    except (ValueError, UnicodeError, KeyError, TypeError, StopIteration, IndexError) as error:
        errors.append(str(error) or 'Incomplete process evidence')
    checks['strict_process_evidence'] = not errors
    post = validate_run(console, execution.get('returncode'), arch='aarch64',
                        timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    checks['post_process_baseline'] = post['passed']
    return {'passed': all(checks.values()), 'checks': checks, 'errors': errors,
            'records': report['record_count'] if report else 0, 'trace': trace_report, 'post_process': post,
            'scope': 'archive-backed native ELF EL0 execution/containment/reclamation on pinned QEMU; no Linux ABI claim',
            'trace_grammar_reviewed': TRACE_GRAMMAR_REVIEWED}


def read_symbols(path, output, label, required):
    command = ['readelf', '-sW', str(path)]
    write_json(output / f'{label}.command.json', command)
    proc = subprocess.run(command, capture_output=True, timeout=10)
    (output / f'{label}.stdout.log').write_bytes(proc.stdout)
    (output / f'{label}.stderr.log').write_bytes(proc.stderr)
    require(proc.returncode == 0 and not proc.stderr, 'readelf failed for ' + label)
    found = {name: [] for name in required}
    for line in proc.stdout.decode('ascii').splitlines():
        parts = line.split()
        if len(parts) >= 8 and parts[-1] in found:
            found[parts[-1]].append(int(parts[1], 16))
    require(all(len(values) == 1 for values in found.values()), 'Missing or duplicate ELF symbol')
    return {name: values[0] for name, values in found.items()}


def check_fixture_manifest(path, output, kernel_symbols):
    document = json.loads(path.read_text())
    require(set(document) == {'version', 'kernel_text', 'kernel_data', 'fixtures'} and type(document['version']) is int and document['version'] == 1 and
            set(document['fixtures']) == set(CASES), 'Invalid fixture manifest envelope')
    require(type(document['kernel_text']) is int and type(document['kernel_data']) is int and
            document['kernel_text'] == kernel_symbols['__text_start'] and
            document['kernel_data'] == kernel_symbols['__bss_start'], 'Fixture manifest is bound to a different kernel')
    fixtures = document['fixtures']
    for case in CASES:
        item = fixtures[case]
        require(set(item) == {'file', 'sha256', 'entry', 'pages', 'symbols', 'segments'}, 'Invalid fixture metadata fields')
        file = (path.parent / item['file']).resolve()
        data = file.read_bytes()
        require(64 <= len(data) <= 1024 * 1024 and hashlib.sha256(data).hexdigest() == item['sha256'], 'Fixture hash/size mismatch')
        require(data[:7] == b'\x7fELF\x02\x01\x01' and struct.unpack_from('<HHI', data, 16) == (2, 183, 1), 'Wrong fixture ELF format')
        entry, phoff = struct.unpack_from('<QQ', data, 24)
        phentsize, phnum = struct.unpack_from('<HH', data, 54)
        require(phentsize == 56 and 1 <= phnum <= 16 and phoff + phnum * 56 <= len(data), 'Bad fixture program table')
        require(type(item['entry']) is int and type(item['pages']) is int, 'Non-integer fixture layout')
        pages, executable, loads, segments = set(), [], [], []
        for n in range(phnum):
            kind, flags, offset, va, _, filesz, memsz, align = struct.unpack_from('<IIQQQQQQ', data, phoff + 56 * n)
            require(kind == 1, 'Unexpected fixture program header')
            require(flags in (4, 5, 6) and 0 < memsz and filesz <= memsz and offset + filesz <= len(data) and
                    BASE <= va < va + memsz <= BASE + 0x100000 and offset % 4096 == va % 4096 and
                    align in (0, 1, 4096), 'Invalid fixture load')
            owned = set(range(va // 4096, (va + memsz + 4095) // 4096))
            require(not (pages & owned), 'Fixture LOAD overlap')
            pages |= owned
            segments.append({'flags': flags, 'offset': offset, 'va': va, 'filesz': filesz, 'memsz': memsz,
                             'align': align, 'start': va & ~4095, 'end': (va + memsz + 4095) & ~4095})
            loads.append((va, filesz, offset))
            if flags == 5:
                executable.append((va, va + filesz))
        require(type(item['segments']) is list and item['segments'] == segments and
                all(type(value) is int for segment in item['segments'] for value in segment.values()),
                'Fixture segment metadata differs from ELF')
        require(all(a['end'] <= b['start'] for a, b in zip(segments, segments[1:])), 'Fixture LOAD order')
        require(entry == item['entry'] and len(pages) == item['pages'] and 1 <= len(pages) <= 256 and
                entry % 4 == 0 and any(lo <= entry and entry + 4 <= hi for lo, hi in executable), 'Fixture entry/page mismatch')
        required = set(item['symbols'])
        require(required <= {'write_svc', 'exit_svc', 'fault', 'loop_start', 'loop_end', 'data', 'message',
                              'register_svc_1', 'register_svc_2', 'register_svc_3', 'native_start',
                              'kernel_text', 'kernel_data'}, 'Unknown fixture symbols')
        actual = read_symbols(file, output, 'fixture-' + case, required)
        require(actual == item['symbols'], 'Fixture symbol metadata mismatch')
        require(actual.get('native_start') == entry and actual.get('kernel_text') == kernel_symbols['__text_start'] and
                actual.get('kernel_data') == kernel_symbols['__bss_start'], 'Fixture absolute symbols target a different kernel')
        for name, value in actual.items():
            if name in ('kernel_text', 'kernel_data'):
                require(type(value) is int and 0x40000000 <= value < 0x41000000, 'Invalid absolute kernel symbol')
                continue
            require(type(value) is int and BASE <= value <= BASE + 0x100000, 'Invalid fixture symbol range')
            if name.endswith('svc') or name.startswith('register_svc_') or name in ('fault', 'loop_start'):
                require(value % 4 == 0 and any(lo <= value and value + 4 <= hi for lo, hi in executable), 'Instruction symbol outside executable file bytes')
        instructions = {}
        for name, value in actual.items():
            if name.endswith('svc') or name.startswith('register_svc_') or name in ('fault', 'loop_start'):
                matches = [(offset + value - va) for va, size, offset in loads if va <= value and value + 4 <= va + size]
                require(len(matches) == 1, 'Instruction lacks unique file backing')
                opcode = struct.unpack_from('<I', data, matches[0])[0]
                instructions[name] = f'{opcode:08x}'
                if name.endswith('svc') or name.startswith('register_svc_'):
                    require(opcode == 0xd4000001, 'Native call symbol does not contain svc #0')
                elif name == 'fault' and case in ('svcimm', 'semihost', 'cntpdisable', 'daifmask'):
                    require(opcode == {'svcimm': 0xd4000021, 'semihost': 0xd45e0000,
                        'cntpdisable': 0xd51be23f, 'daifmask': 0xd50342df}[case], 'Wrong deliberate fault instruction')
        write_json(output / ('fixture-' + case + '.instructions.json'), instructions)
        # Required symbol presence is exercised by independently constructed expectations.
        handle = (1 << 32) | 1
        calls = expected_calls(case, item, handle, CASES.index(case))
        if not calls and case != 'loop':
            fault_contract(case, item, {'__text_start': 0x40200000, '__bss_start': 0x40300000})
        if case == 'loop':
            require(actual['loop_start'] < actual['loop_end'] and any(lo <= actual['loop_start'] < actual['loop_end'] <= hi for lo, hi in executable), 'Invalid loop instruction interval')
    return fixtures


def run_profile(kernel, fixtures, symbols, output, memory=128, cpus=1, archive_bytes=None):
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-smp') + 1] = str(cpus)
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-accel') + 1] = 'tcg,thread=multi'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'),
                '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', {'kernel': str(kernel), 'sha256': hashlib.sha256(kernel.read_bytes()).hexdigest(),
               'cpus': cpus, 'ram_mb': memory, 'fixtures': {name: item['sha256'] for name, item in fixtures.items()}})
    execution = {'started_utc': datetime.now(timezone.utc).isoformat(), 'timeout_seconds': TIMEOUT_SECONDS,
                 'returncode': None, 'timed_out': False, 'launch_error': None}
    start = time.monotonic()
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
    execution['elapsed_seconds'] = time.monotonic() - start
    paths = {'serial': output / 'serial.log', 'exceptions': output / 'exceptions.log'}
    artifacts = {name: artifact(path) for name, path in paths.items()}
    result = validate(bounded_bytes(paths['serial']), bounded_bytes(paths['exceptions']), execution, fixtures, symbols, cpus, archive_bytes)
    result['checks']['artifacts_present_and_bounded'] = all(item['exists'] and item['within_size_limit'] for item in artifacts.values())
    result['checks']['trace_grammar_reviewed'] = TRACE_GRAMMAR_REVIEWED
    result['passed'] = all(result['checks'].values())
    result.update(execution, artifacts=artifacts)
    write_json(output / 'result.json', result)
    return result


# Approved after independent inspection of all three process-first-qemu profiles.
# Keep this gate explicit for future changes to emulator version or trace grammar.
TRACE_GRAMMAR_REVIEWED = True


def embedded_archive(kernel, archive, payloads):
    """Locate exact bytes using independently decoded ELF64 program headers."""
    require(len(kernel) >= 64 and kernel[:6] == b'\x7fELF\x02\x01', 'Invalid kernel ELF')
    phoff = struct.unpack_from('<Q', kernel, 32)[0]
    size, count = struct.unpack_from('<HH', kernel, 54)
    require(size == 56 and 0 < count <= 128 and phoff <= len(kernel) and count * size <= len(kernel) - phoff, 'Invalid kernel headers')
    loads, matches = [], []
    for i in range(count):
        kind, flags, offset, va, pa, filesz, memsz, align = struct.unpack_from('<IIQQQQQQ', kernel, phoff + i * size)
        if kind != 1:
            continue
        require(filesz <= memsz and offset <= len(kernel) and filesz <= len(kernel) - offset and
                va + memsz <= MAX_U64 and pa + memsz <= MAX_U64, 'Invalid kernel load interval')
        end = offset + filesz
        require(not any(offset < other_end and other_start < end for other_start, other_end, _ in loads), 'Overlapping kernel file loads')
        loads.append((offset, end, flags))
        position = kernel.find(archive, offset, end)
        while position >= 0:
            matches.append((position, flags))
            position = kernel.find(archive, position + 1, end)
    require(len(matches) == 1, 'Archive must occur exactly once in kernel loads')
    start, flags = matches[0]
    require(flags & 4 and not flags & 2, 'Archive is not read-only')
    stop = start + len(archive)
    for payload in payloads:
        for left, right, _ in loads:
            position = kernel.find(payload, left, right)
            while position >= 0:
                require(start <= position and position + len(payload) <= stop, 'Raw process ELF outside archive')
                position = kernel.find(payload, position + 1, right)
    return start


def check_archive_artifact(path, fixture_manifest, kernel, output):
    import build_initramfs as builder
    entries = builder.process_payloads(fixture_manifest)
    require(path.is_file() and 0 < path.stat().st_size <= builder.MAX_BYTES, 'Missing or oversized archive')
    archive = path.read_bytes()
    require(archive == builder.newc(entries), 'Process archive differs from exact canonical entries')
    expected = builder.manifest(archive, entries)
    expected['archive'] = 'process-initramfs.cpio'
    expected['process_manifest_sha256'] = builder.sha(fixture_manifest.read_bytes())
    require(path.with_suffix('.manifest.json').read_bytes() == (json.dumps(expected, indent=2) + '\n').encode(), 'Process archive manifest mismatch')
    builder.inspect_with_gnu(path, entries, output, 'process-archive')
    offset = embedded_archive(kernel.read_bytes(), archive, [data for name, mode, data in entries if name.startswith('bin/')])
    return {'bytes': len(archive), 'sha256': builder.sha(archive), 'kernel_file_offset': offset,
            'entries': 36, 'files': 34, 'manifest': str(path.with_suffix('.manifest.json'))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True, help='independently generated ELF manifest JSON')
    parser.add_argument('--archive', type=Path, required=True, help='canonical process archive; adjacent .manifest.json required')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    kernel, manifest, output = args.kernel.resolve(), args.fixtures.resolve(), args.output.resolve()
    if not kernel.is_file() or not manifest.is_file() or output.exists():
        parser.error('require built kernel, fixture manifest and fresh evidence directory')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
        fixtures = check_fixture_manifest(manifest, output, symbols)
        archive_report = check_archive_artifact(args.archive.resolve(), manifest, kernel, output)
        write_json(output / 'archive.json', archive_report)
        write_json(output / 'metadata.json', {'manifest': str(manifest), 'sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(),
                                             'fixtures': fixtures, 'symbols': symbols})
        for memory, cpus in ((64, 1), (128, 1), (128, 2)):
            name = f'process-{memory}m-{cpus}cpu'
            profiles[name] = run_profile(kernel, fixtures, symbols, output / name, memory, cpus, archive_report['bytes'])['passed']
    except (OSError, ValueError, UnicodeError, KeyError, TypeError, struct.error, subprocess.TimeoutExpired) as caught:
        error = str(caught)
    result = {'passed': error is None and len(profiles) == 3 and all(profiles.values()),
              'profiles': profiles, 'error': error, 'trace_grammar_reviewed': TRACE_GRAMMAR_REVIEWED}
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
