#!/usr/bin/env python3
"""Failure-sensitive tests of ownership, accounting and actual transition gates."""
import copy
import struct
import unittest

import test_runtime as gate


def wire(record_kind, **row):
    return 'HMRT ' + record_kind + ''.join(f' {key}={row[key]}' for key in gate.FIELDS[record_kind].split())


class RuntimeParserTests(unittest.TestCase):
    def setUp(self):
        self.info = dict(total_ticks=100, quantum_ticks=10)
        self.symbols = dict(arm_root=0x40000000, arm_vectors=0x40010000, arm_process_vectors=0x40020000)
        self.fixtures = dict(hello=dict(symbols=dict(write_svc=0x100000100, exit_svc=0x100000200, message=0x100002000),
            segments=[dict(flags=5, va=0x100000000, filesz=4096)]))
        self.task = dict(group=1, slot=0, generation=1, fixture='hello', console_generation=1,
            process_frames=33, box_frames=3, root=0x40030000, calls=0, written=0, irqs=0, remaining=100,
            charged=0, turns=0, ordinal=0, cancel=False, terminal=None, pc=0x100000000, last_end=0,
            history=[], poll_irqs=0, setup_irqs=0, released=False)
        self.rows = [
            ('STEP', dict(group=1, turn=1, slot=0, generation=1, state='live', calls=0, written=0, irqs=0, remaining=100, charged=0, turns=0, before=1000)),
            ('CLOCK', dict(group=1, turn=1, begin=1000, remaining=100, comparator=1010, whole_end=1100)),
            ('ENTER', dict(group=1, turn=1, pc=0x100000000, root=0x40030000)),
            ('TRAP', dict(group=1, turn=1, kind='svc', esr=0x56000000, far=0, elr=0x100000104, spsr=0x00400340)),
            ('CALL', dict(group=1, turn=1, ordinal=1, pc=0x100000104, version=1, operation=1, arg1=0x100000001, arg2=0x100002000, arg3=15)),
            ('OUTPUT', dict(group=1, turn=1, ordinal=1, length=15, hex=b'hello from EL0\n'.hex())),
            ('RESULT', dict(group=1, turn=1, ordinal=1, status=0, bytes=15)),
            ('CHARGE', dict(group=1, turn=1, end=1005, accounted=1, remaining=95, charged=5, turns=1)),
            ('RETURN', dict(group=1, turn=1, event='svc', terminal='none', entered=1, code=0, calls=1, written=15, irqs=0,
                elr=0x100000104, remaining=95, charged=5, turns=1, root=0x40000000, vbar=0x40010000, controls=1, before=1000, after=1000))]

    def parse(self, rows=None, task=None, cursor=0):
        rows = self.rows if rows is None else rows
        records = gate.Records(('\n'.join(wire(kind, **row) for kind, row in rows) + '\nHMRT END 24\n').encode())
        records.rows.pop()
        task = copy.deepcopy(self.task if task is None else task)
        group = dict(group=1, cursor=cursor, slots={0: task})
        result = gate.step_record(records, group, self.info, self.fixtures, self.symbols, 1000)
        gate.require(records.index == len(records.rows), 'Unconsumed step record')
        return task, result

    def test_actual_success_and_saved_resume(self):
        task, (free, _) = self.parse()
        self.assertEqual((free, task['pc'], task['remaining'], task['written']), (1000, 0x100000104, 95, 15))
        self.assertEqual(task['ordinal'], 1)

    def test_missing_duplicate_reordered_records(self):
        for index in range(len(self.rows)):
            for rows in (self.rows[:index] + self.rows[index + 1:], self.rows[:index] + [self.rows[index]] + self.rows[index:]):
                with self.subTest(index=index, length=len(rows)), self.assertRaises((ValueError, StopIteration)):
                    self.parse(rows)
        rows = copy.deepcopy(self.rows); rows[2], rows[3] = rows[3], rows[2]
        with self.assertRaises(ValueError): self.parse(rows)

    def test_each_numeric_identity_counter_and_control_field(self):
        # Every non-timestamp scalar in a successful transaction has an independent expectation.
        ignored = {'begin', 'end', 'far', 'spsr'}
        for index, (_, row) in enumerate(self.rows):
            for field, value in row.items():
                if type(value) is not int or field in ignored: continue
                changed = copy.deepcopy(self.rows); changed[index][1][field] += 1
                with self.subTest(record=index, field=field), self.assertRaises(ValueError): self.parse(changed)

    def test_output_not_success_flag(self):
        rows = copy.deepcopy(self.rows); rows[5][1]['hex'] = bytes(15).hex()
        with self.assertRaises(ValueError): self.parse(rows)

    def test_backward_finish_and_counter_overflow(self):
        for index, key, value in ((7, 'end', 999), (1, 'begin', 2**63), (7, 'end', 2**63)):
            rows = copy.deepcopy(self.rows); rows[index][1][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError): self.parse(rows)

    def test_wrong_saved_pc_and_live_root(self):
        for key in ('pc', 'root'):
            task = copy.deepcopy(self.task); task[key] += 4096
            with self.subTest(key=key), self.assertRaises(ValueError): self.parse(task=task)

    def test_cancelled_task_must_not_enter(self):
        task = copy.deepcopy(self.task); task['cancel'] = True
        rows = copy.deepcopy(self.rows); rows[0][1]['state'] = 'cancel_pending'
        with self.assertRaises(ValueError): self.parse(rows, task)
        rows = [rows[0], ('RETURN', dict(group=1, turn=1, event='none', terminal='cancelled', entered=0, code=0,
            calls=0, written=0, irqs=0, elr=task['pc'], remaining=100, charged=0, turns=0,
            root=self.symbols['arm_root'], vbar=self.symbols['arm_vectors'], controls=1, before=1000, after=1036))]
        parsed, (free, _) = self.parse(rows, task)
        self.assertEqual((parsed['terminal'], free), ('cancelled', 1036))
        rows[-1][1]['after'] = 1000
        with self.assertRaises(ValueError): self.parse(rows, task)

    def test_unsafe_pstate_and_unknown_terminal(self):
        for value in (0, 0x00400740, 0x00400341, 0x004003c0):
            rows = copy.deepcopy(self.rows); rows[3][1]['spsr'] = value
            with self.subTest(value=value), self.assertRaises(ValueError): self.parse(rows)
        rows = copy.deepcopy(self.rows); rows[-1][1]['terminal'] = 'unavailable'
        with self.assertRaises(ValueError): self.parse(rows)

    def test_canonical_fields_and_foreign_noise(self):
        line = wire('CLOCK', **self.rows[1][1])
        for wrong in (line + ' extra=1', line.replace('begin=1000', 'begin=01000'), line.replace('begin=1000', 'begin=-1'),
                      line.replace('begin=1000', 'begin=' + str(2**64))):
            with self.subTest(line=wrong), self.assertRaises(ValueError): gate.fields(wrong, 'CLOCK')
        for raw in (b'HMRT BEGIN 24\nnoise\n', b'HMPROC BEGIN 66\n'):
            with self.assertRaises(ValueError): gate.Records(raw)

    def test_full_context_wire_independent_layout(self):
        for name in ('context_svc_a', 'context_svc_b', 'context_irq_a', 'context_irq_b'):
            data = gate.capture_bytes(name)
            words = struct.unpack('<104Q', data)
            self.assertEqual(len(data), 832)
            self.assertEqual(words[31], 0x1001ffc00)
            self.assertEqual(words[-4:], (0, 0, 0, 0))
            self.assertEqual(words[17 if name.endswith('_a') else 18], 1 if 'irq' in name else
                0x1111000000000000 | (1 if name.endswith('_a') else 2) << 8 | (17 if name.endswith('_a') else 18))
        self.assertNotEqual(gate.capture_bytes('context_irq_a'), gate.capture_bytes('context_irq_b'))

    def test_nonzero_exit_timeout_never_pass(self):
        for execution in (dict(returncode=1, timed_out=False, launch_error=None), dict(returncode=0, timed_out=True, launch_error=None)):
            result = gate.validate(b'', b'', execution, {}, {}, [], 9832)
            self.assertFalse(result['passed'])
            self.assertIn('Runtime guest failed/exceeded deadline', result['errors'])



