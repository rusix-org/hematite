#!/usr/bin/env python3
"""Strict ordinary task-creation evidence; negative injection profiles are excluded."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

import build_task_cases as builder
import test_runtime as runtime
from test_process import NUMBER, require

CASES = builder.CASES
# Accepted raw 64/128-MiB traces: docs/reviews/0029-native-task-creation.md,
# "First ordinary matrix hardware evidence accepted". Injection cases excluded.
TRACE_GRAMMAR_REVIEWED = True
MAX_LOG_BYTES = 32 * 1024 * 1024
MAX_TRACE_BYTES = 16 * 1024 * 1024  # Shared reviewed QEMU parser bound.
FIELDS = {
 'PREFLIGHT': 'checks irqs', 'SETUP': 'stage free before after published',
 'INFO': 'wire cpus frequency total_ticks quantum_ticks process_bytes process_pages image_bytes image_pages authority_bytes authority_pages',
 'CASE': 'index pass name before', 'GROUP': 'group limit metadata before after',
 'ROOT': 'group slot generation launch_generation console_generation fixture entry root process_frames box_frames before after',
 'PRESSURE': 'group phase stage before after held',
 'STEP': 'group turn slot generation state calls written irqs remaining charged turns before',
 'CLOCK': 'group turn begin remaining comparator whole_end', 'ENTER': 'group turn pc root',
 'TRAP': 'group turn kind esr far elr spsr', 'CALL': 'group turn ordinal pc version operation arg1 arg2 arg3',
 'COPY': 'group turn part length hex',
 'COMMIT': 'group turn cap_slot cap_generation child_slot child_generation program console_generation entry root process_frames box_frames before after',
 'OUTPUT': 'group turn ordinal length hex',
 'CONSUME': 'group turn child_slot child_generation cap_slot cap_generation outcome',
 'SETTLE': 'group turn kind status result child_slot child_generation cap_slot cap_generation',
 'CHARGE': 'group turn end accounted remaining charged turns',
 'RETURN': 'group turn event terminal state entered code calls written irqs elr remaining charged turns root vbar controls before after',
 'CLOSE': 'group root_slot root_generation end code evidence turn trigger_group trigger_slot trigger_generation',
 'MANAGE': 'group sequence woken reaped remaining_children closing charged',
 'PUBLISH': 'group root_slot root_generation end code evidence turn trigger_group trigger_slot trigger_generation charged',
 'GATE': 'group phase op slot generation result before after charged_before charged_after task_next_before task_next_after console_before console_after',
 'CONTROL': 'group operation state before after', 'REAP_ROOT': 'group outcome before after',
 'DROP': 'group before after', 'PASS': 'index pass name before after',
}
TEXT = {'stage', 'name', 'fixture', 'phase', 'state', 'kind', 'part', 'hex', 'event', 'terminal', 'evidence', 'op'}
TEXT_BY_KIND = {'GATE': {'result'}, 'CONTROL': {'operation'}, 'CLOSE': {'end'}, 'PUBLISH': {'end'}}


def fields(line, kind):
    require(kind in FIELDS, 'Unknown task record')
    names = FIELDS[kind].split()
    text = TEXT | TEXT_BY_KIND.get(kind, set())
    pattern = 'HMTASK ' + kind + ''.join(' ' + key + '=' + ('([0-9a-f]+)' if key == 'hex' else
        '([a-z0-9_]+)' if key in text else NUMBER) for key in names)
    match = re.fullmatch(pattern, line)
    require(match is not None, 'Malformed task ' + kind)
    row = {key: value if key in text else int(value) for key, value in zip(names, match.groups())}
    require(all(value < 2**64 for value in row.values() if type(value) is int), 'Task integer exceeds u64')
    if 'hex' in row:
        require(row['length'] in range(1, 257) and len(row['hex']) == row['length'] * 2, 'Unbounded/partial copied bytes')
    return row


class Records:
    def __init__(self, raw):
        require(len(raw) <= MAX_LOG_BYTES, 'Task serial exceeds bound')
        self.rows, self.positions, active, ended = [], [], False, False
        lines = raw.split(b'\n')
        for index, value in enumerate(lines):
            value = value.removesuffix(b'\r')
            if value.startswith(b'HMTASK '):
                require(not ended and index < len(lines) - 1, 'Extra/unterminated task record')
                self.rows.append(value.decode('ascii')); self.positions.append(index)
                ended = value == b'HMTASK END 50'
                active = not ended
            elif active or b'HMTASK' in value or b'HMPROC' in value or b'HMRT ' in value:
                raise ValueError('Foreign output inside task report')
        self.index = 0
    def kind(self):
        require(self.index < len(self.rows), 'Incomplete task report')
        return self.rows[self.index].split()[1]
    def take(self, kind):
        require(self.kind() == kind, 'Expected task ' + kind + ', got ' + self.kind())
        row = fields(self.rows[self.index], kind); self.index += 1
        return row
    def literal(self, value):
        require(self.index < len(self.rows) and self.rows[self.index] == value, 'Missing ' + value)
        self.index += 1


def scoped(records, kind, step):
    row = records.take(kind)
    require((row['group'], row['turn']) == (step['group'], step['turn']), 'Cross-owner turn record')
    return row


WORKER_BYTES = b'worker complete\n'


def spawn(alias, program=1, payload=WORKER_BYTES, **changes):
    value = dict(op=3, site='spawn_svc', alias=alias, program=program,
                 payload=None if program == 2 else payload, console=True, frames=128, rights=3, status=0)
    value.update(changes)
    return value


def wait(alias, outcome=55 << 8, **changes):
    value = dict(op=4, site='wait_svc', alias=alias, mode=0, status=0, result=outcome)
    value.update(changes)
    return value


def cancel(alias, result=0, **changes):
    value = dict(op=5, site='cancel_svc', alias=alias, status=0, result=result)
    value.update(changes)
    return value


def write(payload=b'', site='raw_svc'):
    return dict(op=1, site=site, payload=payload, status=0, result=len(payload))


def exit_call(code=0): return dict(op=2, site='exit_svc', code=code, status=0, result=0)


def script(name, child_frames):
    """Fixed source-independent request inventory; aliases bind actual prior grants."""
    finish = [exit_call()]
    if name == 'supervisor':
        return [spawn('a'), spawn('b', 2), spawn(None, 2, status=9), wait('a'), cancel('b'),
                cancel('b', 1), wait('b', 5), write(b'supervisor complete\n', 'write_svc')] + finish
    if name == 'copy_inputs':
        result = []
        for index, payload in enumerate((None, b'', bytes(range(256)))):
            result += [spawn(str(index), payload=payload, mutable=True), write(site='overwrite_svc'),
                       wait(str(index), (10 if payload is None else 11 if not payload else 55) << 8)]
        return result + [write(b'copy complete\n', 'write_svc')] + finish
    if name == 'wait_poll_ready':
        return [spawn('a', 2), wait('a', 0, mode=1, status=10, site='raw_svc'), cancel('a'), cancel('a', 1),
                wait('a', 5), wait('a', 0, status=3, site='raw_svc'), cancel('a', 0, status=3, site='raw_svc')] + finish
    if name == 'wait_budget_child': return [spawn('a', 2), wait('a', 2)] + finish
    if name == 'rights_console_policy':
        return [spawn(None, status=8), spawn('a', payload=None, console=False), wait('a', 10 << 8)] + finish
    if name == 'rights_console_wait_only':
        return [spawn('a', payload=None, console=False, rights=1), cancel('a', 0, status=8, site='raw_svc'), wait('a', 10 << 8)] + finish
    if name == 'foreign_authority':
        return [spawn('a', 2), spawn('b', payload='foreign'), wait('b', 0), cancel('a'), wait('a', 5)] + finish
    if name == 'stale_reuse':
        return [spawn('a'), wait('a'), spawn('b'), wait('a', 0, status=3, site='raw_svc'),
                cancel('a', 0, status=3, site='raw_svc'), wait('b')] + finish
    if name == 'malformed_outer':
        bad = [(2, 3, 'zero', 0, 0, 1), (1, 99, 'zero', 0, 0, 2),
               (1, 3, 'zero', 2**64-1, 0, 3), (1, 3, 'console', 0, 64, 3),
               (1, 3, 'unknown', 0, 64, 3), (1, 3, 'launch', 0, 63, 5),
               (1, 3, 'launch', 0, 65, 5), (1, 4, 'console', 99, 99, 3), (1, 5, 'launch', 0, 0, 3)]
        return [dict(op=op, site='raw_svc', version=v, authority=a, address=b, length=c, status=status, result=0, outer=True)
                for v, op, a, b, c, status in bad] + [spawn('a'), wait('a')] + finish
    if name == 'malformed_descriptor':
        return [spawn(None, status=5 if i == 14 else 4 if i == 15 else 7, mutation=i) for i in range(17)] + [spawn('a'), wait('a')] + finish
    if name == 'copy_faults':
        result = [dict(op=3, site='spawn_svc', authority='launch', address=address, length=64, status=4, result=0, outer=True)
                  for address in (0, 2**64-32, 0x1001fffe1)]
        result += [spawn(None, status=4, bad_input=value) for value in ((0, 1), (2**64-1, 2), (0x1001fffff, 2))]
        return result + [spawn('a', payload=bytes(range(256)), unaligned=True), wait('a')] + finish
    if name == 'child_fault': return [spawn('a', payload=b'HMFAULT\0'), spawn('b'), wait('a', 1), wait('b')] + finish
    if name == 'held_capacity':
        return [spawn('a', 2), spawn('b', 2), cancel('a'), cancel('b'), spawn(None, status=9), wait('a', 5),
                spawn('c'), wait('a', 0, status=3, site='raw_svc'), wait('b', 5), wait('c')] + finish
    if name == 'child_frame_cap': return [spawn(None, frames=child_frames-1, status=9), spawn('a', frames=child_frames), wait('a')] + finish
    if name == 'group_frame_cap': return [spawn('a'), spawn(None, status=9), wait('a'), spawn('b'), wait('b')] + finish
    if name == 'allocation_rollback': return [spawn(None, status=9, pressure=i) for i in range(3)] + [spawn('a'), wait('a')] + finish
    if name in ('parent_exit', 'parent_fault', 'parent_budget', 'parent_cancel_ready', 'parent_call_limit'):
        result = [spawn('a'), spawn('b', 2)]
        if name == 'parent_exit': result += [exit_call(17)]
        if name == 'parent_call_limit': result += [write() for _ in range(61)] + [spawn(None, limit=True)]
        return result
    if name in ('parent_cancel_waiting', 'drop_waiting'): return [spawn('a', 2), wait('a', None)]
    if name == 'limit_spawn': return [write() for _ in range(63)] + [spawn(None, limit=True)]
    if name in ('limit_wait', 'limit_cancel'):
        return [spawn('a', 2)] + [write() for _ in range(62)] + [
            (wait('a', None, site='raw_svc', limit=True) if name == 'limit_wait' else cancel('a', site='raw_svc', limit=True))]
    raise ValueError('Unknown fixture script: ' + name)


def child_script(payload, console):
    if payload == b'HMFAULT\0': return []
    if payload is not None and len(payload) == 32 and payload[:8] == b'HMAUTH\0\0':
        launch, sibling, parent_console = struct.unpack_from('<3Q', payload, 8)
        return [dict(op=op, site='raw_svc', authority=a, address=b, length=c, status=3, result=0, outer=True)
                for op, a, b, c in ((3, launch, 0, 64), (4, sibling, 0, 0), (5, sibling, 0, 0),
                    (1, parent_console, 0, 0), (4, console, 0, 0), (5, console, 0, 0), (3, 0, 0, 64), (4, 0, 0, 0))] + [exit_call()]
    result = [write(site=f'worker_yield_{i}') for i in range(1, 5)] + [write(payload or b'', 'write_svc')] if console else []
    return result + [exit_call(10 if payload is None else 11 if not payload else 55)]


def packed(terminal, code=0):
    return code << 8 if terminal == 'exit' else {'fault': 1, 'budget': 2, 'call_limit': 3, 'turn_limit': 4, 'cancelled': 5, 'unavailable': 6}[terminal]


def expected_end(case):
    return {'parent_exit': ('exit', 17), 'parent_fault': ('fault', 0), 'parent_budget': ('budget', 0),
        'parent_call_limit': ('call_limit', 0), 'parent_cancel_ready': ('cancelled', 0),
        'parent_cancel_waiting': ('cancelled', 0), 'limit_spawn': ('call_limit', 0),
        'limit_wait': ('call_limit', 0), 'limit_cancel': ('call_limit', 0)}.get(case, ('exit', 0))


def descriptor_bytes(spec, actual, fixture, payload):
    """Use only pointer locations from actual bytes; every semantic byte is fixed."""
    require(len(actual) == 64, 'Descriptor is not full64')
    pointer = struct.unpack_from('<Q', actual, 16)[0]
    if spec.get('mutation') == 15: pointer = fixture['symbols']['worker_message']
    length = 0 if payload is None else len(payload)
    if spec.get('bad_input') is not None:
        pointer, length = spec['bad_input']
    elif payload is None or not payload:
        pointer = 0
    else:
        require(0x100000000 <= pointer < 0x100200000 and pointer + length <= 0x100200000, 'Unexpected input pointer')
    expected = bytearray(struct.pack('<IIIIQQQI', 1, 64, spec['program'], int(payload is not None) | (int(spec['console']) << 1),
                                     pointer, length, spec['frames'], spec['rights']) + bytes(20))
    mutation = spec.get('mutation')
    if mutation is not None:
        if mutation == 0: expected[0] = 2
        elif mutation == 1: expected[4] = 63
        elif mutation == 2: expected[12] |= 4
        elif mutation in (3, 4, 5): expected[(44, 48, 56)[mutation-3]] = 1
        elif mutation in (6, 7): expected[40] = 0 if mutation == 6 else 2
        elif mutation in (8, 9): expected[32:40] = struct.pack('<Q', 0 if mutation == 8 else 129)
        elif mutation == 10: expected[12] = 2; expected[24:32] = bytes(8)
        elif mutation == 11: expected[24:32] = bytes(8)
        elif mutation == 12: expected[8:12] = bytes(4)
        elif mutation == 13: expected[8] = 3
        elif mutation == 14: expected[24:32] = struct.pack('<Q', 257)
        elif mutation == 15: expected[16:24] = struct.pack('<Q', 2**64-1)
        elif mutation == 16: expected[0] = 2; expected[24:32] = struct.pack('<Q', 257)
    require(actual == expected, 'Descriptor differs from fixed request/mutation')
    return pointer


def check_fault(task, fixture, trap):
    require((task['fixture'] == 'parent_fault' or task['payload'] == b'HMFAULT\0') and
            trap['esr'] == 0x9200004f and trap['far'] == fixture['entry'] and
            trap['elr'] == fixture['symbols']['fault'] and trap['spsr'] == 0x00400340,
            'Wrong lower-EL own-RX write fault')


class Model:
    def __init__(self, records, fixtures, symbols, layout, info, baseline):
        self.r, self.fixtures, self.symbols, self.layout, self.info = records, fixtures, symbols, layout, info
        self.baseline, self.free = baseline, baseline
        self.k = layout['fields']['box_backing_pages']
        self.metadata = layout['task_fields']['metadata_pages']
        # Six actual failed root constructions precede BEGIN. Issuance is burned
        # at real reservation points, while last-prepared differs from next console.
        self.next_group, self.next_task, self.next_console, self.last_console = 7, 5, 4, 3
        self.last_clock = 0
        self.slices, self.history = [], []
        self.group = None
        self.case_steps = self.pass_steps = 0

    def fixture(self, name): return self.fixtures['large_root' if name == 'group_frame_cap' else name]
    def charged(self):
        g = self.group
        return (0 if g['reaped'] else self.metadata) + sum(t['frames'] for t in g['slots'].values() if t['live'])
    def balance(self): require(self.free == self.baseline - self.group['pressure'] - self.charged(), 'Frame conservation failed')
    def identity(self, row, task):
        require(row['group'] == self.group['id'] and row['slot'] == task['slot'] and row['generation'] == task['generation'], 'Wrong task identity')
    def new_task(self, name, slot, generation, root, console, payload, commands):
        f = self.fixture(name)
        require(root != self.symbols['arm_root'] and root % 4096 == 0 and 0x40000000 <= root < 0x41000000 and
                all(not t['live'] or t['root'] != root for t in self.group['slots'].values()), 'Invalid/aliased live root')
        task = dict(fixture=name, slot=slot, generation=generation, root=root, console=console, payload=payload,
            frames=f['pages']+29+self.k, pc=f['entry'], elr=f['entry'], calls=0, written=0, irqs=0, remaining=self.info['total_ticks'],
            charged=0, turns=0, ordinal=0, live=True, cancel=False, waiting=None, terminal=None, code=0,
            commands=commands, aliases={}, markers=[], poll_irqs=0, history=[])
        self.group['slots'][slot] = task
        return task

    def begin(self, fixture):
        row = self.r.take('GROUP')
        f = self.fixture(fixture)
        charge = self.metadata + f['pages'] + 29 + self.k
        require(row == dict(group=self.next_group, limit=512, metadata=self.metadata, before=self.baseline, after=self.baseline-charge), 'Wrong group admission/metadata charge')
        self.group = dict(id=self.next_group, slots={}, caps={}, next_slots=[1]*4, cursor=0, sequence=0,
                          closure=None, published=False, reaped=False, pressure=0, gates=[], controls=[], consumes=[], commits=[], pressures=[])
        self.next_group += 1
        row = self.r.take('ROOT')
        require(row['group'] == self.group['id'] and row['slot'] == 0 and row['generation'] == 1 and row['fixture'] == fixture and
                row['entry'] == f['entry'] and row['process_frames'] == f['pages']+29 and row['box_frames'] == self.k and
                row['before'] == self.baseline and row['after'] == self.baseline-charge and
                row['launch_generation'] == self.next_task and row['console_generation'] == self.next_console, 'Wrong root identity/layout/issuance')
        self.group['launch'] = self.next_task << 32 | 2
        self.next_task += 1
        self.last_console = self.next_console; self.next_console += 1
        self.new_task(fixture, 0, 1, row['root'], self.last_console << 32 | 1, None,
                      script(fixture, self.fixtures['worker']['pages']+29+self.k))
        self.free = row['after']; self.balance()

    def consume(self, cap_slot):
        cap = self.group['caps'].pop(cap_slot)
        task = self.group['slots'].pop(cap['slot'])
        require(not task['live'] and task['terminal'] is not None, 'Consumed nonterminal child')
        self.group['next_slots'][task['slot']] += 1
        self.group['consumes'].append((cap_slot, cap['generation'], task['slot'], task['generation']))
        return task

    def request(self, task, step):
        call = scoped(self.r, 'CALL', step)
        ordinal = task['calls'] + 1
        require(ordinal <= len(task['commands']), 'Unexpected application call')
        spec = dict(task['commands'][ordinal-1])
        f = self.fixture(task['fixture'])
        require(call['ordinal'] == ordinal and call['pc'] == f['symbols'][spec['site']] + 4 and
                call['pc'] == task['pc'] and call['version'] == spec.get('version', 1) and call['operation'] == spec['op'], 'Wrong application SVC/version/operation')
        task['calls'] = ordinal
        authority = {'zero': 0, 'console': task['console'], 'launch': self.group['launch'], 'unknown': 0xffffffff00000002}
        if spec.get('outer'):
            expected_a = authority.get(spec['authority'], spec['authority'])
            require((call['arg1'], call['arg2'], call['arg3']) == (expected_a, spec['address'], spec['length']), 'Wrong malformed/foreign outer scalars')
        elif spec['op'] == 1:
            require(call['arg1'] == task['console'] and call['arg3'] == len(spec['payload']), 'Wrong console grant/length')
            if spec['site'] in ('raw_svc', 'overwrite_svc') or spec['site'].startswith('worker_yield_'):
                require(call['arg2'] == 2**64-1, 'Zero WRITE must carry the fixed invalid pointer')
            else: require(0x100000000 <= call['arg2'] <= 0x100200000 - len(spec['payload']), 'Payload pointer outside user range')
        elif spec['op'] == 2:
            require((call['arg1'], call['arg2'], call['arg3']) == (spec['code'], 0, 0), 'Wrong application exit scalars')
        elif spec['op'] == 3:
            require(call['arg1'] == self.group['launch'] and call['arg3'] == 64 and 0x100000000 <= call['arg2'] <= 0x100200000-64, 'Wrong launch authority/descriptor range')
            if spec.get('unaligned'): require(call['arg2'] == f['symbols']['descriptor_storage'] + 1, 'Unaligned descriptor missing')
        else:
            token = task['aliases'][spec['alias']]
            require((call['arg1'], call['arg2'], call['arg3']) == (token, spec.get('mode', 0), 0), 'Wrong completion authority/scalars')
        if spec.get('limit'):
            require(ordinal == 64, 'Task limit at wrong shared call')
            settle = scoped(self.r, 'SETTLE', step)
            require(settle == dict(group=step['group'], turn=step['turn'], kind='terminate', status=0, result=0,
                                  child_slot=0, child_generation=0, cap_slot=0, cap_generation=0), '64th task operation had an effect/reply')
            return 'call_limit', 0
        payload = spec.get('payload')
        if payload == 'foreign':
            payload = b'HMAUTH\0\0' + struct.pack('<3Q', self.group['launch'], task['aliases']['a'], task['console'])
        if spec['op'] == 3 and not spec.get('outer'):
            row = scoped(self.r, 'COPY', step)
            require(row['part'] == 'descriptor' and row['length'] == 64, 'Missing complete descriptor copy')
            pointer = descriptor_bytes(spec, bytes.fromhex(row['hex']), f, payload)
            if spec.get('unaligned'): require(pointer == f['symbols']['input_storage']+1, 'Unaligned input missing')
            # Descriptor/policy/frame errors precede input copy. Physical pressure
            # occurs after the input was fully copied into a bounded owned value.
            copied_input = spec['status'] in (0, 9)
            if copied_input and payload:
                row = scoped(self.r, 'COPY', step)
                require(row['part'] == 'input' and bytes.fromhex(row['hex']) == payload, 'Copied input differs from exact original job')
            if spec['status'] == 0:
                row = scoped(self.r, 'COMMIT', step)
                slots = [i for i in range(1, 4) if i not in self.group['slots']]
                cap_slots = [i for i in (3, 4) if i not in self.group['caps']]
                require(slots and cap_slots, 'Commit exceeded held capacity')
                slot, cap_slot = slots[0], cap_slots[0]
                name = ('large_worker' if self.group['case'] == 'group_frame_cap' else 'worker') if spec['program'] == 1 else ('large_loop' if self.group['case'] == 'group_frame_cap' else 'loop')
                child_f = self.fixture(name)
                frames = child_f['pages'] + 29 + self.k
                console_gen = self.next_console if spec['console'] else 0
                require(row == dict(group=step['group'], turn=step['turn'], cap_slot=cap_slot, cap_generation=self.next_task,
                    child_slot=slot, child_generation=self.group['next_slots'][slot], program=spec['program'], console_generation=console_gen,
                    entry=child_f['entry'], root=row['root'], process_frames=frames-self.k, box_frames=self.k, before=self.free, after=self.free-frames), 'Wrong committed child identity/charge/issuance')
                token = self.next_task << 32 | cap_slot
                self.next_task += 1
                if console_gen: self.last_console = self.next_console
                self.next_console += 1
                self.group['caps'][cap_slot] = dict(generation=token >> 32, slot=slot, rights=spec['rights'])
                child_console = console_gen << 32 | 1 if console_gen else 0
                child = self.new_task(name, slot, row['child_generation'], row['root'], child_console, payload,
                                      child_script(payload, child_console) if spec['program'] == 1 else [])
                self.group['commits'].append(child)
                task['aliases'][spec['alias']] = token
                self.free -= frames
                spec['result'] = token
            elif spec['status'] == 9 and 'pressure' not in spec:
                child_name = 'large_worker' if self.group['case'] == 'group_frame_cap' else 'worker' if spec['program'] == 1 else 'loop'
                required = self.fixture(child_name)['pages'] + 29 + self.k
                require(len(self.group['caps']) == 2 or required > spec['frames'] or self.charged() + required > 512, 'Resources reply lacks actual held/logical cap cause')
            elif 'pressure' in spec:
                self.next_task += 1
                if spec['pressure'] > 0:
                    if spec['pressure'] == 2: self.last_console = self.next_console
                    self.next_console += 1
        if spec['op'] == 1 and not spec.get('outer'):
            if spec['payload']:
                row = scoped(self.r, 'OUTPUT', step)
                require(row['ordinal'] == ordinal and bytes.fromhex(row['hex']) == spec['payload'], 'Output bytes do not prove actual fixture work')
            task['written'] += len(spec['payload'])
            if spec['site'] == 'overwrite_svc':
                child = self.group['commits'][-1]
                require(child['calls'] < 4, 'Child passed copy boundary before source overwrite')
                task['markers'].append(ordinal)
        consume = None
        if spec['op'] == 4 and not spec.get('outer') and spec['status'] == 0:
            token = task['aliases'][spec['alias']]
            cap = self.group['caps'].get(token & 0xffffffff)
            require(cap is not None and cap['generation'] == token >> 32, 'WAIT unexpectedly lacks owned capability')
            target = self.group['slots'][cap['slot']]
            if target['terminal'] is not None:
                require(packed(target['terminal'], target['code']) == spec['result'], 'Wrong waited child outcome')
                consume = scoped(self.r, 'CONSUME', step)
                require(consume == dict(group=step['group'], turn=step['turn'], child_slot=target['slot'], child_generation=target['generation'],
                                       cap_slot=token & 0xffffffff, cap_generation=token >> 32, outcome=spec['result']), 'Wrong immediate completion consumption')
                self.consume(token & 0xffffffff)
            else:
                require(spec['mode'] == 0, 'Polling unexpectedly parked')
                task['waiting'] = (token & 0xffffffff, spec['result'])
        settle = scoped(self.r, 'SETTLE', step)
        pending = task['waiting'] if spec['op'] == 4 and spec['status'] == 0 else None
        if pending:
            cap = self.group['caps'][pending[0]]; target = self.group['slots'][cap['slot']]
            wanted = dict(kind='park', status=0, result=0, child_slot=target['slot'], child_generation=target['generation'],
                          cap_slot=pending[0], cap_generation=cap['generation'])
        else:
            wanted = dict(kind='terminate' if spec['op'] == 2 else 'reply', status=spec['status'], result=spec.get('result', 0),
                          child_slot=0, child_generation=0, cap_slot=0, cap_generation=0)
        require(settle == dict(group=step['group'], turn=step['turn'], **wanted), 'Wrong owned settlement/reply')
        if spec['op'] == 5 and not spec.get('outer') and spec['status'] == 0:
            cap = self.group['caps'][task['aliases'][spec['alias']] & 0xffffffff]
            target = self.group['slots'][cap['slot']]
            require(int(not target['live']) == spec['result'], 'CANCEL completion state differs')
            if target['live']: target['cancel'] = True
        if spec['op'] == 2: return 'exit', spec['code']
        return None, 0

    def management(self):
        g, root = self.group, self.group['slots'][0]
        reaped, woken = 0, 0
        if g['closure'] is not None:
            for cap_slot, cap in list(g['caps'].items()):
                if not g['slots'][cap['slot']]['live']:
                    self.consume(cap_slot); reaped += 1
        elif root['waiting'] is not None and not root['cancel']:
            cap_slot, outcome = root['waiting']
            cap = g['caps'][cap_slot]
            target = g['slots'][cap['slot']]
            if not target['live']:
                require(packed(target['terminal'], target['code']) == outcome, 'Wake installed wrong child completion')
                self.consume(cap_slot); reaped = woken = 1
                root['waiting'] = None
        g['sequence'] += 1
        expected = dict(group=g['id'], sequence=g['sequence'], woken=woken, reaped=reaped,
                        remaining_children=len(g['caps']), closing=int(g['closure'] is not None), charged=self.charged())
        require(self.r.take('MANAGE') == expected, 'Wrong completion management/parked wake')
        if woken: root['wakes'] = root.get('wakes', 0) + 1
        return expected

    def step(self):
        g = self.group
        selected = next((i for n in range(4) if (i := (g['cursor'] + n) % 4) in g['slots'] and
                         g['slots'][i]['live'] and (g['slots'][i]['cancel'] or g['slots'][i]['waiting'] is None)), None)
        require(selected is not None and not g['published'], 'Step without runnable owner')
        task = g['slots'][selected]; f = self.fixture(task['fixture'])
        row = self.r.take('STEP'); before = self.free
        require(row == dict(group=g['id'], turn=task['ordinal']+1, slot=selected, generation=task['generation'],
                            state='cancel_pending' if task['cancel'] else 'ready', calls=task['calls'], written=task['written'],
                            irqs=task['irqs'], remaining=task['remaining'], charged=task['charged'], turns=task['turns'], before=before),
                'Selected owner/order/counter snapshot differs')
        task['ordinal'] += 1; g['cursor'] = (selected+1) % 4
        self.case_steps += 1; self.pass_steps += 1
        require(self.case_steps <= 1024 and self.pass_steps <= 8192 and task['turns'] < 256, 'Exceeded bounded ordinary execution')
        clock = enter = trap = None
        terminal, code, event = ('cancelled' if task['cancel'] else None), 0, 'none'
        if not task['cancel']:
            clock = scoped(self.r, 'CLOCK', row)
            start = clock['begin']
            require(self.last_clock <= start <= 2**56-1 and clock['remaining'] == task['remaining'] and
                    clock['whole_end'] == start + task['remaining'] <= 2**56-1 and
                    clock['comparator'] == start + min(task['remaining'], self.info['quantum_ticks']), 'Invalid actual counter/deadline')
            if self.r.kind() == 'ENTER':
                enter = scoped(self.r, 'ENTER', row)
                require(enter['pc'] == task['pc'] and enter['root'] == task['root'], 'Restarted or wrong-root task entry')
                trap = scoped(self.r, 'TRAP', row)
                require(trap['kind'] in ('svc', 'irq', 'fault') and trap['spsr'] & ~0xf0000000 == 0x00400340, 'Unexpected trap/PSTATE')
                task['pc'] = task['elr'] = trap['elr']
                if trap['kind'] == 'svc':
                    require(trap['esr'] == 0x56000000 and trap['far'] == 0, 'Wrong SVC syndrome')
                    terminal, code = self.request(task, row); event = 'svc'
                elif trap['kind'] == 'irq':
                    require(runtime.executable(f, trap['elr']), 'IRQ outside actual admitted RX')
                    task['irqs'] += 1; event = 'timer'
                    if trap['elr'] == f['symbols']['loop_start']: task['poll_irqs'] += 1
                else:
                    check_fault(task, f, trap)
                    terminal, event = 'fault', 'fault'
                self.slices.append(dict(enter=enter, trap=trap))
            charge = scoped(self.r, 'CHARGE', row)
            require(start <= charge['end'] <= 2**56-1 and charge['accounted'] == 1, 'Invalid/rejected ordinary finish')
            if trap is not None and trap['kind'] == 'irq':
                require(charge['end'] >= clock['comparator'], 'Timer IRQ completed before programmed comparator')
            elapsed = min(charge['end'] - start, task['remaining'])
            task['remaining'] -= elapsed; task['charged'] += elapsed; task['turns'] += 1
            self.last_clock = charge['end']
            require((charge['remaining'], charge['charged'], charge['turns']) ==
                    (task['remaining'], task['charged'], task['turns']), 'Owner budget charged parked/foreign interval')
            if terminal is None and task['remaining'] == 0: terminal = 'budget'
            if enter is None: require(terminal == 'budget', 'No-entry noncancel turn without real exhaustion')
        if terminal == 'budget':
            require(task['fixture'] == 'parent_budget' or (task['fixture'] in ('loop', 'large_loop') and g['case'] == 'wait_budget_child'),
                    'Unexpected earlier budget termination')
            require(task['poll_irqs'] >= 2, 'Budget did not involve repeated real loop preemption')
        if terminal is not None:
            task['live'] = False; task['terminal'] = terminal; task['code'] = code
            self.free += task['frames']
            task['waiting'] = None
            if selected == 0:
                require((terminal, code) == expected_end(task['fixture']), 'Wrong root closure outcome')
                require(task['calls'] == len(task['commands']), 'Root terminated before exact script completed')
                g['closure'] = dict(group=g['id'], root_slot=0, root_generation=task['generation'], end=terminal,
                    code=code, evidence='executed', turn=task['ordinal'], trigger_group=0, trigger_slot=0, trigger_generation=0)
                for child in g['slots'].values():
                    if child['slot'] and child['live']: child['cancel'] = True
            elif terminal == 'exit': require(task['calls'] == len(task['commands']), 'Child success without exact actual calls')
        ret = scoped(self.r, 'RETURN', row)
        state = 'closing' if selected == 0 and terminal else 'completed' if terminal else 'waiting' if task['waiting'] else 'ready'
        require(ret == dict(group=g['id'], turn=task['ordinal'], event=event, terminal=terminal or 'none', state=state,
            entered=int(enter is not None), code=code, calls=task['calls'], written=task['written'], irqs=task['irqs'],
            elr=task['elr'], remaining=task['remaining'], charged=task['charged'], turns=task['turns'],
            root=self.symbols['arm_root'], vbar=self.symbols['arm_vectors'], controls=1, before=before, after=self.free),
            'Finalized task state/counters/controls/resources differ')
        if selected == 0 and terminal:
            require(self.r.take('CLOSE') == g['closure'], 'Missing immutable executed root closure')
        managed = self.management()
        self.balance()
        task['history'].append(dict(step=row, trap=trap, returned=ret, manage=managed))
        return task

    def publish(self):
        g = self.group
        require(g['closure'] is not None and not g['caps'] and not g['published'], 'Premature/duplicate root publication')
        self.management()
        require(self.r.take('PUBLISH') == dict(g['closure'], charged=self.metadata), 'Publication changed saved closure/charge')
        g['published'] = True

    def gate(self):
        g = self.group
        row = self.r.take('GATE')
        phase = row['phase']; root = g['slots'][0]
        require(row['group'] == g['id'] and row['before'] == row['after'] == self.free and
                row['charged_before'] == row['charged_after'] == self.charged() and
                row['task_next_before'] == row['task_next_after'] == self.next_task and
                row['console_before'] == row['console_after'] == self.last_console, 'Mode gate mutated resources/issuers')
        target = g['slots'].get(row['slot'])
        require(target is not None and row['generation'] == target['generation'], 'Gate uses invented identity')
        if phase == 'active': require(root['calls'] == 0 and root['ordinal'] == 0 and row['slot'] == 0, 'Active gate too late')
        elif phase == 'child': require(row['slot'] != 0 and target['ordinal'] == 1, 'Child gate not at first actual child step')
        elif phase == 'waiting': require(root['waiting'] is not None and root['calls'] == 2 and row['slot'] == 0, 'Missing actual parked gate')
        elif phase == 'closing': require(g['closure'] is not None and not g['published'] and row['slot'] == 0, 'Closing gate outside closure')
        elif phase == 'closed': require(g['reaped'] and row['slot'] == 0, 'Closed gate before task teardown')
        else: raise ValueError('Unknown mode gate phase')
        expected = 'none' if phase in ('waiting', 'closing') and row['op'] == 'task_completion' else 'pending' if phase in ('waiting', 'closing') else 'idle' if row['op'] == 'turn' else 'mode'
        require(row['result'] == expected, 'Wrong mode gate error')
        g['gates'].append((phase, row['op']))

    def pressure(self):
        row = self.r.take('PRESSURE'); g = self.group
        require(g['case'] == 'allocation_rollback' and row['group'] == g['id'], 'Unexpected physical pressure')
        ordinal = len(g['pressures']); phase = ordinal // 2
        require(phase < 3 and row['stage'] == ('load', 'startup', 'box')[phase] and
                row['phase'] == ('hold' if ordinal % 2 == 0 else 'release'), 'Pressure stage/order changed')
        leave = (0, self.fixtures['worker']['pages']+28, self.fixtures['worker']['pages']+29)[phase]
        require(row['before'] == self.free, 'Pressure before count differs')
        if ordinal % 2 == 0:
            require(g['pressure'] == 0 and row['after'] == leave and row['held'] == self.free-leave and
                    g['slots'][0]['calls'] == phase, 'Wrong actual allocation failure depth')
            g['pressure'] = row['held']
        else:
            require(row['before'] == leave and row['held'] == g['pressure'] and row['after'] == self.free+g['pressure'] and
                    g['slots'][0]['calls'] == phase+1, 'Pressure released before failed actual SPAWN')
            g['pressure'] = 0
        self.free = row['after']; g['pressures'].append(row); self.balance()

    def control(self):
        g, root = self.group, self.group['slots'][0]
        row = self.r.take('CONTROL')
        require(not g['controls'] and row['group'] == g['id'] and row['before'] == self.free and root['calls'] == 2,
                'Unexpected/repeated driver control')
        if g['case'] == 'parent_cancel_ready':
            require(row['operation'] == 'cancel_root' and row['state'] == 'ready' and root['waiting'] is None and
                    root['history'][-1]['returned']['event'] == 'svc', 'Cancel not at second-SPAWN finalized boundary')
        elif g['case'] in ('parent_cancel_waiting', 'drop_waiting'):
            require(row['operation'] == ('drop' if g['case'] == 'drop_waiting' else 'cancel_root') and
                    row['state'] == 'waiting' and root['waiting'] is not None, 'Control lacks actual Waiting owner')
        else: raise ValueError('Control in unsupported case')
        if row['operation'] == 'cancel_root':
            require(row['after'] == self.free, 'Cancellation eagerly freed frames')
            root['cancel'] = True
        else:
            require(row['after'] == self.baseline, 'Inactive group drop did not reclaim all owners/metadata')
            g['drop_before'] = self.free
            self.free = self.baseline
        g['controls'].append(row)

    def reap_root(self):
        g = self.group
        require(g['published'] and not g['reaped'] and not g['caps'] and self.charged() == self.metadata, 'Reap before complete teardown')
        closure = g['closure']
        require(self.r.take('REAP_ROOT') == dict(group=g['id'], outcome=packed(closure['end'], closure['code']),
                                               before=self.free, after=self.baseline), 'Wrong root reap outcome/metadata release')
        g['reaped'] = True; self.free = self.baseline; self.balance()

    def finish_group(self):
        g, root = self.group, self.group['slots'][0]
        require(self.r.take('DROP') == dict(group=g['id'], before=g.get('drop_before', self.free), after=self.baseline), 'Wrong group drop count')
        require(self.free == self.baseline and (g['reaped'] or 'drop_before' in g), 'Group destroyed before required publication/reap')
        gates = []
        if g['case'] == 'supervisor' and not g['recovery']:
            gates = [('active', op) for op in ('admit', 'completion', 'reap')]
            gates += [('child', op) for op in ('info', 'cancel', 'task_completion', 'task_reap')]
            gates += [('closing', op) for op in ('task_completion', 'task_reap')]
            gates += [('closed', op) for op in ('admit', 'info', 'completion', 'reap', 'cancel', 'task_completion', 'task_reap', 'task_root', 'turn')]
        elif g['case'] == 'wait_budget_child':
            gates = [('waiting', op) for op in ('task_completion', 'task_reap')]
            require(root.get('wakes') == 1, 'Missing actual budget-child parked wake')
        require(g['gates'] == gates, 'Missing/extra/reordered mode gates')
        require(len(g['controls']) == int(g['case'] in ('parent_cancel_ready', 'parent_cancel_waiting', 'drop_waiting')), 'Missing exact driver control')
        require(len(g['pressures']) == (6 if g['case'] == 'allocation_rollback' else 0), 'Missing physical failure stages')
        if g['case'] == 'copy_inputs': require(len(root['markers']) == 3, 'Missing actual source-overwrite markers')
        if g['case'] == 'drop_waiting': require(root['calls'] == 2 and root['waiting'] is not None and g['closure'] is None, 'Drop manufactured completion/wake')
        self.history.append(dict(case=g['case'], root=root, commits=g['commits'], gates=g['gates'], controls=g['controls']))
        self.group = None


def validate_serial(raw, fixtures, symbols, witness):
    r = Records(raw)
    require(r.take('PREFLIGHT') == dict(checks=15, irqs=8), 'Missing actual timer preflight')
    fields_layout, task_layout = witness['fields'], witness['task_fields']
    k, images = fields_layout['box_backing_pages'], task_layout['image_pages'] * 2
    n = fixtures['supervisor']['pages'] + 28
    leaves = (0, task_layout['image_pages'], images, images+n, images+n+1, images+n+1+k)
    baseline = None
    for name, leave in zip(('image1', 'image2', 'root_load', 'root_startup', 'root_box', 'authority'), leaves):
        row = r.take('SETUP')
        if baseline is None: baseline = row['before']
        require(300 < baseline < 4096 and row == dict(stage=name, free=leave, before=baseline, after=baseline, published=0),
                'Wrong actual setup pressure depth/recovery')
    r.literal('HMTASK BEGIN 50')
    info = r.take('INFO')
    require(info['wire'] == info['cpus'] == 1 and info['frequency'] > 0 and
            info['total_ticks'] == info['frequency']//10 and info['quantum_ticks'] == info['frequency']//1000 > 0 and
            all(info[key] == source[key] for source, keys in ((fields_layout, ('process_bytes',)),
                (task_layout, ('image_bytes', 'image_pages', 'authority_bytes', 'authority_pages'))) for key in keys) and
            info['process_pages'] == k, 'Changed wire/actual layout/timing contract')
    model = Model(r, fixtures, symbols, witness, info, baseline)
    cases = []
    for index in range(50):
        case = CASES[index % 25]
        if index % 25 == 0: model.pass_steps = 0
        model.case_steps = 0
        require(r.take('CASE') == dict(index=index, **{'pass': index//25}, name=case, before=baseline), 'Missing/reordered case')
        roots = ('rights_console_policy', 'rights_console_wait_only') if case == 'rights_console' else ('drop_waiting', 'supervisor') if case == 'drop_waiting' else (case,)
        for group_index, fixture in enumerate(roots):
            model.begin(fixture)
            model.group['case'] = fixture
            model.group['recovery'] = case == 'drop_waiting' and group_index == 1
            while model.group is not None:
                kind = r.kind()
                if kind == 'STEP': model.step()
                elif kind == 'MANAGE': model.publish()
                elif kind == 'GATE': model.gate()
                elif kind == 'PRESSURE': model.pressure()
                elif kind == 'CONTROL': model.control()
                elif kind == 'REAP_ROOT': model.reap_root()
                elif kind == 'DROP': model.finish_group()
                else: raise ValueError('Unexpected task-group record: ' + kind)
        require(r.take('PASS') == dict(index=index, **{'pass': index//25}, name=case, before=baseline, after=baseline), 'Wrong/missing PASS or leaked resources')
        cases.append(dict(name=case, steps=model.case_steps))
    r.literal('HMTASK END 50')
    require(r.index == len(r.rows), 'Extra task report')
    baseline_lines = [i for i, line in enumerate(raw.split(b'\n')) if line.startswith(b'HMTEST ')]
    require(baseline_lines and min(baseline_lines) > r.positions[-1], 'Bootstrap baseline precedes task END')
    return dict(info=info, cases=cases, slices=model.slices, groups=model.history, records=len(r.rows))


def validate(console, trace, execution, fixtures, symbols, fixups, layout):
    errors, report, traced = [], None, None
    try:
        require(execution.get('returncode') == 0 and execution.get('timed_out') is False and execution.get('launch_error') is None,
                'Task guest failed or exceeded deadline')
        report = validate_serial(console, fixtures, symbols, layout)
        require(len(trace) <= MAX_TRACE_BYTES, 'Task trace exceeds shared reviewed 16MiB parser bound')
        traced = runtime.validate_trace(trace, report, symbols, fixups)
        require(TRACE_GRAMMAR_REVIEWED, 'Actual task matrix trace grammar awaits independent review')
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration, IndexError) as error:
        errors.append(str(error) or 'Incomplete task evidence')
    baseline = runtime.validate_run(console, execution.get('returncode'), arch='aarch64',
        timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    return dict(passed=not errors and baseline['passed'], errors=errors, baseline=baseline, trace=traced,
                records=report['records'] if report else 0, scope='ordinary task creation; no injected negative profiles')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('kernel', 'fixtures', 'archive', 'large-archive', 'layout', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args(); output = args.output.resolve()
    if output.exists(): parser.error('preserve failures; use fresh output')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        _, layout = runtime.verify_layout(args.layout.resolve())
        kernel = args.kernel.resolve()
        fixtures, archives = builder.verify(args.fixtures.resolve(), args.archive.resolve(), args.large_archive.resolve(),
                                            args.layout.resolve(), output, kernel)
        symbols = runtime.read_symbols(kernel, output, 'kernel-symbols', runtime.SYMBOLS)
        fixups = runtime.fixup_table(kernel.read_bytes(), symbols)
        (output / 'metadata.json').write_text(json.dumps(dict(archives=archives, layout=layout, symbols=symbols,
            sources={name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest() for name in
                     ('test_task_cases.py', 'build_task_cases.py', 'test_runtime.py', 'test_launcher.py', 'test_process.py')}), indent=2) + '\n')
        # Reuse bounded QEMU execution only; its module-level validator is passed
        # explicitly by a local wrapper, never by a global monkeypatch.
        inputs = dict(fixtures_sha256=hashlib.sha256(args.fixtures.read_bytes()).hexdigest(),
                      layout_sha256=hashlib.sha256(args.layout.read_bytes()).hexdigest(), archives=archives)
        for memory in (64, 128):
            profiles[f'tasks-{memory}m'] = run_profile(output / f'tasks-{memory}m', kernel, fixtures, symbols, fixups, memory, layout['result'], inputs)['passed']
    except (OSError, ValueError, KeyError, TypeError, UnicodeError, struct.error) as caught:
        error = str(caught)
    result = dict(passed=error is None and len(profiles) == 2 and all(profiles.values()), profiles=profiles, error=error,
                  scope='50 ordinary cases per RAM profile; detailed injection tests excluded')
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n'); print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


def run_profile(output, kernel, fixtures, symbols, fixups, memory, layout, inputs):
    import subprocess
    import time
    from test_kernel import qemu_command
    from test_boot_profiles import write_json
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'), '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', dict(kernel=str(kernel), kernel_sha256=hashlib.sha256(kernel.read_bytes()).hexdigest(),
        ram_mb=memory, cpus=1, timeout_seconds=60, max_serial_bytes=MAX_LOG_BYTES, max_trace_bytes=MAX_TRACE_BYTES, **inputs))
    execution = dict(returncode=None, timed_out=False, launch_error=None)
    started = time.monotonic()
    paths = [output / name for name in ('serial.log', 'stderr.log', 'exceptions.log')]
    with paths[0].open('wb') as stdout, paths[1].open('wb') as stderr:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr) as process:
                while process.poll() is None:
                    if any(p.exists() and p.stat().st_size > (MAX_TRACE_BYTES if p.name == 'exceptions.log' else MAX_LOG_BYTES) for p in paths):
                        execution['launch_error'] = 'Task artifact size exceeded'
                        process.kill(); process.wait(); break
                    remaining = 60 - (time.monotonic() - started)
                    if remaining <= 0:
                        execution['timed_out'] = True
                        process.kill(); process.wait(); break
                    try: process.wait(timeout=min(remaining, 0.25))
                    except subprocess.TimeoutExpired: pass
                execution['returncode'] = process.returncode
        except OSError as error: execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - started
    logs = {p.name: p.read_bytes() if p.exists() and p.stat().st_size <= (MAX_TRACE_BYTES if p.name == 'exceptions.log' else MAX_LOG_BYTES) else b'' for p in paths}
    result = validate(logs['serial.log'], logs['exceptions.log'], execution, fixtures, symbols, fixups, layout)
    result['execution'] = execution
    result['artifacts'] = {p.name: dict(exists=p.exists(), bytes=p.stat().st_size if p.exists() else 0,
        sha256=hashlib.sha256(logs[p.name]).hexdigest(), within_size_limit=p.exists() and p.stat().st_size <= (MAX_TRACE_BYTES if p.name == 'exceptions.log' else MAX_LOG_BYTES)) for p in paths}
    result['passed'] &= all(v['within_size_limit'] for v in result['artifacts'].values())
    write_json(output / 'result.json', result)
    return result



if __name__ == "__main__": sys.exit(main())
