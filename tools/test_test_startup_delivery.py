#!/usr/bin/env python3
"""Failure-sensitive hardware-input serial and EL0 transaction inventory tests."""
import copy
import unittest

import test_startup_delivery as h
from test_kernel import CASES as BASELINE
from test_process import FIELDS as NATIVE_FIELDS, parse_trace
from test_test_launcher import SYMBOLS as BASE_SYMBOLS, FIXUPS, trace as launcher_trace

SYMBOLS = {**BASE_SYMBOLS, 'arm_root': 0x40240000}
FIXTURES = {name: {'entry': 0x100000000, 'pages': 3,
            'input': {'present': data is not None, 'hex': (data or b'').hex()}, 'symbols': {
                'write_svc': 0x100000040, 'newline_svc': 0x100000050, 'exit_svc': 0x100000060,
                'newline': 0x100001000, 'fault_write': 0x100000070, 'branch_payload': 0x100000074,
                'loop_start': 0x100000080, 'loop_end': 0x100000084}} for name, data in h.INPUTS.items()}
EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}


def record(kind, **values):
    native = kind in ('CALL', 'OUTPUT', 'RESULT')
    names = (NATIVE_FIELDS if native else h.FIELDS)[kind].split()
    return ('HMPROC ' if native else 'HMINPUT ') + kind + ''.join(f' {key}={values[key]}' for key in names)