class RuntimeControlAndTraceTests(unittest.TestCase):
    def test_context_setter_windows_and_resume_sanitization(self):
        fixture = dict(symbols=dict(context_set_dit=0x100010100, context_set_ssbs=0x100010108,
            context_svc=0x100010200, poll_start=0x100000100, poll_end=0x100000108))
        task = dict(fixture='context_irq_a')
        gate.check_pstate(task, fixture, dict(elr=0x100010100, spsr=0x00400340))
        gate.check_pstate(task, fixture, dict(elr=0x100010104, spsr=0x01400340))
        gate.check_pstate(task, fixture, dict(elr=0x100000100, spsr=0x00401340))
        gate.check_pstate(task, fixture, dict(elr=0x100000104, spsr=0x00400340))
        for bit in (12, 24):
            with self.subTest(bit=bit), self.assertRaises(ValueError):
                gate.check_pstate(task, fixture, dict(elr=0x100000104, spsr=0x00400340 | 1 << bit))
        task = dict(fixture='context_svc_a')
        gate.check_pstate(task, fixture, dict(elr=0x100010204, spsr=0x01401340))
        with self.assertRaises(ValueError): gate.check_pstate(dict(fixture='hello'), {}, dict(elr=0, spsr=0x01401340))

    def test_exact_action_inventory_cannot_accept_filler(self):
        tasks = [dict(group=1, slot=i, generation=1) for i in range(2)]
        actions = [dict(group=1, id_group=1, slot=i, generation=1, op='reap', result='ok') for i in range(2)]
        gate.action_contract('svc_context', tasks, actions, [], [])
        for changed in (actions[1:], actions + actions[:1], list(reversed(actions)),
                        [dict(actions[0], op='query')] + actions):
            with self.subTest(actions=changed), self.assertRaises(ValueError):
                gate.action_contract('svc_context', tasks, changed, [], [])
        with self.assertRaises(ValueError): gate.action_contract('svc_context', tasks, actions, [], [dict(value=0)])

    def test_actual_trace_transaction_mutations(self):
        from test_test_launcher import SYMBOLS, FIXUPS, trace as baseline_trace
        from test_test_startup_delivery import prefix
        symbols = dict(SYMBOLS, arm_root=0x40240000)
        lines = prefix(baseline_trace(True), 42)
        def writes(sequence):
            for offset, value in sequence: lines.append(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}')
        slices = []
        for kind, pc, number, esr in (('svc', 0x100000104, 2, 0x56000000), ('irq', 0x100000108, 5, 0x56000000)):
            entry = 0x100000000 if kind == 'svc' else 0x100000104
            writes(gate.SLICE_SETUP)
            lines.append(f'Exception return from AArch64 EL1 to AArch64 EL0 PC {entry:#x}')
            lines += [f'Taking exception {number} [{"SVC" if kind == "svc" else "IRQ"}] on CPU 0',
                '...from EL0 to EL1', f'...with ESR {esr >> 26:#x}/{esr:#x}', f'...with ELR {pc:#x}',
                f'...to EL1 PC {symbols["arm_process_vectors"] + (0x400 if kind == "svc" else 0x480):#x} PSTATE 0x3c5']
            if kind == 'irq':
                lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30'); writes(((16, 30),))
            writes(gate.SLICE_RESTORE)
            slices.append(dict(enter=dict(pc=entry), trap=dict(kind=kind, elr=pc, esr=esr, far=0)))
        lines += prefix(baseline_trace(), 142)
        lines += ['Taking exception 16 [Semihosting call] on CPU 0', '...from EL1 to EL1', '...handling as semihosting call 0x20']
        raw = ('\n'.join(lines) + '\n').encode()
        report = dict(slices=slices)
        self.assertEqual(gate.validate_trace(raw, report, symbols, FIXUPS)['slices'], 2)
        for old, new in ((b'...from EL0 to EL1', b'...from EL1 to EL1'),
            (b'PC 0x100000104', b'PC 0x100000108'), (b'...with ELR 0x100000104', b'...with ELR 0x100000100'),
            (b'acknowledged irq 30', b'acknowledged irq 31'), (b'...from EL1 to EL1\n...handling', b'...from EL0 to EL1\n...handling'),
            (b'...with ESR 0x15/0x56000000', b'...with ESR 0x15/0x56000001')):
            self.assertIn(old, raw)
            with self.subTest(old=old), self.assertRaises(ValueError): gate.validate_trace(raw.replace(old, new, 1), report, symbols, FIXUPS)
        for row in (b'gic_cpu_write cpu 0 iface write at 0x00000004 0x000000ff\n',
            b'gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n'):
            for changed in (raw.replace(row, b'', 1), raw.replace(row, row + row, 1)):
                with self.assertRaises(ValueError): gate.validate_trace(changed, report, symbols, FIXUPS)



