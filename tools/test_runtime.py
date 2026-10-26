#!/usr/bin/env python3
"""Strict owned-group records and actual ARM transitions; no metadata-only pass."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

from test_process import require, NUMBER, read_symbols
from test_launcher import TraceReader, SLICE_SETUP, SLICE_RESTORE, SYMBOLS as BASE_SYMBOLS, fixup_table
from test_kernel import validate_run

CASES = ('svc_context', 'irq_context', 'busy_peer', 'byte_budget', 'call_limit', 'fault_peer',
         'cancel_new', 'cancel_reply', 'cancel_quantum', 'completion_tokens', 'admission_rollback', 'drop_live')
SYMBOLS = tuple(dict.fromkeys(BASE_SYMBOLS + ('arm_root',)))
# Independent review of unmodified runtime-third ARM traces: see
# docs/reviews/0028-native-process-runtime.md.
TRACE_GRAMMAR_REVIEWED = True
MAX_LOG_BYTES = 32 * 1024 * 1024
FIELDS = {
 'PREFLIGHT': 'checks irqs', 'INFO': 'wire cpus frequency total_ticks quantum_ticks box_bytes box_frames',
 'CASE': 'index pass name before', 'GROUP': 'group limit before',
 'ADMIT': 'group slot generation console_generation fixture entry root process_frames box_frames before after',
 'ACTION': 'group id_group op slot generation result before after',
 'WORD': 'group id_group slot generation value',
 'STEP': 'group turn slot generation state calls written irqs remaining charged turns before',
 'CLOCK': 'group turn begin remaining comparator whole_end', 'ENTER': 'group turn pc root',
 'TRAP': 'group turn kind esr far elr spsr', 'CALL': 'group turn ordinal pc version operation arg1 arg2 arg3',
 'OUTPUT': 'group turn ordinal length hex', 'RESULT': 'group turn ordinal status bytes',
 'CHARGE': 'group turn end accounted remaining charged turns',
 'RETURN': 'group turn event terminal entered code calls written irqs elr remaining charged turns root vbar controls before after',
 'IDLE': 'group before after', 'DROP': 'group before after', 'PASS': 'index pass name before after',
}
TEXT = {'name', 'fixture', 'op', 'result', 'state', 'kind', 'event', 'terminal', 'hex'}


def fields(line, kind):
    names = FIELDS[kind].split()
    pattern = 'HMRT ' + kind + ''.join(' ' + key + '=' + ('([0-9a-f]+)' if key == 'hex' else
        '([a-z0-9_]+)' if key in TEXT else NUMBER) for key in names)
    match = re.fullmatch(pattern, line)
    require(match is not None, 'Malformed runtime ' + kind)
    row = {key: value if key in TEXT else int(value) for key, value in zip(names, match.groups())}
    require(all(value < 2**64 for value in row.values() if type(value) is int), 'Runtime integer exceeds u64')
    return row


def capture_bytes(name):
    seed = 1 if name.endswith('_a') else 2
    words = [0x1111000000000000 | seed << 8 | i for i in range(31)]
    if name.startswith('context_svc'):
        for i, value in ((0, 0), (1, 0), (2, 2**64 - 1), (3, 0), (8, 1)): words[i] = value
    else:
        scratch, address = (17, 18) if seed == 1 else (18, 17)
        words[scratch], words[address] = 1, 0x100002000
    words += [0x1001ffc00, 0xa0000000 if seed == 1 else 0x50000000]
    for i in range(32):
        words += [0x2222000000000000 | seed << 8 | i, 0x3333000000000000 | seed << 8 | i]
    words += [seed << 22, 0x08000000 | seed, 0x4444000000000000 | seed, 0, 0, 0, 0]
    return struct.pack('<104Q', *words)


def executable(fixture, pc):
    return pc % 4 == 0 and any(s['flags'] == 5 and s['va'] <= pc < s['va'] + s['filesz'] for s in fixture['segments'])


def call_contract(task, ordinal, fixtures):
    """Expected request/reply bytes derived independently from each actual fixture."""
    name, handle = task['fixture'], task['console_generation'] << 32 | 1
    syms = fixtures[name]['symbols']
    call = dict(version=1, operation=1, arg1=handle, arg2=2**64 - 1, arg3=0,
                pc=syms['write_svc'] + 4, output=b'', status=0, bytes=0)
    exit_at = None
    if name.startswith('context_'):
        offset = ordinal - 1
        if name.startswith('context_svc'):
            if ordinal == 1: call['pc'] = syms['context_svc'] + 4
            offset -= 1
        if 0 <= offset < 4:
            payload = capture_bytes(name)[offset * 256:(offset + 1) * 256]
            call.update(arg2=syms['capture'] + offset * 256, arg3=len(payload), output=payload, bytes=len(payload))
        exit_at = 6 if name.startswith('context_svc') else 5
    elif name == 'hello':
        if ordinal == 1: call.update(arg2=syms['message'], arg3=15, output=b'hello from EL0\n', bytes=15)
        exit_at = 2
    elif name == 'grant':
        if ordinal == 1: call.update(arg1=task['foreign_grant'], status=3)
        elif ordinal == 2: call.update(arg2=syms['grant_message'], arg3=12, output=b'grant owned\n', bytes=12)
        exit_at = 3
    elif name == 'byte_budget':
        if ordinal <= 16: call.update(arg2=syms['pattern'], arg3=256, output=bytes(range(256)), bytes=256)
        elif ordinal == 17: call.update(arg2=syms['pattern'], arg3=1, status=6)
        exit_at = 19
    elif name in ('call_limit', 'yield'):
        require(ordinal <= (64 if name == 'call_limit' else 1), 'Unexpected resumed call-limit/yield fixture')
    else:
        raise ValueError('Non-calling fixture issued SVC')
    if exit_at is not None:
        require(ordinal <= exit_at, 'Extra fixture call')
        if ordinal == exit_at:
            call.update(operation=2, arg1=0, arg2=0, arg3=0, pc=syms['exit_svc'] + 4, output=b'', status=None, bytes=0)
    return call


class Records:
    def __init__(self, raw):
        require(len(raw) <= MAX_LOG_BYTES, 'Runtime console too large')
        self.rows, self.positions = [], []
        active = False
        lines = raw.split(b'\n')
        for position, value in enumerate(lines):
            value = value.removesuffix(b'\r')
            if value.startswith(b'HMRT '):
                require(position < len(lines) - 1, 'Unterminated runtime record')
                active = value != b'HMRT END 24'
                self.rows.append(value.decode('ascii')); self.positions.append(position)
            elif active or b'HMRT' in value or b'HMPROC' in value:
                raise ValueError('Foreign/malformed output inside runtime report')
        self.index = 0
    def kind(self):
        require(self.index < len(self.rows), 'Incomplete runtime report')
        return self.rows[self.index].split()[1]
    def take(self, kind):
        require(self.kind() == kind, 'Expected runtime ' + kind + ', got ' + self.kind())
        line = self.rows[self.index]; self.index += 1
        return fields(line, kind)
    def literal(self, value):
        require(self.index < len(self.rows) and self.rows[self.index] == value, 'Missing ' + value)
        self.index += 1


def scoped(records, kind, step):
    row = records.take(kind)
    require((row['group'], row['turn']) == (step['group'], step['turn']), 'Cross-owner step record')
    return row


def check_pstate(task, fixture, trap):
    expected = 0x00400340
    if task['fixture'].startswith('context_'):
        symbols = fixture['symbols']
        seen = task.setdefault('control_setters_seen', set())
        for label, bit in (('context_set_dit', 24), ('context_set_ssbs', 12)):
            # These setters occur once, on the straight-line sentinel setup path.
            # Numeric ranges exclude Rust startup/capture helpers elsewhere in RX.
            reached = (symbols[label] < trap['elr'] <= symbols['context_svc'] + 4 or
                symbols['poll_start'] <= trap['elr'] <= symbols['poll_end'])
            if reached and label not in seen:
                expected |= 1 << bit
                seen.add(label)
    require(trap['spsr'] & ~0xf0000000 == expected, 'Unsafe/unsanitized runtime saved PSTATE')


def release_allowed(task):
    last = task['history'][-1] if task['history'] else None
    return (task['provenance'] and task['terminal'] is None and not task['cancel'] and not task['released'] and
        task['irqs'] >= 2 and last is not None and last['trap'] is not None and last['trap']['kind'] == 'irq' and
        0x100000100 <= last['trap']['elr'] < 0x100000108)


def step_record(records, group, info, fixtures, symbols, free):
    step = records.take('STEP')
    slot = step['slot']
    tasks = group['slots']
    live = [i for i in range(4) if i in tasks and tasks[i]['terminal'] is None]
    require(live, 'Selected completed/empty group')
    selected = next((group['cursor'] + n) % 4 for n in range(4) if (group['cursor'] + n) % 4 in live)
    require(slot == selected, 'Round-robin selection differs')
    task = tasks[slot]
    require(step == dict(group=group['group'], turn=task['ordinal'] + 1, slot=slot, generation=task['generation'],
        state='cancel_pending' if task['cancel'] else 'live', before=free,
        **{k: task[k] for k in ('calls', 'written', 'irqs', 'remaining', 'charged', 'turns')}), 'STEP reset counters/identity/resources')
    group['cursor'] = (slot + 1) % 4
    task['ordinal'] += 1
    require(task['ordinal'] <= 257, 'Unbounded owner selections')
    clock = entered = trap = call = None
    next_counters = {k: task[k] for k in ('calls', 'written', 'irqs', 'remaining', 'charged', 'turns')}
    if records.kind() == 'CLOCK':
        clock = scoped(records, 'CLOCK', step)
        start = clock['begin']
        require(not task['cancel'] and task['turns'] < 256 and task['remaining'] > 0 and start >= task['last_end'] and
            clock == dict(group=step['group'], turn=step['turn'], begin=start, remaining=task['remaining'],
                comparator=start + min(task['remaining'], info['quantum_ticks']), whole_end=start + task['remaining']) and
            clock['whole_end'] <= 2**56 - 1, 'Invalid actual budget begin/deadline')
        if records.kind() == 'ENTER':
            entered = scoped(records, 'ENTER', step)
            require(entered['pc'] == task['pc'] and entered['root'] == task['root'] and executable(fixtures[task['fixture']], entered['pc']), 'Wrong owner saved-PC resume')
            trap = scoped(records, 'TRAP', step)
            check_pstate(task, fixtures[task['fixture']], trap)
            require(executable(fixtures[task['fixture']], trap['elr']), 'Trap PC outside admitted code')
            if trap['kind'] == 'svc':
                require(trap['esr'] == 0x56000000, 'Wrong SVC syndrome')
                if records.kind() == 'CALL':
                    call = scoped(records, 'CALL', step)
                    ordinal = task['calls'] + 1
                    expected = call_contract(task, ordinal, fixtures)
                    require(call == dict(group=step['group'], turn=step['turn'], ordinal=ordinal,
                        **{k: expected[k] for k in ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')}) and
                        call['pc'] == trap['elr'], 'Wrong persistent native CALL')
                    next_counters['calls'] += 1
                    if expected['output']:
                        require(scoped(records, 'OUTPUT', step) == dict(group=step['group'], turn=step['turn'],
                            ordinal=ordinal, length=len(expected['output']), hex=expected['output'].hex()), 'Actual output/context bytes differ')
                        next_counters['written'] += len(expected['output'])
                    if expected['status'] is not None:
                        require(scoped(records, 'RESULT', step) == dict(group=step['group'], turn=step['turn'], ordinal=ordinal,
                            status=expected['status'], bytes=expected['bytes']), 'Wrong native reply')
            elif trap['kind'] == 'irq':
                next_counters['irqs'] += 1
                syms = fixtures[task['fixture']]['symbols']
                if syms['poll_start'] <= trap['elr'] < syms['poll_end']:
                    task['poll_irqs'] += 1
                elif task['fixture'].startswith('context_irq') and task['poll_irqs'] == 0:
                    task['setup_irqs'] += 1
                    require(task['setup_irqs'] <= 8, 'Context setup IRQ bound exceeded')
            elif trap['kind'] == 'fault':
                require(task['fixture'] == 'fault' and trap['esr'] == 0x9200004f and trap['far'] == fixtures['fault']['entry'] and
                    trap['elr'] == fixtures['fault']['symbols']['fault'] and trap['spsr'] == 0x00400340, 'Unexpected process fault')
            else: raise ValueError('Unexpected runtime trap')
        charge = scoped(records, 'CHARGE', step)
        end = charge['end']
        require(clock['begin'] <= end <= 2**56 - 1, 'Backward/out-of-range actual finish')
        if trap and trap['kind'] == 'irq': require(end >= clock['comparator'], 'Timer IRQ preceded its programmed compare')
        delta = min(end - clock['begin'], task['remaining'])
        next_counters.update(remaining=task['remaining'] - delta, charged=task['charged'] + delta, turns=task['turns'] + 1)
        require(charge == dict(group=step['group'], turn=step['turn'], end=end, accounted=1,
            **{k: next_counters[k] for k in ('remaining', 'charged', 'turns')}), 'Charge reset/parked charge/incorrect conservation')
        task['last_end'] = end
    result = scoped(records, 'RETURN', step)
    terminal = result['terminal']
    require(terminal in ('none', 'exit', 'fault', 'budget', 'call_limit', 'turn_limit', 'cancelled'), 'Unacceptable terminal result')
    event = {'svc': 'svc', 'irq': 'timer', 'fault': 'fault'}[trap['kind']] if trap else 'none'
    pc = trap['elr'] if trap else task['pc']
    after = free + (task['process_frames'] + task['box_frames'] if terminal != 'none' else 0)
    require(result == dict(group=step['group'], turn=step['turn'], event=event, terminal=terminal,
        entered=int(entered is not None), code=0, elr=pc, root=symbols['arm_root'], vbar=symbols['arm_vectors'],
        controls=1, before=free, after=after, **next_counters), 'Wrong return accounting/control restoration')
    if task['cancel']: require(terminal == 'cancelled' and clock is None, 'Cancellation entered/charged/not completed')
    elif task['turns'] == 256: require(terminal == 'turn_limit' and clock is None, 'Turn limit re-entered')
    else:
        require(clock is not None, 'Selected live process omitted budget begin')
        expected_end = 'fault' if event == 'fault' else 'exit' if call and call['operation'] == 2 else \
            'call_limit' if call and task['fixture'] == 'call_limit' and call['ordinal'] == 64 else \
            'budget' if next_counters['remaining'] == 0 else 'none'
        require(terminal == expected_end, 'Terminal precedence differs')
        require(entered is not None or terminal == 'budget', 'Unexplained no-entry turn')
        require(event != 'svc' or call is not None or terminal == 'budget', 'SVC omitted native dispatch without expiry')
    task.update(next_counters, pc=pc, terminal=None if terminal == 'none' else terminal)
    task['history'].append(dict(step=step, clock=clock, enter=entered, trap=trap, call=call, result=result))
    return after, task['history'][-1]


def action_contract(case, tasks, actions, groups, words):
    def action(task, op, result='ok', target=None):
        return (task['group'] if target is None else target, task['group'], task['slot'], task['generation'], op, result)
    def reject(group, result): return (group['group'], 0, 0, 0, 'admit_reject', result)
    actual = [(a['group'], a['id_group'], a['slot'], a['generation'], a['op'], a['result']) for a in actions]
    reaps = [action(t, 'reap') for t in tasks]
    expected = reaps
    expected_words = 0
    if case.startswith('cancel_'):
        expected = [action(tasks[0], 'cancel')] * 2 + reaps
    elif case == 'irq_context':
        a, b, ordinary = tasks
        prefix = [action(ordinary, 'release', 'release_denied'), action(ordinary, 'cancel')]
        completed = min((a, b), key=lambda t: t['completion_sequence'])
        middle = [action(t, op, result) for t in (a, b) for op, result in
            (('release', 'ok'), ('release', 'release_denied'))] + [action(completed, 'release', 'release_denied')]
        require(actual[:2] == prefix and actual[-3:] == reaps and sorted(actual[2:-3]) == sorted(middle),
            'Wrong exact polling release/cancel/completed-refusal actions')
        for index, row in enumerate(actual):
            if row[-2:] == ('release', 'ok'):
                require(actual[index + 1] == row[:-1] + ('release_denied',), 'Successful release lacks immediate double-release refusal')
        denials = [a for a in actions if a['op'] == 'release' and a['result'] == 'release_denied' and a['slot'] != 2]
        require(sum(a['_target_terminal'] == 'exit' for a in denials) == 1, 'Missing actual completed-owner release refusal')
        expected = actual
        expected_words = 12
    elif case == 'completion_tokens':
        a, b, c, d, replacement, probe, hello = tasks
        expected = [reject(groups[0], 'full')]
        expected += [action(t, 'query') for t in (a, b, c, d) for _ in range(2)]
        expected += [action(a, 'reap')]
        expected += [action(a, op, 'stale') for op in ('cancel', 'query', 'reap', 'release')]
        expected += [action(replacement, 'reap', 'pending')]
        expected += [action(t, 'reap') for t in (replacement, b, c, d)]
        expected += [action(replacement, op, 'stale', groups[1]['group']) for op in ('cancel', 'query', 'reap', 'release')]
        expected += [action(hello, 'release', 'release_denied')] * 2
        expected += [action(probe, 'release'), action(probe, 'release', 'release_denied')]
        expected += [action(t, 'reap') for t in (probe, hello)]
        hello_denials = [a for a in actions if a['group'] == hello['group'] and a['slot'] == hello['slot'] and a['op'] == 'release']
        require([a['_target_terminal'] for a in hello_denials] == [None, 'exit'], 'Noncontext/completed refusal states differ')
        expected_words = 12
    elif case == 'admission_rollback':
        expected = [reject(group, 'resources') for group in groups] + reaps
    elif case == 'drop_live':
        expected = [action(tasks[-1], 'reap')]
        expected_words = 2
    require(actual == expected, 'Extra/missing/reordered scenario actions: ' + case)
    require(len(words) == expected_words, 'Extra/missing fixed-word observations: ' + case)


def case_contract(case, tasks, actions, groups, info):
    names = [t['fixture'] for t in tasks]
    expected = {
        'svc_context': ['context_svc_a', 'context_svc_b'],
        'irq_context': ['context_irq_a', 'context_irq_b', 'context_irq_a'],
        'busy_peer': ['loop', 'hello'], 'byte_budget': ['byte_budget', 'grant'],
        'call_limit': ['call_limit', 'hello'], 'fault_peer': ['fault', 'hello'],
        'cancel_new': ['loop', 'hello'], 'cancel_reply': ['yield', 'hello'],
        'cancel_quantum': ['loop', 'hello'],
        'completion_tokens': ['hello'] * 4 + ['grant', 'context_irq_a', 'hello'],
        'admission_rollback': ['hello'], 'drop_live': ['context_irq_a', 'context_irq_b', 'hello'],
    }
    require(names == expected[case], 'Wrong case admission inventory: ' + case)
    require(len(groups) == (2 if case in ('completion_tokens', 'drop_live') else 4 if case == 'admission_rollback' else 1), 'Wrong group inventory')
    for index, task in enumerate(tasks):
        name = task['fixture']
        cancelled = (case.startswith('cancel_') and index == 0) or (case == 'irq_context' and index == 2)
        dropped = case == 'drop_live' and index < 2
        end = 'cancelled' if cancelled else 'dropped' if dropped else 'budget' if name == 'loop' else 'call_limit' if name == 'call_limit' else 'fault' if name == 'fault' else 'exit'
        require(task['terminal'] == end, 'Fixture did not reach its required outcome')
        expected_calls = 0 if name in ('loop', 'fault') or dropped or (cancelled and name != 'yield') else \
            1 if name == 'yield' else 2 if name == 'hello' else 3 if name == 'grant' else 19 if name == 'byte_budget' else \
            64 if name == 'call_limit' else 6 if name.startswith('context_svc') else 5
        expected_written = 0 if cancelled or dropped or name in ('loop', 'fault', 'call_limit', 'yield') else \
            15 if name == 'hello' else 12 if name == 'grant' else 4096 if name == 'byte_budget' else 832
        require(task['calls'] == expected_calls and task['written'] == expected_written, 'Incomplete persistent call/byte inventory')
        entered = [h for h in task['history'] if h['enter']]
        if case == 'cancel_new' and index == 0:
            require(task['console_generation'] == 0 and not entered, 'No-grant TaskId cancellation not tested')
        elif task['console_generation'] == 0: raise ValueError('Unexpected ungranted task')
        if cancelled:
            require(sum(a['op'] == 'cancel' and a['result'] == 'ok' and
                (a['group'], a['slot'], a['generation']) == (task['group'], task['slot'], task['generation']) for a in actions) >= 2 if case.startswith('cancel_') else True,
                'Missing repeated cancellation')
            if case == 'cancel_reply':
                require(entered and entered[-1]['trap']['kind'] == 'svc' and sum(h['trap']['kind'] == 'svc' for h in entered) == 1, 'Cancelled reply resumed')
            else: require(len(entered) == (1 if case == 'cancel_quantum' else 0), 'Cancelled task entered again')
        if name.startswith('context_irq') and not cancelled:
            require(task['poll_irqs'] >= (1 if dropped else 2), 'Missing actual poll IRQ preservation proof')
            require(task['released'] != dropped, 'Missing release or released dropped task')
        if case == 'busy_peer' and index == 0:
            require(task['irqs'] >= 2 and task['remaining'] == 0, 'Loop lacks repeated real quantum/budget evidence')
    if case == 'busy_peer':
        require(tasks[1]['completion_sequence'] < tasks[0]['completion_sequence'], 'Busy task prevented peer completion')
    if case == 'completion_tokens':
        require(any(a['op'] == 'admit_reject' and a['result'] == 'full' for a in actions), 'Held completions did not reject admission')
        for op in ('cancel', 'query', 'reap', 'release'):
            require(any(a['op'] == op and a['id_group'] != a['group'] and a['result'] in ('stale', 'release_denied') for a in actions), 'Missing foreign-owner rejection: ' + op)
        for op in ('cancel', 'query', 'reap'):
            require(any(a['op'] == op and a['id_group'] == a['group'] and a['result'] == 'stale' for a in actions), 'Missing stale-slot rejection: ' + op)
        require(any(a['op'] == 'reap' and a['result'] == 'pending' for a in actions), 'Missing live reap rejection')
        for task in tasks[:4]:
            require(sum(a['op'] == 'query' and a['result'] == 'ok' and a['group'] == task['group'] and a['slot'] == task['slot'] and a['generation'] == task['generation'] for a in actions) == 2, 'Held completion not queried twice')
    if case == 'admission_rollback':
        failures = [a for a in actions if a['op'] == 'admit_reject']
        require(len(failures) == 4 and all(a['result'] == 'resources' for a in failures), 'Missing actual allocation rollback stages')
        n = tasks[0]['process_frames'] - 1
        require([a['before'] for a in failures[1:]] == [0, n, n + 1], 'Wrong real pressure for load/startup/box failure')
    if case == 'irq_context':
        require(sum(a['op'] == 'release' and a['result'] == 'release_denied' for a in actions) >= 3, 'Missing no-provenance/double release refusal')


def validate_serial(raw, fixtures, symbols, box_bytes):
    records = Records(raw)
    require(records.take('PREFLIGHT') == dict(checks=15, irqs=8), 'Wrong runtime preflight')
    records.literal('HMRT BEGIN 24')
    info = records.take('INFO')
    require(info['wire'] == info['cpus'] == 1 and 1000 <= info['frequency'] <= 10**9 and
        info['total_ticks'] == info['frequency'] // 10 and info['quantum_ticks'] == info['frequency'] // 1000 and
        info['box_bytes'] == box_bytes and info['box_frames'] == (box_bytes + 4095) // 4096, 'Wrong budget/compiler layout witness')
    require(0 < box_bytes <= 16384, 'Invalid actual Process size')
    baseline = last_group = console_generation = 0
    slices, cases, all_tasks = [], [], []
    pass_turns = [0, 0]
    sequence = 0
    counter_end = 0
    for index in range(24):
        name = CASES[index % 12]
        opening = records.take('CASE')
        if index == 0: baseline = opening['before']
        require(285 <= baseline <= 4096 and opening == dict(index=index, **{'pass': index // 12}, name=name, before=baseline), 'Wrong case order/baseline')
        free, group = baseline, None
        tasks, actions, groups, words = [], [], [], []
        case_turns = 0
        burned = 0
        while records.kind() != 'PASS':
            kind = records.kind()
            if kind == 'GROUP':
                row = records.take(kind)
                require(group is None and row['group'] > last_group and row['before'] == free == baseline and 1 <= row['limit'] <= 512, 'Invalid group creation')
                last_group = row['group']
                group = dict(row, slots={}, generations=[0] * 4, cursor=0, idle=0)
                groups.append(group)
            else:
                require(group is not None, 'Record outside owned group')
                if kind == 'ADMIT':
                    row = records.take(kind)
                    slot, gen, fixture = row['slot'], row['generation'], row['fixture']
                    require(row['group'] == group['group'] and slot in range(4) and slot not in group['slots'] and
                        slot == next(i for i in range(4) if i not in group['slots']) and fixture in fixtures and
                        gen == group['generations'][slot] + 1, 'Invalid slot admission/reuse')
                    required = fixtures[fixture]['pages'] + 29
                    root = row['root']
                    require(root % 4096 == 0 and 0x40000000 <= root < 0x41000000 and root != symbols['arm_root'] and
                        all(t['terminal'] is not None or t['root'] != root for t in group['slots'].values()), 'Aliased/invalid live address-space root')
                    require(row['entry'] == fixtures[fixture]['entry'] and row['process_frames'] == required and
                        row['box_frames'] == info['box_frames'] and row['before'] == free and
                        row['after'] == free - required - info['box_frames'], 'Wrong admission frame ownership')
                    require(sum(t['process_frames'] + t['box_frames'] for t in group['slots'].values() if t['terminal'] is None) + required + info['box_frames'] <= group['limit'], 'Group frame cap exceeded')
                    if row['console_generation']:
                        require(row['console_generation'] == console_generation + 1, 'Console generation reset/unexplained burn')
                        console_generation += 1
                    else:
                        require(name == 'cancel_new' and fixture == 'loop', 'Unexpected no-console admission')
                        console_generation += 1  # Issuer.issue(false) still consumes its private sequence.
                    foreign = None
                    if fixture == 'grant':
                        earlier = [t for t in tasks if t['console_generation']]
                        require(earlier, 'Grant probe lacks actual retired/peer grant')
                        foreign = earlier[0]['console_generation'] << 32 | 1
                    task = dict(row, calls=0, written=0, irqs=0, remaining=info['total_ticks'], charged=0, turns=0,
                        ordinal=0, cancel=False, terminal=None, pc=row['entry'], last_end=0, history=[], poll_irqs=0,
                        setup_irqs=0, released=False, foreign_grant=foreign, completion_sequence=None,
                        provenance=fixture.startswith('context_irq') and not (name == 'irq_context' and slot == 2))
                    group['generations'][slot] = gen; group['slots'][slot] = task
                    tasks.append(task); all_tasks.append(task); free = row['after']
                elif kind == 'STEP':
                    free, step = step_record(records, group, info, fixtures, symbols, free)
                    sequence += 1; case_turns += 1; pass_turns[index // 12] += 1
                    require(case_turns <= 1024 and pass_turns[index // 12] <= 4096, 'Dynamic turn bound reached')
                    task = group['slots'][step['step']['slot']]
                    if step['clock'] is not None:
                        require(step['clock']['begin'] >= counter_end, 'Hardware counter moved backward across owners')
                        counter_end = task['last_end']
                    if task['terminal']: task['completion_sequence'] = sequence
                    if step['enter']: slices.append(step)
                    task['history'][-1]['sequence'] = sequence
                elif kind == 'ACTION':
                    row = records.take(kind)
                    require(row['group'] == group['group'] and row['before'] == row['after'], 'Action changed ownership')
                    op, result = row['op'], row['result']
                    require(op in ('cancel', 'query', 'reap', 'release', 'admit_reject'), 'Unknown action')
                    task = group['slots'].get(row['slot'])
                    valid = row['id_group'] == group['group'] and task is not None and row['generation'] == task['generation']
                    if op == 'admit_reject':
                        require(row['id_group'] == row['slot'] == row['generation'] == 0, 'Tokenless rejection has identity')
                        if result == 'full': require(len(group['slots']) == 4 and row['before'] == free, 'False occupied-slot Full')
                        else:
                            require(name == 'admission_rollback' and result == 'resources', 'Unexpected admission failure')
                            failures = sum(a['op'] == 'admit_reject' for a in actions)
                            n = fixtures['hello']['pages'] + 28
                            require(row['before'] == [baseline, 0, n, n + 1][failures] if failures < 4 else False, 'Wrong exact pressure/cap rollback stage')
                            require(group['limit'] == (n + 1 + info['box_frames'] - 1 if failures == 0 else 512), 'Wrong rollback group limit')
                            if failures >= 2: console_generation += 1; burned += 1
                            require(failures < 4, 'Extra rollback attempt')
                    else:
                        require(row['before'] == free, 'Action lied about live free frames')
                        if not valid: require(result in ('stale', 'release_denied') and (op == 'release' or result == 'stale'), 'Foreign/stale token accepted')
                        elif op == 'cancel':
                            require(result == 'ok', 'Valid cancellation rejected')
                            if task['terminal'] is None: task['cancel'] = True
                        elif op == 'query': require(result == ('pending' if task['terminal'] is None else 'ok'), 'Completion query differs')
                        elif op == 'reap':
                            require(result == ('pending' if task['terminal'] is None else 'ok'), 'Live/reaped completion confused')
                            if result == 'ok': del group['slots'][row['slot']]
                        elif result == 'ok':
                            require(release_allowed(task) and task['poll_irqs'] >= 2, 'Invalid/early/double/nonpoll context release')
                            require(not (name == 'irq_context' and row['slot'] == 2), 'Ordinary admission gained release authority')
                            polls = [h['sequence'] for h in task['history'] if h['trap'] and h['trap']['kind'] == 'irq' and 0x100000100 <= h['trap']['elr'] < 0x100000108]
                            require(polls and any(h.get('sequence', 0) > polls[0] for other in group['slots'].values() if other is not task for h in other['history']), 'No peer progress before release')
                            task['released'] = True
                        else:
                            require(result == 'release_denied' and not release_allowed(task), 'Unjustified release refusal')
                    if op == 'release' and result in ('release_denied', 'stale') and any(t['terminal'] is None for t in group['slots'].values()):
                        require(records.index >= 2 and records.rows[records.index - 2].startswith('HMRT WORD ') and records.kind() == 'WORD', 'Refused release lacks adjacent fixed-word readback')
                        prior_word = fields(records.rows[records.index - 2], 'WORD')
                        following_word = fields(records.rows[records.index], 'WORD')
                        require(prior_word == following_word, 'Rejected release changed observed storage')
                        if task is not None and task['terminal'] is None:
                            require(prior_word['slot'] == task['slot'] and prior_word['generation'] == task['generation'],
                                'Refusal observed a different live target owner')
                        else:
                            observer = group['slots'].get(prior_word['slot'])
                            require(observer is not None and observer['provenance'] and observer['terminal'] is None,
                                'Completed refusal lacks a live canonical observer')
                    row['_target_terminal'] = task['terminal'] if valid else None
                    actions.append(row)
                elif kind == 'WORD':
                    row = records.take(kind)
                    task = group['slots'].get(row['slot'])
                    require(row['group'] == row['id_group'] == group['group'] and task is not None and
                        task['generation'] == row['generation'] and task['terminal'] is None and row['value'] == int(task['released']), 'Fixed release word unexpectedly changed')
                    words.append(row)
                elif kind == 'IDLE':
                    require(records.take(kind) == dict(group=group['group'], before=free, after=free) and
                        group['slots'] and all(t['terminal'] is not None for t in group['slots'].values()) and group['idle'] == 0,
                        'Missing runnable owner or duplicate/empty IDLE')
                    group['idle'] += 1
                elif kind == 'DROP':
                    row = records.take(kind)
                    live = [t for t in group['slots'].values() if t['terminal'] is None]
                    require(row == dict(group=group['group'], before=free, after=free + sum(t['process_frames'] + t['box_frames'] for t in live)) and row['after'] == baseline, 'Drop failed exact reclaim')
                    require(not live or name == 'drop_live', 'Unexpected live group destruction')
                    if any(t['group'] == group['group'] for t in tasks) and not live:
                        require(group['idle'] == 1, 'Missing held-completion idle check')
                    for task in live: task['terminal'] = 'dropped'
                    free = row['after']; group = None
                else: raise ValueError('Unexpected runtime record: ' + kind)
        require(group is None and free == baseline, 'Case retains group/frames')
        require(records.take('PASS') == dict(index=index, **{'pass': index // 12}, name=name, before=baseline, after=baseline), 'Wrong case PASS')
        if name == 'busy_peer':
            loop_pc = fixtures['loop']['symbols']['loop_start']
            require(sum(h['trap'] is not None and h['trap']['kind'] == 'irq' and h['trap']['elr'] == loop_pc for h in tasks[0]['history']) >= 2, 'Missing repeated busy-loop IRQs')
        case_contract(name, tasks, actions, groups, info)
        action_contract(name, tasks, actions, groups, words)
        if name == 'admission_rollback': require(burned == 2, 'Wrong failed-prepare issuance')
        if name in ('irq_context', 'completion_tokens'): require(len(words) >= 4, 'Missing rejected-hook readback evidence')
        cases.append(dict(name=name, tasks=tasks, actions=actions, turns=case_turns))
    records.literal('HMRT END 24')
    require(records.index == len(records.rows), 'Extra runtime report')
    lines = raw.split(b'\n')
    baseline_positions = [i for i, line in enumerate(lines) if line.startswith(b'HMTEST ')]
    require(baseline_positions and min(baseline_positions) > records.positions[-1], 'Baseline did not follow runtime END')
    return dict(info=info, cases=cases, slices=slices, records=len(records.rows))


def validate_trace(raw, report, symbols, fixups):
    require(len(raw) <= MAX_LOG_BYTES, 'Runtime trace too large')
    reader = TraceReader(raw, symbols, fixups)
    reader.timer_group()
    counts = dict(svc=0, irq=0, fault=0)
    for step in report['slices']:
        enter, trap = step['enter'], step['trap']
        reader.writes(SLICE_SETUP); reader.returned(0, enter['pc'])
        event = reader.take()
        kind = trap['kind']; counts[kind] += 1
        require(event['kind'] == 'exception' and event['source'] == 0 and event['target'] == 1 and
            event['elr'] == trap['elr'] and event['vector'] == symbols['arm_process_vectors'] + (0x480 if kind == 'irq' else 0x400) and
            event['number'] == {'svc': 2, 'irq': 5, 'fault': 4}[kind], 'Actual lower-EL transition differs from record')
        if kind != 'irq':
            require(event['esr'] == trap['esr'] and event.get('far') == (trap['far'] if kind == 'fault' else None), 'Actual synchronous syndrome differs')
        else:
            ack = reader.take()
            require(ack['kind'] == 'ack' and ack['id'] == 30, 'Unexpected group IRQ source')
            reader.writes(((16, 30),))
        reader.writes(SLICE_RESTORE)
    reader.baseline()
    event = reader.take()
    require(event['kind'] == 'exception' and event['number'] == 16 and event['source'] == event['target'] == 1, 'Missing privileged final exit')
    reader.done()
    return dict(slices=len(report['slices']), **counts, preflight_irqs=8, baseline_irqs=16)


def validate(console, trace, execution, fixtures, symbols, fixups, box_bytes):
    errors, report, traced = [], None, None
    try:
        require(execution.get('returncode') == 0 and execution.get('timed_out') is False and execution.get('launch_error') is None,
            'Runtime guest failed/exceeded deadline')
        report = validate_serial(console, fixtures, symbols, box_bytes)
        traced = validate_trace(trace, report, symbols, fixups)
        require(TRACE_GRAMMAR_REVIEWED, 'Actual runtime trace grammar awaits independent pinned-trace review')
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration, IndexError) as error:
        errors.append(str(error) or 'Incomplete runtime evidence')
    baseline = validate_run(console, execution.get('returncode'), arch='aarch64', timed_out=execution.get('timed_out', True), launch_error=execution.get('launch_error'))
    return dict(passed=not errors and baseline['passed'], errors=errors, baseline=baseline, trace=traced,
        records=report['records'] if report else 0, scope='actual resumed EL0 groups, persistent limits, cancellation and exact reclamation')


def run_profile(output, kernel, fixtures, symbols, fixups, memory, box_bytes):
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
    execution = dict(returncode=None, timed_out=False, launch_error=None)
    started = time.monotonic()
    paths = [output / name for name in ('serial.log', 'stderr.log', 'exceptions.log')]
    with paths[0].open('wb') as stdout, paths[1].open('wb') as stderr:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr) as process:
                while process.poll() is None:
                    if any(p.exists() and p.stat().st_size > MAX_LOG_BYTES for p in paths):
                        execution['launch_error'] = 'Runtime artifact size exceeded'
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
    logs = {p.name: p.read_bytes() if p.exists() and p.stat().st_size <= MAX_LOG_BYTES else b'' for p in paths}
    result = validate(logs['serial.log'], logs['exceptions.log'], execution, fixtures, symbols, fixups, box_bytes)
    result['execution'] = execution
    result['artifacts'] = {p.name: dict(exists=p.exists(), bytes=p.stat().st_size if p.exists() else 0,
        sha256=hashlib.sha256(logs[p.name]).hexdigest(), within_size_limit=p.exists() and p.stat().st_size <= MAX_LOG_BYTES) for p in paths}
    result['passed'] &= all(v['within_size_limit'] for v in result['artifacts'].values())
    write_json(output / 'result.json', result)
    return result


def verify_layout(path, root=None):
    root = Path(__file__).resolve().parents[1] if root is None else root.resolve()
    def local(value):
        text = str(value)
        candidate = root / text.removeprefix('/workspace/') if text.startswith('/workspace/') else Path(text)
        if not candidate.is_absolute(): candidate = root / candidate
        candidate = candidate.resolve()
        require(candidate.is_relative_to(root), 'Layout evidence points outside workspace')
        return candidate
    layout = json.loads(path.read_text())
    require(layout.get('passed') is True and isinstance(layout.get('fields'), dict), 'Missing passing actual compiled layout witness')
    names = ('process_bytes', 'space_bytes', 'box_bytes', 'optional_box_bytes', 'box_backing_pages')
    fields = layout['fields']
    require(set(fields) == set(names) and all(type(fields[k]) is int and 0 < fields[k] < 2**64 for k in names), 'Malformed actual layout fields')
    blob = (path.parent / 'layout.bin').read_bytes()
    require(blob == struct.pack('<5Q', *(fields[k] for k in names)) and
        hashlib.sha256(blob).hexdigest() == layout.get('bytes_sha256'), 'Compiler layout bytes mismatch')
    command = json.loads((path.parent / 'assemble.command.json').read_text())
    assembly = local(command['argv'][1])
    require(hashlib.sha256(assembly.read_bytes()).hexdigest() == layout.get('assembly_sha256'), 'Layout assembly witness mismatch')
    inputs = json.loads((assembly.parent / 'input.json').read_text())
    sources = inputs['sources']
    require(isinstance(sources, dict) and len(sources) >= 4, 'Missing actual layout compiler inputs')
    resolved = {}
    for name, digest in sources.items():
        source = local(name)
        require(hashlib.sha256(source.read_bytes()).hexdigest() == digest, 'Stale actual layout source: ' + str(source))
        resolved[str(source.relative_to(root))] = digest
    require({'crates/hm-core/src/process.rs', 'crates/hm-core/src/user_space.rs'} <= set(resolved), 'Layout witness lacks actual owner types')
    size = fields['process_bytes']
    require(0 < size <= 16384 and fields['box_backing_pages'] == (size + 4095) // 4096, 'Malformed actual box backing charge')
    return size, dict(result=layout, sources=resolved, input_sha256=hashlib.sha256((assembly.parent / 'input.json').read_bytes()).hexdigest())


def main():
    import build_runtime
    from test_boot_profiles import write_json
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('kernel', 'fixtures', 'archive', 'layout', 'output'): parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists(): parser.error('preserve evidence; use a fresh output directory')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        layout_path = args.layout.resolve()
        box_bytes, layout = verify_layout(layout_path)
        kernel = args.kernel.resolve()
        fixtures, archive = build_runtime.verify(args.fixtures.resolve(), args.archive.resolve(), output, kernel)
        symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
        fixups = fixup_table(kernel.read_bytes(), symbols)
        root = Path(__file__).resolve().parents[1]
        sources = ('tools/test_runtime.py', 'tools/test_launcher.py', 'tools/test_process.py', 'tools/build_runtime.py',
            'crates/hm-core/src/process.rs', 'crates/hm-core/src/process_group.rs', 'crates/hm-core/src/runtime_checks.rs',
            'crates/hm-core/fixtures/runtime.rs', 'crates/hm-core/fixtures/runtime.S', 'crates/hm-core/fixtures/runtime.ld')
        write_json(output / 'metadata.json', dict(layout=layout, layout_sha256=hashlib.sha256(layout_path.read_bytes()).hexdigest(),
            kernel_sha256=hashlib.sha256(kernel.read_bytes()).hexdigest(), fixtures=fixtures, archive=archive, symbols=symbols,
            sources={name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in sources}))
        for memory in (64, 128):
            name = f'runtime-{memory}m'
            profiles[name] = run_profile(output / name, kernel, fixtures, symbols, fixups, memory, box_bytes)['passed']
    except (OSError, ValueError, KeyError, TypeError, UnicodeError, struct.error) as caught: error = str(caught)
    result = dict(passed=error is None and len(profiles) == 2 and all(profiles.values()), profiles=profiles, error=error)
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