def console():
    lines = ['hematite bootstrap', record('REJECT', phase='before', error='length', initialized=0,
             generation=0, before=1000, after=1000), 'HMINPUT PREFLIGHT checks=15 irqs=8', 'HMINPUT BEGIN 24',
             record('INFO', wire=1, extension=1, max_input=256, cpus=1, timer_frequency=62500000, timer_ticks=6250000)]
    for index in range(24):
        case = h.CASES[index % 12]
        name = h.fixture_name(case)
        fixture, payload = FIXTURES[name], h.INPUTS[name]
        lines.append(record('RUN', index=index, **{'pass': index // 12}, case=case, fixture=name,
            present=int(payload is not None), length=len(payload or b''), generation=index + 1,
            entry=fixture['entry'], root=0x40400000, before=1000, during=968, owned=32))
        lines.append(record('READBACK', index=index, base=1, extension=1, zero_tail=1, source_detached=1))
        calls = h.expected_calls(name, fixture, index)
        written = 0
        for ordinal, call in enumerate(calls, 1):
            lines.append(record('CALL', index=index, ordinal=ordinal, **{key: call[key] for key in
                ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')}))
            if call['output']:
                lines.append(record('OUTPUT', index=index, ordinal=ordinal, length=len(call['output']), hex=call['output'].hex()))
                written += len(call['output'])
            if call['status'] is not None:
                lines.append(record('RESULT', index=index, ordinal=ordinal, status=0, bytes=call['bytes']))
        reason = 'exit' if calls else 'deadline' if name == 'input_loop' else 'fault'
        if calls:
            esr, far, elr, code = 0x56000000, 0, calls[-1]['pc'], calls[-1]['arg1']
        elif name == 'payload_write':
            esr, far, elr, code = 0x9200004f, h.PAYLOAD, fixture['symbols']['fault_write'], 0
        elif name == 'payload_execute':
            esr, far, elr, code = 0x8200000f, h.PAYLOAD, h.PAYLOAD, 0
        else:
            esr, far, elr, code = 0, 0, fixture['symbols']['loop_start'], 0
        lines.append(record('STOP', index=index, reason=reason, code=code, calls=len(calls), written=written,
                esr=esr, far=far, elr=elr, spsr=0x00400740 if name == 'payload_execute' else 0x00400340,
                irqs=int(reason == 'deadline')))
        lines.append(record('RESTORE', index=index, root=SYMBOLS['arm_root'], vbar=SYMBOLS['arm_vectors'],
                controls=1, inactive=1, before=1000, after=1000))
        lines.append(record('PASS', index=index, **{'pass': index // 12}, case=case))
    lines += [record('REJECT', phase='after', error='length', initialized=1, generation=24, before=1000, after=1000),
              record('PREPARE', budget=1, exhausted=1, drop=1, generation=27, before=1000, after=1000),
              'HMINPUT END 24', f'HMTEST BEGIN {len(BASELINE)}']
    lines += [f'HMTEST PASS {name}' for name in BASELINE] + [f'HMTEST END {len(BASELINE)}']
    return ('\n'.join(lines) + '\n').encode()


def prefix(source, count):
    events, lines = parse_trace(source)
    return lines[:events[count]['line']]


def trace():
    lines = prefix(launcher_trace(True), 42)  # One exact timer8 preflight.
    def writes(sequence):
        for offset, value in sequence:
            lines.append(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}')
    def returned(el, pc):
        lines.append(f'Exception return from AArch64 EL1 to AArch64 EL{el} PC {pc:#x}')
    def exception(number, esr, elr, vector, far=None):
        name = {2: 'SVC', 3: 'Prefetch Abort', 4: 'Data Abort', 5: 'IRQ'}[number]
        lines.extend([f'Taking exception {number} [{name}] on CPU 0', '...from EL0 to EL1', f'...with ESR {esr >> 26:#x}/{esr:#x}'])
        if far is not None: lines.append(f'...with FAR {far:#x}')
        lines.extend([f'...with ELR {elr:#x}', f'...to EL1 PC {vector:#x} PSTATE 0x3c5'])
    for index in range(24):
        name = h.fixture_name(h.CASES[index % 12])
        fixture = FIXTURES[name]
        writes(h.SLICE_SETUP); returned(0, fixture['entry'])
        calls = h.expected_calls(name, fixture, index)
        if calls:
            for ordinal, call in enumerate(calls, 1):
                exception(2, 0x56000000, call['pc'], SYMBOLS['arm_process_vectors'] + 0x400)
                writes(h.SLICE_RESTORE)
                if ordinal < len(calls):
                    writes(h.SLICE_SETUP); returned(0, call['pc'])
        elif name == 'payload_write':
            exception(4, 0x9200004f, fixture['symbols']['fault_write'], SYMBOLS['arm_process_vectors'] + 0x400, h.PAYLOAD)
            writes(h.SLICE_RESTORE)
        elif name == 'payload_execute':
            exception(3, 0x8200000f, h.PAYLOAD, SYMBOLS['arm_process_vectors'] + 0x400, h.PAYLOAD)
            writes(h.SLICE_RESTORE)
        else:
            exception(5, 0, fixture['symbols']['loop_start'], SYMBOLS['arm_process_vectors'] + 0x480)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            writes(((16, 30),)); writes(h.SLICE_RESTORE)
    lines += prefix(launcher_trace(), 142)  # Registered full41-case baseline hardware activity.
    lines += ['Taking exception 16 [Semihosting call] on CPU 0', '...from EL1 to EL1', '...handling as semihosting call 0x20']
    return ('\n'.join(lines) + '\n').encode()


class StartupDeliveryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw, cls.trace = console(), trace()
    def reject(self, raw=None, traced=None, execution=EXECUTION, fixtures=FIXTURES):
        result = h.validate(self.raw if raw is None else raw, self.trace if traced is None else traced,
                            execution, fixtures, SYMBOLS, FIXUPS)
        self.assertFalse(result['passed'], result)

    def test_complete_inventory(self):
        result = h.validate(self.raw, self.trace, EXECUTION, FIXTURES, SYMBOLS, FIXUPS)
        self.assertTrue(result['passed'], result)
        self.assertEqual(result['trace']['slices'], 60)
        self.assertEqual(result['trace']['el0_svcs'], 54)
        self.assertEqual(len(h.CASES), 12)
        # Empty/absent output equality does not erase their distinct EL0 exit proof.
        self.assertIn(b'reason=exit code=10 calls=3 written=1', self.raw)
        self.assertIn(b'reason=exit code=11 calls=3 written=1', self.raw)
        self.assertIn(b'reason=exit code=12 calls=3 written=257', self.raw)

    def test_every_record_required_once(self):
        for line in self.raw.splitlines(keepends=True):
            if line.startswith((b'HMINPUT', b'HMPROC', b'HMTEST')):
                with self.subTest(line=line[:100]):
                    self.reject(raw=self.raw.replace(line, b'', 1))
                    self.reject(raw=self.raw.replace(line, line + line, 1))

    def test_presence_length_generation_order_and_resource_mutations(self):
        for old, new in ((b'present=0', b'present=1'), (b'length=256', b'length=255'),
                         (b'generation=24', b'generation=25'), (b'generation=27', b'generation=26'),
                         (b'initialized=0', b'initialized=1'), (b'after=1000', b'after=999'),
                         (b'during=968', b'during=969'), (b'owned=32', b'owned=31'),
                         (b'case=after_binary_absent', b'case=absent'), (b'pass=1', b'pass=0'),
                         (b'root=1077936128', b'root=1077936129')):
            self.reject(raw=self.raw.replace(old, new, 1))
        self.reject(raw=self.raw.replace(b'HMINPUT BEGIN 24', b'HMINPUT BEGIN 23', 1))
        self.reject(raw=self.raw.replace(b'HMINPUT END 24', b'HMINPUT END 25', 1))

    def test_readback_and_failure_drop_proofs(self):
        for old in (b'base=1', b'extension=1', b'zero_tail=1', b'source_detached=1', b'controls=1', b'inactive=1',
                    b'budget=1', b'exhausted=1', b'drop=1'):
            self.reject(raw=self.raw.replace(old, old[:-1] + b'0', 1))
        self.reject(raw=self.raw.replace(b'error=length', b'error=size', 1))
        self.reject(raw=self.raw.replace(b'generation=27', b'generation=28', 1))

    def test_exact_bytes_calls_and_exit_presence_proof(self):
        for old, new in ((b'code=10', b'code=11'), (b'code=11', b'code=10'), (b'code=12', b'code=0'),
                         (b'hex=41', b'hex=40'), (b'hex=0a', b'hex=00'), (b'status=0', b'status=1'),
                         (b'arg2=' + str(h.PAYLOAD).encode(), b'arg2=' + str(h.STARTUP + 64).encode()),
                         (b'arg1=4294967297', b'arg1=8589934593'), (b'written=257', b'written=256')):
            self.reject(raw=self.raw.replace(old, new, 1))
        expected = bytes(range(256)).hex().encode()
        self.reject(raw=self.raw.replace(expected, expected[:-2] + b'fe', 1))
        extra = b'HMPROC OUTPUT index=0 ordinal=1 length=0 hex=00\n'
        self.reject(raw=self.raw.replace(b'HMPROC RESULT index=0 ordinal=1', extra + b'HMPROC RESULT index=0 ordinal=1', 1))

    def test_fault_addresses_syndromes_and_exact_branch_type(self):
        for old, new in ((f'esr={0x9200004f}'.encode(), f'esr={0x92000007}'.encode()), # 9200004f ->92000007
                         (f'esr={0x8200000f}'.encode(), f'esr={0x82000007}'.encode()),
                         (b'far=' + str(h.PAYLOAD).encode(), b'far=' + str(h.PAYLOAD + 1).encode()),
                         (b'spsr=4196160', b'spsr=4197184'), # BTYPE1 ->2
                         (b'spsr=4195136', b'spsr=4196160'),
                         (b'irqs=1', b'irqs=0')):
            self.assertIn(old, self.raw)
            self.reject(raw=self.raw.replace(old, new, 1))

    def test_fault_and_loop_cleared_nzcv_is_exact(self):
        rows = self.raw.splitlines(keepends=True)
        count = 0
        for row in rows:
            if row.startswith(b'HMINPUT STOP') and b'reason=exit' not in row:
                count += 1
                fields = h.fields(row.decode().strip(), 'STOP')
                old = f"spsr={fields['spsr']}".encode()
                for bit in (28, 29, 30, 31):
                    changed = f"spsr={fields['spsr'] | (1 << bit)}".encode()
                    with self.subTest(index=fields['index'], bit=bit):
                        self.reject(raw=self.raw.replace(row, row.replace(old, changed), 1))
        self.assertEqual(count, 6)
        # Rust exit probes may leave arithmetic NZCV; only control bits are fixed.
        exit_row = next(row for row in rows if row.startswith(b'HMINPUT STOP') and b'reason=exit' in row)
        fields = h.fields(exit_row.decode().strip(), 'STOP')
        changed_row = exit_row.replace(f"spsr={fields['spsr']}".encode(), f"spsr={fields['spsr'] | 0xf0000000}".encode())
        result = h.validate(self.raw.replace(exit_row, changed_row, 1), self.trace, EXECUTION, FIXTURES, SYMBOLS, FIXUPS)
        self.assertTrue(result['passed'], result)

    def test_real_el0_trace_cannot_be_replaced_by_serial_claims(self):
        for old, new in ((b'...from EL0 to EL1', b'...from EL1 to EL1'),
                         (b'...with ELR 0x100000044', b'...with ELR 0x100000048'),
                         (b'...with FAR 0x100100058', b'...with FAR 0x10010005c'),
                         (b'...with ESR 0x20/0x8200000f', b'...with ESR 0x20/0x82000007'),
                         (b'...to EL1 PC 0x40210480', b'...to EL1 PC 0x40200a80'),
                         (b'CPU 0', b'CPU 1')):
            self.reject(traced=self.trace.replace(old, new, 1))
        line = b'Exception return from AArch64 EL1 to AArch64 EL0 PC 0x100000000\n'
        self.reject(traced=self.trace.replace(line, b'', 1))
        self.reject(traced=self.trace.replace(line, line + line, 1))
        self.reject(traced=self.trace.replace(b'Taking exception 5 [IRQ] on CPU 0\n...from EL0', b'Taking exception 2 [SVC] on CPU 0\n...from EL0', 1))

    def test_no_extra_missing_or_reordered_gic_baseline_or_exit(self):
        for line in (b'gic_cpu_write cpu 0 iface write at 0x00000004 0x000000ff\n',
                     b'gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n',
                     b'gic_acknowledge_irq cpu 0 acknowledged irq 30\n'):
            self.reject(traced=self.trace.replace(line, b'', 1))
            self.reject(traced=self.trace.replace(line, line + line, 1))
        self.reject(traced=self.trace.replace(b'...with ESR 0x25/0x96000006', b'...with ESR 0x25/0x96000007', 1))
        self.reject(traced=self.trace.rsplit(b'Taking exception 16', 1)[0])
        self.reject(traced=self.trace + b'unknown trace line\n')
        self.reject(traced=self.trace[:-1])
        self.reject(raw=self.raw.replace(b'HMINPUT BEGIN 24\n', b'HMINPUT BEGIN 24\nHMTEST BEGIN 41\n'))

    def test_manifest_inventory_and_independent_expectations(self):
        fixtures = copy.deepcopy(FIXTURES); fixtures.pop('empty'); self.reject(fixtures=fixtures)
        for name in h.INPUTS:
            fixtures = copy.deepcopy(FIXTURES)
            fixtures[name]['input']['present'] = not fixtures[name]['input']['present']
            self.reject(fixtures=fixtures)
        fixtures = copy.deepcopy(FIXTURES); fixtures['binary256']['input']['hex'] = '00'; self.reject(fixtures=fixtures)

    def test_complete_text_does_not_hide_execution_failure(self):
        for changed in ({'returncode': 1}, {'returncode': None}, {'timed_out': True}, {'launch_error': 'QEMU failed'}):
            self.reject(execution={**EXECUTION, **changed})


if __name__ == '__main__':
    unittest.main()