class SyntheticRun:
    """A deterministic peer-interleaving transcript, independent of parser state."""
    def __init__(self):
        import json
        from pathlib import Path
        self.fixtures = json.loads((Path(__file__).resolve().parents[1] / 'crates/hm-core/fixtures/runtime/manifest.json').read_text())['fixtures']
        self.symbols = dict(arm_root=0x40000000, arm_vectors=0x40010000)
        self.lines = ['HMRT PREFLIGHT checks=15 irqs=8', 'HMRT BEGIN 24', wire('INFO', wire=1, cpus=1,
            frequency=10000, total_ticks=1000, quantum_ticks=10, box_bytes=9832, box_frames=3)]
        self.namespace = self.issuer = 0
        self.free = 1000
        self.time = 10000
    def row(self, record_kind, **values): self.lines.append(wire(record_kind, **values))
    def group(self, limit=512):
        self.namespace += 1; self.slots = {}; self.generations = [0] * 4; self.cursor = 0
        self.row('GROUP', group=self.namespace, limit=limit, before=self.free)
    def admit(self, name, console=True, foreign=None):
        slot = next(i for i in range(4) if i not in self.slots)
        self.generations[slot] += 1; self.issuer += 1
        frames = self.fixtures[name]['pages'] + 29
        task = dict(group=self.namespace, slot=slot, generation=self.generations[slot], console_generation=self.issuer if console else 0,
            fixture=name, entry=self.fixtures[name]['entry'], root=0x40400000 + slot * 4096,
            process_frames=frames, box_frames=3)
        self.row('ADMIT', **task, before=self.free, after=self.free - frames - 3)
        self.free -= frames + 3
        task.update(calls=0, written=0, irqs=0, turns=0, ordinal=0, remaining=1000, charged=0, cancel=False,
            terminal=None, pc=task['entry'], released=False, foreign_grant=foreign, polls=0)
        self.slots[slot] = task
        return task
    def action(self, task, op, result='ok'):
        self.row('ACTION', group=self.namespace, id_group=task['group'], slot=task['slot'], generation=task['generation'],
            op=op, result=result, before=self.free, after=self.free)
        if op == 'cancel' and result == 'ok' and not task['terminal']: task['cancel'] = True
        if op == 'reap' and result == 'ok': del self.slots[task['slot']]
    def reject(self, result='resources', free=None):
        before = self.free if free is None else free
        self.row('ACTION', group=self.namespace, id_group=0, slot=0, generation=0, op='admit_reject', result=result, before=before, after=before)
    def word(self, task):
        self.row('WORD', group=self.namespace, id_group=task['group'], slot=task['slot'], generation=task['generation'], value=int(task['released']))
    def refuse(self, task, observer, stale=False):
        self.word(observer); self.action(task, 'release', 'stale' if stale else 'release_denied'); self.word(observer)
    def release(self, task):
        self.word(task); self.action(task, 'release'); task['released'] = True; self.word(task); self.refuse(task, task)
    def step(self):
        slot = next((self.cursor + i) % 4 for i in range(4) if (self.cursor + i) % 4 in self.slots and not self.slots[(self.cursor + i) % 4]['terminal'])
        self.cursor = (slot + 1) % 4
        task = self.slots[slot]; task['ordinal'] += 1
        scope = dict(group=self.namespace, turn=task['ordinal'])
        self.row('STEP', **scope, slot=slot, generation=task['generation'], state='cancel_pending' if task['cancel'] else 'live',
            before=self.free, **{k: task[k] for k in ('calls', 'written', 'irqs', 'remaining', 'charged', 'turns')})
        event = 'none'; end = 'cancelled' if task['cancel'] else 'none'; entered = not task['cancel']
        if entered:
            start = self.time; self.time += 100
            self.row('CLOCK', **scope, begin=start, remaining=task['remaining'], comparator=start + min(10, task['remaining']), whole_end=start + task['remaining'])
            self.row('ENTER', **scope, pc=task['pc'], root=task['root'])
            name = task['fixture']; symbols = self.fixtures[name]['symbols']; pstate = 0x00400340
            if name == 'loop' or name.startswith('context_irq') and not task['released']:
                event, kind, esr, far = 'timer', 'irq', 0, 0
                task['pc'] = symbols['loop_start'] if name == 'loop' else symbols['poll_start']
                if name.startswith('context_irq'):
                    if task['polls'] == 0: pstate |= 0x01001000
                    task['polls'] += 1
                task['irqs'] += 1; duration = 10
            elif name == 'fault':
                event, kind, esr, far = 'fault', 'fault', 0x9200004f, task['entry']
                task['pc'] = symbols['fault']; duration = 5; end = 'fault'
            else:
                event, kind, esr, far = 'svc', 'svc', 0x56000000, 0
                call = gate.call_contract(task, task['calls'] + 1, self.fixtures)
                task['pc'] = call['pc']; duration = 5
                if name.startswith('context_svc') and task['calls'] == 0: pstate |= 0x01001000
            self.row('TRAP', **scope, kind=kind, esr=esr, far=far, elr=task['pc'], spsr=pstate)
            if event == 'svc':
                task['calls'] += 1
                self.row('CALL', **scope, ordinal=task['calls'], **{k: call[k] for k in ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')})
                if call['output']:
                    self.row('OUTPUT', **scope, ordinal=task['calls'], length=len(call['output']), hex=call['output'].hex())
                    task['written'] += len(call['output'])
                if call['status'] is not None: self.row('RESULT', **scope, ordinal=task['calls'], status=call['status'], bytes=call['bytes'])
                if call['operation'] == 2: end = 'exit'
                elif name == 'call_limit' and task['calls'] == 64: end = 'call_limit'
            duration = min(duration, task['remaining']); task['remaining'] -= duration; task['charged'] += duration; task['turns'] += 1
            if task['remaining'] == 0 and end == 'none': end = 'budget'
            self.row('CHARGE', **scope, end=start + duration, accounted=1, **{k: task[k] for k in ('remaining', 'charged', 'turns')})
        after = self.free + (task['process_frames'] + 3 if end != 'none' else 0)
        self.row('RETURN', **scope, event=event, terminal=end, entered=int(entered), code=0, elr=task['pc'],
            root=self.symbols['arm_root'], vbar=self.symbols['arm_vectors'], controls=1, before=self.free, after=after,
            **{k: task[k] for k in ('calls', 'written', 'irqs', 'remaining', 'charged', 'turns')})
        self.free = after; task['terminal'] = None if end == 'none' else end
        return task
    def drain(self):
        while any(not t['terminal'] for t in self.slots.values()): self.step()
    def idle(self): self.row('IDLE', group=self.namespace, before=self.free, after=self.free)
    def drop(self):
        self.row('DROP', group=self.namespace, before=self.free, after=1000); self.free = 1000
    def finish(self, tasks):
        self.drain(); self.idle()
        for task in tasks: self.action(task, 'reap')
        self.drop()
    def build(self):
        for index in range(24):
            name = gate.CASES[index % 12]
            self.row('CASE', index=index, **{'pass': index // 12}, name=name, before=1000)
            if name == 'admission_rollback':
                n = self.fixtures['hello']['pages'] + 28
                for phase, free in enumerate((1000, 0, n, n + 1)):
                    self.group(n + 3 if phase == 0 else 512); self.reject(free=free)
                    if phase >= 2: self.issuer += 1
                    if phase == 3:
                        a = self.admit('hello'); self.finish([a])
                    else: self.drop()
            elif name == 'completion_tokens':
                self.group(); tasks = [self.admit('hello') for _ in range(4)]; self.drain(); self.reject('full')
                for task in tasks:
                    self.action(task, 'query'); self.action(task, 'query')
                old = tasks[0]; self.action(old, 'reap')
                replacement = self.admit('grant', foreign=old['console_generation'] << 32 | 1)
                for op in ('cancel', 'query', 'reap'): self.action(old, op, 'stale')
                self.refuse(old, replacement, True); self.action(replacement, 'reap', 'pending')
                self.finish([replacement] + tasks[1:])
                self.group(); a = self.admit('context_irq_a'); b = self.admit('hello')
                for op in ('cancel', 'query', 'reap'): self.action(replacement, op, 'stale')
                self.refuse(replacement, a, True); self.refuse(b, b)
                refused = False
                while not a['terminal'] or not b['terminal']:
                    task = self.step()
                    if task is b and b['terminal']:
                        self.refuse(b, a); refused = True
                    if task is a and a['polls'] >= 2 and refused and not a['released']: self.release(a)
                self.finish([a, b])
            elif name in ('irq_context', 'drop_live'):
                self.group(); a = self.admit('context_irq_a'); b = self.admit('context_irq_b')
                tasks = [a, b]
                if name == 'irq_context':
                    ordinary = self.admit('context_irq_a'); tasks.append(ordinary)
                    self.refuse(ordinary, ordinary); self.action(ordinary, 'cancel')
                    completed = False
                    while any(not t['terminal'] for t in tasks):
                        task = self.step()
                        if task in (a, b):
                            if task['polls'] >= 2 and not task['released']: self.release(task)
                            if task['terminal'] and not completed:
                                self.refuse(task, b if task is a else a); completed = True
                    self.finish(tasks)
                else:
                    while not a['polls'] or not b['polls']: self.step()
                    self.word(a); self.word(b); self.drop()
                    self.group(); hello = self.admit('hello'); self.finish([hello])
            else:
                pairs = {'svc_context': ('context_svc_a', 'context_svc_b'), 'busy_peer': ('loop', 'hello'),
                    'byte_budget': ('byte_budget', 'grant'), 'call_limit': ('call_limit', 'hello'), 'fault_peer': ('fault', 'hello'),
                    'cancel_new': ('loop', 'hello'), 'cancel_reply': ('yield', 'hello'), 'cancel_quantum': ('loop', 'hello')}
                self.group(); first, second = pairs[name]
                a = self.admit(first, name != 'cancel_new'); b = self.admit(second, foreign=a['console_generation'] << 32 | 1)
                if name.startswith('cancel_'):
                    if name != 'cancel_new': self.step()
                    self.action(a, 'cancel'); self.action(a, 'cancel')
                self.finish([a, b])
            self.row('PASS', index=index, **{'pass': index // 12}, name=name, before=1000, after=1000)
        from test_kernel import CASES
        self.lines += ['HMRT END 24', f'HMTEST BEGIN {len(CASES)}'] + [f'HMTEST PASS {case}' for case in CASES] + [f'HMTEST END {len(CASES)}']
        return ('\n'.join(self.lines) + '\n').encode()


class FullRuntimeInventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.script = SyntheticRun(); cls.raw = cls.script.build()
    def validate(self, raw): return gate.validate_serial(raw, self.script.fixtures, self.script.symbols, 9832)
    def test_complete_two_pass_inventory(self):
        report = self.validate(self.raw)
        self.assertEqual(len(report['cases']), 24)
        self.assertEqual([c['name'] for c in report['cases']], list(gate.CASES) * 2)
    def test_every_case_action_and_ownership_record_required_once(self):
        for row in self.raw.splitlines(keepends=True):
            if row.startswith(tuple(b'HMRT ' + name for name in (b'CASE', b'GROUP', b'ADMIT', b'ACTION', b'WORD', b'IDLE', b'DROP', b'PASS'))):
                for replacement in (b'', row + row):
                    with self.subTest(row=row[:100], duplicate=bool(replacement)), self.assertRaises((ValueError, StopIteration)):
                        self.validate(self.raw.replace(row, replacement, 1))
    def test_group_generation_reclamation_and_layout_mutations(self):
        for old, new in ((b'box_bytes=9832', b'box_bytes=8936'), (b'box_frames=3', b'box_frames=2'),
                         (b'console_generation=1', b'console_generation=2'), (b'generation=2', b'generation=1'),
                         (b'after=1000', b'after=999'), (b'before=0 after=0', b'before=1 after=1'),
                         (b'name=svc_context', b'name=irq_context'), (b'pass=1', b'pass=0')):
            self.assertIn(old, self.raw)
            with self.subTest(old=old), self.assertRaises(ValueError): self.validate(self.raw.replace(old, new, 1))


    def test_completed_refusal_cannot_move_before_completion(self):
        rows = self.raw.decode().splitlines()
        start = next(i for i, row in enumerate(rows) if row.startswith('HMRT CASE index=1 '))
        stop = next(i for i, row in enumerate(rows) if row.startswith('HMRT PASS index=1 '))
        denials = [i for i in range(start, stop) if rows[i].startswith('HMRT ACTION ') and
            'op=release slot=0 generation=1 result=release_denied' in rows[i]]
        self.assertEqual(len(denials), 2)
        index = denials[-1]
        step = max(i for i in range(start, index) if rows[i].startswith('HMRT STEP '))
        self.assertIn('terminal=exit', rows[index - 2])
        block = rows[index - 1:index + 2]
        changed = rows[:step] + block + rows[step:index - 1] + rows[index + 2:]
        with self.assertRaises(ValueError): self.validate(('\n'.join(changed) + '\n').encode())

    def test_counter_time_is_monotonic_across_owners(self):
        rows = self.raw.decode().splitlines()
        clock_indices = [i for i, row in enumerate(rows) if row.startswith('HMRT CLOCK ')]
        index = clock_indices[1]
        clock = gate.fields(rows[index], 'CLOCK')
        delta = clock['begin'] - 1
        for key in ('begin', 'comparator', 'whole_end'): clock[key] -= delta
        rows[index] = wire('CLOCK', **clock)
        finish = next(i for i in range(index, len(rows)) if rows[i].startswith('HMRT CHARGE '))
        charge = gate.fields(rows[finish], 'CHARGE'); charge['end'] -= delta
        rows[finish] = wire('CHARGE', **charge)
        with self.assertRaises(ValueError): self.validate(('\n'.join(rows) + '\n').encode())

    def test_refusal_must_observe_the_live_target_storage(self):
        rows = self.raw.decode().splitlines()
        index = next(i for i, row in enumerate(rows) if 'op=release slot=2 generation=1 result=release_denied' in row)
        rows[index - 1] = rows[index - 1].replace('slot=2', 'slot=0')
        rows[index + 1] = rows[index + 1].replace('slot=2', 'slot=0')
        with self.assertRaises(ValueError): self.validate(('\n'.join(rows) + '\n').encode())

    def test_release_requires_current_quiescent_poll_trap(self):
        task = dict(provenance=True, terminal=None, cancel=False, released=False, irqs=2,
            history=[dict(trap=dict(kind='irq', elr=0x100000104))])
        self.assertTrue(gate.release_allowed(task))
        for key, value in (('provenance', False), ('terminal', 'exit'), ('cancel', True), ('released', True), ('irqs', 1),
                ('history', [dict(trap=dict(kind='svc', elr=0x100000104))]),
                ('history', [dict(trap=dict(kind='irq', elr=0x100010000))])):
            changed = dict(task); changed[key] = value
            with self.subTest(key=key): self.assertFalse(gate.release_allowed(changed))


class ActualLayoutWitnessTests(unittest.TestCase):
    def test_actual_bytes_assembly_and_current_sources_are_bound(self):
        import hashlib
        import json
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory(prefix='hematite-layout-unit-') as directory:
            root = Path(directory)
            evidence = root / 'evidence'; evidence.mkdir()
            assembly = evidence / 'sizes.s'; assembly.write_bytes(b'compiled assembly witness')
            sources = {}
            for name in ('crates/hm-core/src/process.rs', 'crates/hm-core/src/user_space.rs',
                         'crates/hm-core/src/process_group.rs', 'crates/hm-core/src/process_task.rs',
                         'crates/hm-core/src/process_task_turn.rs', 'wrapper.rs', 'sizes.rs'):
                source = root / name; source.parent.mkdir(parents=True, exist_ok=True); source.write_bytes(name.encode())
                sources[name] = hashlib.sha256(source.read_bytes()).hexdigest()
            (evidence / 'input.json').write_text(json.dumps(dict(sources=sources)))
            (evidence / 'assemble.command.json').write_text(json.dumps(dict(argv=['aarch64-linux-gnu-as', str(assembly)])))
            values = dict(process_bytes=9832, space_bytes=8936, box_bytes=40, optional_box_bytes=40, box_backing_pages=3)
            task = dict(image_bytes=472, image_pages=1, authority_bytes=928, authority_pages=1, group_bytes=1960, metadata_pages=3)
            blob = struct.pack('<11Q', *values.values(), *task.values()); (evidence / 'layout.bin').write_bytes(blob)
            result = dict(passed=True, fields=values, task_fields=task, assembly_sha256=hashlib.sha256(assembly.read_bytes()).hexdigest(),
                bytes_sha256=hashlib.sha256(blob).hexdigest())
            path = evidence / 'result.json'; path.write_text(json.dumps(result))
            self.assertEqual(gate.verify_layout(path, root)[0], 9832)
            for filename, replacement in (('layout.bin', bytes(88)), ('sizes.s', b'different actual assembly'),
                    ('input.json', json.dumps(dict(sources={})).encode())):
                source = evidence / filename; prior = source.read_bytes(); source.write_bytes(replacement)
                with self.subTest(file=filename), self.assertRaises(ValueError): gate.verify_layout(path, root)
                source.write_bytes(prior)
            for missing in ('process_group.rs', 'process_task.rs', 'process_task_turn.rs'):
                reduced = {name: digest for name, digest in sources.items() if not name.endswith('/' + missing)}
                (evidence / 'input.json').write_text(json.dumps(dict(sources=reduced)))
                with self.subTest(missing=missing), self.assertRaises(ValueError): gate.verify_layout(path, root)
            (evidence / 'input.json').write_text(json.dumps(dict(sources=sources)))
            for bad in (None, {}, {**task, 'metadata_pages': 4}, {**task, 'group_bytes': True}):
                path.write_text(json.dumps({**result, 'task_fields': bad}))
                with self.subTest(task=bad), self.assertRaises(ValueError): gate.verify_layout(path, root)
            path.write_text(json.dumps(result))
            process = root / 'crates/hm-core/src/process.rs'; process.write_bytes(b'changed layout source')
            with self.assertRaises(ValueError): gate.verify_layout(path, root)


if __name__ == '__main__': unittest.main()
