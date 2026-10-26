#!/usr/bin/env python3
"""Failure-sensitive native-process report and pinned-QEMU trace parser tests."""
import unittest
import copy
import hashlib
import json
from pathlib import Path
import struct
import tempfile
from unittest.mock import patch
import test_process as h
from test_kernel import CASES as BASELINE

EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}
SYMBOLS = {'arm_root': 0x40200000, 'arm_vectors': 0x40200800, 'arm_process_vectors': 0x40210000,
           'arm_timer_window_begin': 0x40220000, 'arm_timer_window_end': 0x40220100,
           '__text_start': 0x40201000, '__bss_start': 0x40300000}
FIXTURES = {case: {'entry': h.BASE, 'pages': 5, 'symbols': {
    'write_svc': h.BASE + 0x40, 'exit_svc': h.BASE + 0x50, 'fault': h.BASE + 0x60,
    'loop_start': h.BASE + 0x80, 'loop_end': h.BASE + 0x90,
    'data': h.BASE + 0x1000, 'message': h.BASE + 0x1100,
    'register_svc_1': h.BASE + 0xa0, 'register_svc_2': h.BASE + 0xb0,
    'register_svc_3': h.BASE + 0xc0}} for case in h.CASES}


def record(kind, **values):
    return 'HMPROC ' + kind + ''.join(f' {key}={values[key]}' for key in h.FIELDS[kind].split())


def console(cpus=1):
    lines = ['hematite bootstrap', 'HMPROC PREFLIGHT checks=15 irqs=8',
             'HMPROC ARCHIVE bytes=4096 entries=36 files=34',
             'HMPROC ADMISSION case=malformed error=archive before=1000 after=1000',
             'HMPROC ADMISSION case=missing error=missing before=1000 after=1000',
             'HMPROC ADMISSION case=directory error=directory before=1000 after=1000',
             'HMPROC ADMISSION case=invalidelf error=elf before=1000 after=1000', 'HMPROC BEGIN 66',
             record('INFO', wire=1, cpus=cpus, peer_off=1, abi=1, max_write=256,
                    byte_budget=4096, call_limit=64, timer_frequency=62500000, timer_ticks=6250000)]
    for index in range(66):
        case, fixture = h.CASES[index % 33], FIXTURES[h.CASES[index % 33]]
        handle = 0 if case == 'nogrant' else ((index + 1) << 32) | 1
        lines.append(record('RUN', index=index, **{'pass': index // 33}, case=case, fixture=case,
                            entry=h.BASE, root=0x40400000, sp=h.STACK, handle=handle, before=1000, during=966, owned=34))
        calls = h.expected_calls(case, fixture, handle, index)
        written = 0
        for ordinal, call in enumerate(calls, 1):
            lines.append(record('CALL', index=index, ordinal=ordinal, **{k: call[k] for k in
                                ('pc', 'version', 'operation', 'arg1', 'arg2', 'arg3')}))
            if call['output']:
                lines.append(record('OUTPUT', index=index, ordinal=ordinal, length=len(call['output']), hex=call['output'].hex()))
                written += len(call['output'])
            if call['status'] is not None:
                lines.append(record('RESULT', index=index, ordinal=ordinal, status=call['status'], bytes=call['bytes']))
        reason = 'calllimit' if case in ('calllimit', 'invalidstorm') else 'deadline' if case == 'loop' else 'exit' if calls else 'fault'
        if reason in ('exit', 'calllimit'):
            esr, far, elr = 0x56000000, 0, calls[-1]['pc']
        elif reason == 'deadline':
            esr, far, elr = 0, 0, fixture['symbols']['loop_start']
        else:
            fault = h.fault_contract(case, fixture, SYMBOLS)
            esr = fault.get('exact', (fault.get('ec', 0) << 26) | (1 << 25) | fault.get('fsc', 0) | (64 if fault.get('write') else 0))
            far, elr = fault['far'], fault['elr']
        lines.append(record('STOP', index=index, reason=reason, code=0, calls=len(calls), written=written,
                            esr=esr, far=far, elr=elr,
                            spsr=0x00400740 if case in ('rwexecute', 'stackexecute') else 0x00400340, irqs=int(case == 'loop')))
        lines.append(record('RESTORE', index=index, root=SYMBOLS['arm_root'], vbar=SYMBOLS['arm_vectors'],
                            controls=1, inactive=1, before=1000, after=1000))
        lines.append(record('PASS', index=index, **{'pass': index // 33}, case=case))
    lines += ['HMPROC PREPARE budget=1 exhausted=1 drop=1 before=1000 after=1000',
              'HMPROC END 66', f'HMTEST BEGIN {len(BASELINE)}']
    lines += [f'HMTEST PASS {name}' for name in BASELINE]
    lines += [f'HMTEST END {len(BASELINE)}']
    return ('\n'.join(lines) + '\n').encode()


def trace(raw, cpus=1):
    runs = h.validate_serial(raw, FIXTURES, SYMBOLS, cpus, 4096)['runs']
    lines = []
    def write(offset, value):
        lines.append(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}')
    def setup():
        for offset, value in ((0, 0), (4, 255), (8, 0), (0, 1)):
            write(offset, value)
    def exception(number, name, source, esr, elr, vector, far=None):
        lines.extend([f'Taking exception {number} [{name}] on CPU 0', f'...from EL{source} to EL1',
                      f'...with ESR {esr >> 26:#x}/{esr:#x}'])
        if far is not None:
            lines.append(f'...with FAR {far:#x}')
        lines.extend([f'...with ELR {elr:#x}', f'...to EL1 PC {vector:#x} PSTATE 0x3c5'])
    def returned(el, pc):
        lines.append(f'Exception return from AArch64 EL1 to AArch64 EL{el} PC {pc:#x}')
    def timer_pair():
        setup()
        for _ in range(8):
            pc = SYMBOLS['arm_timer_window_begin'] + 4
            exception(5, 'IRQ', 1, 0, pc, SYMBOLS['arm_vectors'] + 0x280)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            write(16, 30)
            returned(1, pc)
        write(0, 0)
        setup()
        write(0, 0)
    timer_pair()
    for run in runs:
        if cpus == 2:
            for _ in range(2):
                lines.extend(['Taking exception 11 [Hypervisor Call] on CPU 0', '...from EL1 to EL2',
                              '...with ESR 0x16/0x5a000000', '...handled as PSCI call'])
        for offset, value in ((4, 255), (8, 0), (0, 1)):
            write(offset, value)
        returned(0, run['entry'])
        for ordinal, call in enumerate(run['calls'], 1):
            exception(2, 'SVC', 0, 0x56000000, call['pc'], SYMBOLS['arm_process_vectors'] + 0x400)
            for offset, value in ((0, 0), (4, 255), (8, 0), (0, 0)):
                write(offset, value)
            if ordinal < len(run['calls']):
                for offset, value in ((4, 255), (8, 0), (0, 1)):
                    write(offset, value)
                returned(0, call['pc'])
        if not run['calls']:
            stop = run['stop']
            if run['case'] == 'loop':
                exception(5, 'IRQ', 0, 0, stop['elr'], SYMBOLS['arm_process_vectors'] + 0x480)
                lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
                write(16, 30)
            else:
                ec = stop['esr'] >> 26
                number, name = (3, 'Prefetch Abort') if ec == 0x20 else (4, 'Data Abort') if ec == 0x24 else (2, 'SVC') if ec == 0x15 else (1, 'Undefined Instruction')
                exception(number, name, 0, stop['esr'], stop['elr'], SYMBOLS['arm_process_vectors'] + 0x400,
                          stop['far'] if ec in (0x20, 0x24) else None)
            for offset, value in ((0, 0), (4, 255), (8, 0), (0, 0)):
                write(offset, value)
    # One baseline EL1 fault distinguishes process restoration from baseline setup.
    exception(4, 'Data Abort', 1, 0x96000006, SYMBOLS['__text_start'], SYMBOLS['arm_vectors'] + 0x200, 0)
    returned(1, SYMBOLS['__text_start'] + 4)
    timer_pair()
    timer_pair()
    lines.extend(['Taking exception 16 [Semihosting call] on CPU 0', '...from EL1 to EL1',
                  '...handling as semihosting call 0x20'])
    return ('\n'.join(lines) + '\n').encode()


class ProcessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = console()
        cls.trace = trace(cls.raw)

    def reject(self, raw=None, trace_data=None, execution=EXECUTION, fixtures=FIXTURES, symbols=SYMBOLS, cpus=1):
        result = h.validate(self.raw if raw is None else raw, self.trace if trace_data is None else trace_data,
                            execution, fixtures, symbols, cpus, 4096)
        self.assertFalse(result['passed'])

    def test_archive_admission_inventory(self):
        for line in self.raw.splitlines(keepends=True):
            if line.startswith((b'HMPROC ARCHIVE', b'HMPROC ADMISSION')):
                self.reject(raw=self.raw.replace(line, b'', 1))
                self.reject(raw=self.raw.replace(line, line + line, 1))
                for old, new in ((b'4096', b'4097'), (b'36', b'35'), (b'34', b'33'),
                                 (b'after=1000', b'after=999'), (b'before=1000', b'before=999'),
                                 (b'error=archive', b'error=elf'), (b'error=missing', b'error=elf'),
                                 (b'error=directory', b'error=missing'), (b'error=elf', b'error=archive')):
                    if old in line:
                        self.reject(raw=self.raw.replace(line, line.replace(old, new), 1))
        rows = [line for line in self.raw.splitlines(keepends=True) if line.startswith(b'HMPROC ADMISSION')]
        self.reject(raw=self.raw.replace(rows[0] + rows[1], rows[1] + rows[0]))
        bad = self.raw
        for row in rows:
            bad = bad.replace(row, row.replace(b'1000', b'999'))
        self.reject(raw=bad)

    def test_archive_embedding_exact_readonly_and_no_raw_fallback(self):
        payload = b'ELF fixture payload'
        archive = b'cpio prefix' + payload + b'cpio suffix'
        def kernel(data, flags=4):
            header = bytearray(120)
            header[:6] = b'\x7fELF\x02\x01'
            struct.pack_into('<Q', header, 32, 64)
            struct.pack_into('<HH', header, 54, 56, 1)
            struct.pack_into('<IIQQQQQQ', header, 64, 1, flags, 120, 0x40000000, 0x40000000, len(data), len(data), 1)
            return bytes(header) + data
        self.assertEqual(h.embedded_archive(kernel(archive), archive, [payload]), 120)
        for data, flags in ((archive, 6), (archive, 0), (archive + archive, 4),
                            (archive + payload, 4), (payload + archive, 4), (archive[:-1], 4)):
            with self.assertRaises(ValueError):
                h.embedded_archive(kernel(data, flags), archive, [payload])
        broken = bytearray(kernel(archive))
        struct.pack_into('<Q', broken, 96, 2**64 - 1)
        with self.assertRaises(ValueError):
            h.embedded_archive(bytes(broken), archive, [payload])

    def test_archive_across_multiple_loads(self):
        payload = b'complete native ELF payload'
        archive = b'archive begin' + payload + b'archive end'
        def kernel(parts, overlap=False):
            start = 64 + 56 * len(parts)
            header = bytearray(start)
            header[:6] = b'\x7fELF\x02\x01'
            struct.pack_into('<Q', header, 32, 64)
            struct.pack_into('<HH', header, 54, 56, len(parts))
            data = b''
            for i, (part, flags) in enumerate(parts):
                offset = start + len(data)
                if overlap and i == 1:
                    offset = start
                struct.pack_into('<IIQQQQQQ', header, 64 + i * 56, 1, flags, offset,
                                 0x40000000 + i * 0x10000, 0x40000000 + i * 0x10000,
                                 len(part), len(part), 1)
                data += part
            return bytes(header) + data
        self.assertEqual(h.embedded_archive(kernel([(b'unrelated', 6), (archive, 4)]), archive, [payload]), 185)
        bad = [kernel([(archive, 4), (archive, 6)]),
               kernel([(archive[:20], 4), (archive[20:], 4)]),
               kernel([(archive, 4), (payload, 6)]),
               kernel([(archive, 4), (b'overlap', 6)], overlap=True)]
        for data in bad:
            with self.assertRaises(ValueError):
                h.embedded_archive(data, archive, [payload])

    def test_complete_one_and_two_cpu_inventory(self):
        for cpus in (1, 2):
            raw = console(cpus)
            result = h.validate(raw, trace(raw, cpus), EXECUTION, FIXTURES, SYMBOLS, cpus, 4096)
            self.assertTrue(result['passed'], result['errors'])

    def test_each_case_run_required_once_in_order(self):
        for line in self.raw.splitlines(keepends=True):
            if line.startswith(b'HMPROC RUN'):
                with self.subTest(line=line[:100]):
                    self.reject(self.raw.replace(line, b'', 1))
                    self.reject(self.raw.replace(line, line + line, 1))

    def test_every_record_kind_required(self):
        for kind in h.FIELDS:
            line = next(line for line in self.raw.splitlines(keepends=True) if line.startswith(('HMPROC ' + kind + ' ').encode()))
            self.reject(self.raw.replace(line, b'', 1))
            self.reject(self.raw.replace(line, line + line, 1))

    def test_serial_contract_mutations(self):
        for before, after in [(b'checks=15', b'checks=7'), (b'irqs=8', b'irqs=16'),
                (b'timer_ticks=6250000', b'timer_ticks=6250001'), (b'peer_off=1', b'peer_off=0'),
                (b'during=966', b'during=967'), (b'owned=34', b'owned=33'), (b'after=1000', b'after=999'),
                (b'controls=1', b'controls=0'), (b'inactive=1', b'inactive=0'),
                (b'handle=4294967297', b'handle=1'), (b'code=0', b'code=1'),
                (b'reason=deadline', b'reason=exit'), (b'status=6', b'status=0'),
                (b'hex=68656c6c6f', b'hex=00656c6c6f'), (b'calls=64', b'calls=63'),
                (b'written=4096', b'written=4095'), (b'spsr=4195136', b'spsr=4195264')]:
            with self.subTest(after=after):
                self.assertIn(before, self.raw)
                self.reject(self.raw.replace(before, after, 1))

    def test_preparation_rollback_and_inactive_drop_are_mandatory(self):
        record = b'HMPROC PREPARE budget=1 exhausted=1 drop=1 before=1000 after=1000\n'
        for before, after in ((b'budget=1', b'budget=0'), (b'exhausted=1', b'exhausted=0'),
                              (b'drop=1', b'drop=0'), (b'before=1000', b'before=999'),
                              (b'after=1000', b'after=999')):
            self.reject(self.raw.replace(record, record.replace(before, after)))
        without = self.raw.replace(record, b'')
        self.reject(without.replace(b'HMPROC BEGIN 66\n', b'HMPROC BEGIN 66\n' + record))
        self.reject(without + record)

    def test_btype_is_exact_and_only_for_indirect_nx_faults(self):
        for index in (0, 9, 10, 42, 43):
            line = next(line for line in self.raw.splitlines(keepends=True)
                        if line.startswith(f'HMPROC STOP index={index} '.encode()))
            old = h.fields(line.decode().strip(), 'STOP')['spsr']
            expected = 1 if index in (9, 10, 42, 43) else 0
            for btype in set(range(4)) - {expected}:
                new = (old & ~0xc00) | (btype << 10)
                self.reject(self.raw.replace(line, line.replace(f'spsr={old}'.encode(), f'spsr={new}'.encode())))

    def test_pinned_daif_quirk_requires_one_exact_syndrome(self):
        self.assertEqual(h.fault_contract('daifmask', FIXTURES['daifmask'], SYMBOLS)['exact'], 0x620793e4)
        for index in (16, 49):
            line = next(line for line in self.raw.splitlines(keepends=True)
                        if line.startswith(f'HMPROC STOP index={index} '.encode()))
            old = h.fields(line.decode().strip(), 'STOP')['esr']
            for incorrect in (0x620cd3e4, 0x620793e5, 0x6232fbe4, 0x02000000):
                self.reject(self.raw.replace(line, line.replace(f'esr={old}'.encode(), f'esr={incorrect}'.encode())))
        self.reject(trace_data=self.trace.replace(b'0x18/0x620793e4', b'0x18/0x620cd3e4', 1))

    def test_fault_syndrome_address_mutations(self):
        for line in self.raw.splitlines(keepends=True):
            if line.startswith(b'HMPROC STOP') and b'reason=fault' in line:
                parsed = h.fields(line.decode().strip(), 'STOP')
                for name in ('esr', 'far', 'elr'):
                    self.reject(self.raw.replace(line, line.replace(f'{name}={parsed[name]}'.encode(),
                                                               f'{name}={parsed[name] ^ 1}'.encode()), 1))

    def test_baseline_execution_and_artifact_failures(self):
        split = self.raw.index(b'HMTEST BEGIN')
        self.reject(self.raw[:split])
        self.reject(self.raw[split:] + self.raw[:split])
        for update in ({'returncode': 1}, {'returncode': None}, {'timed_out': True}, {'launch_error': 'missing QEMU'}):
            self.reject(execution={**EXECUTION, **update})
        self.reject(fixtures={})
        self.reject(symbols={})

    def test_malformed_truncated_and_foreign_report(self):
        for raw in (self.raw + b'HMPROC END 66', self.raw.replace(b'cpus=1', b'cpus=01'),
                    self.raw.replace(b'cpus=1', b'cpus=' + b'9' * 5000),
                    self.raw.replace(b'HMPROC INFO', b'\nHMPROC INFO'),
                    self.raw.replace(b'HMPROC INFO', b'junk\nHMPROC INFO'),
                    self.raw.replace(b'HMPROC INFO', b'\xffHMPROC INFO'),
                    self.raw + b'HMEXCEPTION FATAL\n'):
            self.reject(raw)

    def test_el1_cannot_substitute_for_el0(self):
        for before, after in ((b'...from EL0 to EL1', b'...from EL1 to EL1'),
                (b'to AArch64 EL0 PC', b'to AArch64 EL1 PC'),
                (b'...with ESR 0x24/0x9200004f', b'...with ESR 0x25/0x9600004f')):
            self.assertIn(before, self.trace)
            self.reject(trace_data=self.trace.replace(before, after, 1))

    def test_missing_duplicate_exception_fields_and_resumes(self):
        for prefix in (b'...from EL0', b'...with ESR 0x15', b'...with FAR', b'...with ELR', b'...to EL1',
                       b'Exception return from AArch64 EL1 to AArch64 EL0'):
            line = next(line for line in self.trace.splitlines(keepends=True) if line.startswith(prefix))
            self.reject(trace_data=self.trace.replace(line, b'', 1))
            self.reject(trace_data=self.trace.replace(line, line + line, 1))

    def test_trace_cpu_timer_semihost_and_order(self):
        for bad in (b'', self.trace[:-1], self.trace + b'unknown\n', self.trace.replace(b'on CPU 0', b'on CPU 1', 1),
                    self.trace.replace(b'acknowledged irq 30', b'acknowledged irq 29', 1),
                    self.trace.replace(b'0x00000010 0x0000001e', b'0x00000010 0x0000001d', 1),
                    self.trace.replace(b'...from EL1 to EL1\n...handling as semihosting', b'...from EL0 to EL1\n...handling as semihosting')):
            self.reject(trace_data=bad)

    def test_each_slice_requires_exact_gic_setup_and_restoration(self):
        lines = self.trace.splitlines(keepends=True)
        entry = next(i for i, line in enumerate(lines) if b'to AArch64 EL0' in line)
        start = entry - 3
        setup = lines[start:entry]
        for replacement in ([], setup + setup, list(reversed(setup))):
            self.reject(trace_data=b''.join(lines[:start] + replacement + lines[entry:]))
        trap = next(i for i, line in enumerate(lines) if line == b'...from EL0 to EL1\n')
        restored = next(i for i in range(trap, len(lines)) if lines[i].startswith(b'gic_cpu_write'))
        cleanup = lines[restored:restored + 4]
        for replacement in ([], cleanup + cleanup, list(reversed(cleanup))):
            self.reject(trace_data=b''.join(lines[:restored] + replacement + lines[restored + 4:]))

    def test_loop_requires_lower_el_timer_not_host_or_svc_completion(self):
        irq = b'Taking exception 5 [IRQ] on CPU 0\n...from EL0 to EL1'
        self.assertEqual(self.trace.count(irq), 2)
        self.reject(trace_data=self.trace.replace(irq, irq.replace(b'EL0', b'EL1'), 1))
        self.reject(trace_data=self.trace.replace(irq, b'Taking exception 2 [SVC] on CPU 0\n...from EL0 to EL1', 1))
        self.reject(execution={**EXECUTION, 'timed_out': True, 'returncode': -9})
        extra = f'Exception return from AArch64 EL1 to AArch64 EL0 PC {h.BASE:#x}\n'.encode()
        terminal = b'Taking exception 16 [Semihosting call] on CPU 0'
        self.reject(trace_data=self.trace.replace(terminal, extra + terminal))

    def test_exact_two_cpu_psci_pairs(self):
        raw = console(2)
        traced = trace(raw, 2)
        hvc = (b'Taking exception 11 [Hypervisor Call] on CPU 0\n...from EL1 to EL2\n'
               b'...with ESR 0x16/0x5a000000\n...handled as PSCI call\n')
        self.assertEqual(traced.count(hvc), 132)
        for bad in (traced.replace(hvc, b'', 1), traced.replace(hvc, hvc + hvc, 1)):
            self.reject(raw, bad, cpus=2)

    def test_manifest_layout_hash_and_actual_kernel_binding(self):
        with tempfile.TemporaryDirectory(prefix='hematite-process-metadata-') as temporary:
            root = Path(temporary)
            manifest = {'version': 1, 'kernel_text': SYMBOLS['__text_start'],
                        'kernel_data': SYMBOLS['__bss_start'], 'fixtures': {}}
            actual_symbols = {}
            for case in h.CASES:
                data = bytearray(0x2100)
                data[:7] = b'\x7fELF\x02\x01\x01'
                struct.pack_into('<HHI', data, 16, 2, 183, 1)
                struct.pack_into('<QQ', data, 24, h.BASE, 64)
                struct.pack_into('<HH', data, 54, 56, 2)
                segments = []
                for n, (flags, offset, va, filesz, memsz) in enumerate(((5, 0x1000, h.BASE, 0x100, 0x100),
                        (6, 0x2000, h.BASE + 0x1000, 0x100, 0x1000))):
                    struct.pack_into('<IIQQQQQQ', data, 64 + n * 56, 1, flags, offset, va, 0, filesz, memsz, 4096)
                    segments.append(dict(flags=flags, offset=offset, va=va, filesz=filesz, memsz=memsz,
                                         align=4096, start=va, end=(va + memsz + 4095) & ~4095))
                symbols = {**FIXTURES[case]['symbols'], 'native_start': h.BASE,
                           'kernel_text': SYMBOLS['__text_start'], 'kernel_data': SYMBOLS['__bss_start']}
                for name in ('write_svc', 'exit_svc', 'register_svc_1', 'register_svc_2', 'register_svc_3'):
                    struct.pack_into('<I', data, 0x1000 + symbols[name] - h.BASE, 0xd4000001)
                opcode = {'svcimm': 0xd4000021, 'semihost': 0xd45e0000, 'cntpdisable': 0xd51be23f,
                          'daifmask': 0xd50342df}.get(case, 0xd4200000)
                struct.pack_into('<I', data, 0x1060, opcode)
                struct.pack_into('<I', data, 0x1080, 0x14000000)
                path = root / (case + '.elf')
                path.write_bytes(data)
                manifest['fixtures'][case] = dict(file=path.name, sha256=hashlib.sha256(data).hexdigest(),
                                                 entry=h.BASE, pages=2, symbols=symbols, segments=segments)
                actual_symbols[case] = symbols
            path = root / 'manifest.json'
            def checked(candidate, kernel=SYMBOLS):
                path.write_text(json.dumps(candidate))
                return h.check_fixture_manifest(path, root, kernel)
            with patch.object(h, 'read_symbols', side_effect=lambda file, output, label, required: actual_symbols[label.removeprefix('fixture-')]):
                self.assertEqual(set(checked(manifest)), set(h.CASES))
                changed = copy.deepcopy(manifest)
                changed['kernel_data'] += 4096
                with self.assertRaisesRegex(ValueError, 'different kernel'):
                    checked(changed)
                with self.assertRaisesRegex(ValueError, 'different kernel'):
                    checked(manifest, {**SYMBOLS, '__text_start': SYMBOLS['__text_start'] + 4096})
                for field in ('flags', 'offset', 'va', 'filesz', 'memsz', 'align', 'start', 'end'):
                    changed = copy.deepcopy(manifest)
                    changed['fixtures']['hello']['segments'][0][field] += 1
                    with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'segment metadata'):
                        checked(changed)
                changed = copy.deepcopy(manifest)
                changed['fixtures']['hello']['sha256'] = '0' * 64
                with self.assertRaisesRegex(ValueError, 'hash/size'):
                    checked(changed)
                original = actual_symbols['hello']['kernel_data']
                actual_symbols['hello']['kernel_data'] += 4096
                with self.assertRaisesRegex(ValueError, 'absolute symbols'):
                    checked(manifest)
                actual_symbols['hello']['kernel_data'] = original

    def test_expected_budget_and_call_limit_contracts(self):
        fixture = FIXTURES['bytebudget']
        calls = h.expected_calls('bytebudget', fixture, (1 << 32) | 1, 0)
        self.assertEqual(len(calls), 19)
        self.assertEqual(sum(len(c['output']) for c in calls), 4096)
        self.assertEqual(calls[16]['status'], 6)
        self.assertIsNone(calls[-1]['status'])
        self.assertEqual(len(h.expected_calls('calllimit', fixture, 1, 0)), 64)
        self.assertIsNone(h.expected_calls('exit64', fixture, 1, 0)[-1]['status'])


if __name__ == '__main__':
    unittest.main()
