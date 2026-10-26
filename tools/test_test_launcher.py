#!/usr/bin/env python3
"""Mutation tests for normal monitor output, artifacts and actual-EL0 traces."""
import copy
import struct
import unittest

import build_apps as builder
import test_launcher as h
from test_boot_profiles import monitor_steps

PAGES = 1000
SYMBOLS = {'arm_vectors': 0x40200800, 'arm_process_vectors': 0x40210000,
           'arm_timer_window_begin': 0x40220000, 'arm_timer_window_end': 0x40220100}
APPS = {name: {'entry': 0x100000000, 'symbols': {'write_svc': 0x100000040, 'exit_svc': 0x100000050,
         'fault': 0x100000060, 'loop_start': 0x100000080, 'loop_end': 0x100000084}} for name in builder.APPS}
FIXUPS = [(0x40230000 + i * 32, 0x40230010 + i * 32, 0 if i in (0, 5) else 0x40300000 + i * 4096, esr)
          for i, esr in enumerate((0x96000006, 0x96000007, 0x96000007, 0x9600004f, 0x8600000f, 0x96000006))]
EXECUTION = {'returncode': -15, 'timed_out': False, 'launch_error': None, 'host_terminated': True,
             'host_killed': False, 'alive_after_last_prompt': True}


def serial(fresh=False, ram_mb=128):
    return h.banner(ram_mb, PAGES) + h.PROMPT + b''.join(response + h.PROMPT for _, _, response in h.steps(PAGES, fresh))


def trace(fresh=False):
    lines = []
    def writes(sequence):
        for offset, value in sequence:
            lines.append(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}')
    def returned(el, pc):
        lines.append(f'Exception return from AArch64 EL1 to AArch64 EL{el} PC {pc:#x}')
    def exception(number, source, esr, elr, vector, far=None):
        name = {2: 'SVC', 3: 'Prefetch Abort', 4: 'Data Abort', 5: 'IRQ'}[number]
        lines.extend([f'Taking exception {number} [{name}] on CPU 0', f'...from EL{source} to EL1',
                      f'...with ESR {esr >> 26:#x}/{esr:#x}'])
        if far is not None:
            lines.append(f'...with FAR {far:#x}')
        lines.extend([f'...with ELR {elr:#x}', f'...to EL1 PC {vector:#x} PSTATE 0x3c5'])
    def timer():
        writes(h.SETUP)
        for _ in range(8):
            exception(5, 1, 0, SYMBOLS['arm_timer_window_begin'], SYMBOLS['arm_vectors'] + 0x280)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            writes(((16, 30),))
            returned(1, SYMBOLS['arm_timer_window_begin'])
        writes(((0, 0),) + h.SETUP + ((0, 0),))
    def baseline():
        timer(); timer()
        for _ in range(4):
            for pc, resume, far, esr in FIXUPS:
                exception(3 if esr == 0x8600000f else 4, 1, esr, pc, SYMBOLS['arm_vectors'] + 0x200, far)
                returned(1, resume)
        writes((h.SETUP + ((0, 0),)) * 2)
    def launch(name):
        app = APPS[name]
        writes(h.SLICE_SETUP)
        returned(0, app['entry'])
        if name in ('hello', 'info'):
            for index, label in enumerate(('write_svc', 'exit_svc')):
                pc = app['symbols'][label] + 4
                exception(2, 0, 0x56000000, pc, SYMBOLS['arm_process_vectors'] + 0x400)
                writes(h.SLICE_RESTORE)
                if index == 0:
                    writes(h.SLICE_SETUP); returned(0, pc)
        elif name == 'fault':
            exception(4, 0, 0x9200004f, app['symbols']['fault'], SYMBOLS['arm_process_vectors'] + 0x400, app['entry'])
            writes(h.SLICE_RESTORE)
        else:
            exception(5, 0, 0, app['symbols']['loop_start'], SYMBOLS['arm_process_vectors'] + 0x480)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            writes(((16, 30),)); writes(h.SLICE_RESTORE)
    if not fresh:
        baseline()
    timer()
    if fresh:
        launch('hello')
    else:
        for name in ('info', 'hello', 'fault', 'loop'):
            launch(name)
        baseline(); launch('hello')
    return ('\n'.join(lines) + '\n').encode()


class LauncherTests(unittest.TestCase):
    def reject(self, raw=None, traced=None, execution=None, fresh=False):
        result = h.validate(serial(fresh) if raw is None else raw, trace(fresh) if traced is None else traced,
                            EXECUTION if execution is None else execution, APPS, SYMBOLS, FIXUPS, 128, PAGES, fresh)
        self.assertFalse(result['passed'], result)

    def test_complete_matrix_and_fresh_smoke(self):
        for fresh in (False, True):
            for ram in (64, 128):
                self.assertTrue(h.validate(serial(fresh, ram), trace(fresh), EXECUTION, APPS, SYMBOLS,
                                           FIXUPS, ram, PAGES, fresh)['passed'])

    def test_missing_duplicate_and_wrong_commands_or_prompt(self):
        raw = serial()
        for _, _, response in h.steps(PAGES):
            with self.subTest(response=response[:64]):
                self.reject(raw=raw.replace(response + h.PROMPT, b'', 1))
                self.reject(raw=raw.replace(response, response + response, 1))
        self.reject(raw=raw[:-len(h.PROMPT)])
        self.reject(raw=raw + b'Unexpected\r\n')
        self.reject(raw=raw.replace(b'\r\n', b'\n'))

    def test_output_and_outcome_are_exact_not_fake_success(self):
        raw = serial()
        for old, new in ((b'hello from EL0', b'hello from kernel'), (b'native ABI 1', b'native ABI 2'),
                         (b'Program faulted.', b'Program exited (status 0).'),
                         (b'Program timed out.', b'Program exited (status 0).'),
                         (b'Program exited (status 0).', b'Program exited (status 1).'),
                         (b'  demo/loop\r\n', b''), (b'console budget: 4096', b'console budget: 4095')):
            self.reject(raw=raw.replace(old, new, 1))
        self.reject(raw=raw.replace(b'hello from EL0', b'HMPROC OUTPUT hello from EL0', 1))

    def test_generation_reclamation_and_lazy_initialization(self):
        raw = serial()
        for old, new in ((b'Last prepared generation: none', b'Last prepared generation: 1'),
                         (b'Last prepared generation: 5', b'Last prepared generation: 1'),
                         (b'Last prepared generation: 3', b'Last prepared generation: 4'),
                         (b'Native launches: 5', b'Native launches: 4'),
                         (b'Free frames: 997', b'Free frames: 996'),
                         (b'Native launcher: not initialized', b'Native launcher: ready')):
            self.reject(raw=raw.replace(old, new, 1))
        self.reject(raw=serial(True).replace(b'Free frames: 1000', b'Free frames: 997'), fresh=True)
        self.reject(raw=serial(True).replace(b'Last prepared generation: 1', b'Last prepared generation: 2'), fresh=True)

    def test_missing_extra_or_wrong_el0_transitions(self):
        raw = trace()
        user_return = b'Exception return from AArch64 EL1 to AArch64 EL0 PC 0x100000000\n'
        self.reject(traced=raw.replace(user_return, b'', 1))
        self.reject(traced=raw + user_return)
        self.reject(traced=raw.replace(b'...from EL0 to EL1', b'...from EL1 to EL1', 1))
        self.reject(traced=raw.replace(b'...with ELR 0x100000044', b'...with ELR 0x100000048', 1))
        self.reject(traced=raw.replace(b'...with ESR 0x15/0x56000000', b'...with ESR 0x15/0x56000001', 1))
        self.reject(traced=raw + b'Taking exception 16 [Semihosting call] on CPU 0\n...from EL1 to EL1\n...handling as semihosting call 0x20\n')

    def test_fault_and_deadline_require_exact_lower_el_evidence(self):
        raw = trace()
        for old, new in ((b'...with FAR 0x100000000', b'...with FAR 0x100000004'),
                         (b'...with ESR 0x24/0x9200004f', b'...with ESR 0x24/0x92000007'),
                         (b'...with ELR 0x100000060', b'...with ELR 0x100000064'),
                         (b'...with ELR 0x100000080', b'...with ELR 0x100000084'),
                         (b'...to EL1 PC 0x40210480', b'...to EL1 PC 0x40200a80')):
            self.reject(traced=raw.replace(old, new, 1))
        self.reject(traced=raw.replace(b'Taking exception 5 [IRQ] on CPU 0\n...from EL0', b'Taking exception 2 [SVC] on CPU 0\n...from EL0', 1))

    def test_gic_preflight_and_baseline_are_exact(self):
        raw = trace()
        for line in (b'gic_cpu_write cpu 0 iface write at 0x00000004 0x000000ff\n',
                     b'gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n',
                     b'gic_acknowledge_irq cpu 0 acknowledged irq 30\n'):
            self.reject(traced=raw.replace(line, b'', 1))
            self.reject(traced=raw.replace(line, line + line, 1))
        self.reject(traced=raw.replace(b'...with ESR 0x25/0x96000006', b'...with ESR 0x25/0x96000007', 1))
        self.reject(traced=raw.replace(b'...with ELR 0x40220000', b'...with ELR 0x40220100', 1))
        self.reject(traced=raw.replace(b'CPU 0', b'CPU 1', 1))
        self.reject(traced=raw + b'unknown trace record\n')
        self.reject(traced=raw[:-1])

    def test_execution_failure_cannot_pass_complete_output(self):
        for changes in ({'returncode': 1}, {'returncode': None}, {'timed_out': True}, {'host_terminated': False},
                        {'host_killed': True}, {'alive_after_last_prompt': False}, {'launch_error': 'missing QEMU'}):
            self.reject(execution={**EXECUTION, **changes})

    def test_old_monitor_x86_unsupported_and_arm_three_frame_retention(self):
        x86 = {name: response for name, _, response in monitor_steps('x86_64', PAGES)}
        self.assertIn(b'unsupported', x86['status'])
        self.assertIn(b'Free frames: unavailable', x86['status_after_tests'])
        self.assertEqual(x86['unsupported_apps'], b'apps\nNative launching is unsupported on this platform.\n')
        self.assertEqual(x86['unsupported_run'], b'run bin/hello\nNative launching is unsupported on this platform.\n')
        arm = {name: response for name, _, response in monitor_steps('aarch64', PAGES)}
        self.assertIn(b'Free frames: 1000\n', arm['status'])
        self.assertIn(b'Free frames: 997\n', arm['status_after_tests'])
        self.assertIn(b'Last prepared generation: none', arm['status_after_tests'])

    def test_kernel_fixup_table_bounds_permissions_and_exact_inventory(self):
        # Two file-backed LOADs: executable probes/resumes, then immutable table/data.
        kernel = bytearray(0x3000)
        kernel[:7] = b'\x7fELF\x02\x01\x01'
        struct.pack_into('<Q', kernel, 32, 64)
        struct.pack_into('<HH', kernel, 54, 56, 2)
        struct.pack_into('<IIQQQQQQ', kernel, 64, 1, 5, 0x1000, 0x40200000, 0x40200000, 0x1000, 0x1000, 4096)
        struct.pack_into('<IIQQQQQQ', kernel, 120, 1, 4, 0x2000, 0x40201000, 0x40201000, 0x1000, 0x1000, 4096)
        symbols = {'arm_fault_fixups': 0x40201000, '__stack_guard': 0x40300000,
                   '__exception_stack_guard': 0x40302000, 'arm_probe_ro_datum': 0x40201200,
                   'arm_probe_nx_target': 0x40201204}
        fars = (0, symbols['__stack_guard'], symbols['__exception_stack_guard'],
                symbols['arm_probe_ro_datum'], symbols['arm_probe_nx_target'], 0)
        syndromes = (0x96000006, 0x96000007, 0x96000007, 0x9600004f, 0x8600000f, 0x96000006)
        rows = []
        for i, (far, esr) in enumerate(zip(fars, syndromes)):
            pc = symbols['arm_probe_nx_target'] if i == 4 else 0x40200000 + i * 32
            row = (pc, 0x40200100 + i * 32, far, esr)
            rows.append(row)
            struct.pack_into('<QQQQ', kernel, 0x2000 + i * 32, *row)
        self.assertEqual(h.fixup_table(kernel, symbols), rows)
        mutations = [
            (0x2000 + 24, 0x96000007),  # Wrong syndrome even with a valid table row.
            (0x2000 + 32 + 16, 0),    # Stack guard may not authorize a null access.
            (0x2000 + 32, rows[0][0]),
            (0x2000 + 32 + 8, rows[0][1]),
            (0x2000 + 8, 0x40201200), # Resume in non-executable file storage.
            (0x2000 + 8, 0x40202000), # Resume beyond all file-backed LOADs.
            (0x2000, 0x40200001),     # Unaligned fault PC.
            (120 + 32, 64),           # Truncated file span cannot contain the table.
            (120 + 8, len(kernel)),   # LOAD points past the actual input bytes.
            (32, 2**64 - 1),          # Program-header interval overflow.
        ]
        for offset, value in mutations:
            bad = bytearray(kernel)
            struct.pack_into('<Q', bad, offset, value)
            with self.subTest(offset=offset, value=value), self.assertRaises(ValueError):
                h.fixup_table(bad, symbols)
        for flags in (5, 6, 0):
            bad = bytearray(kernel)
            struct.pack_into('<I', bad, 120 + 4, flags)
            with self.subTest(flags=flags), self.assertRaises(ValueError):
                h.fixup_table(bad, symbols)
        for data in (kernel[:63], kernel[:150], kernel[:0x2080]):
            with self.subTest(length=len(data)), self.assertRaises(ValueError):
                h.fixup_table(data, symbols)

    def test_app_elf_opcodes_bounds_and_kernel_independence(self):
        data = bytearray(4096 + 64)
        data[:7] = b'\x7fELF\x02\x01\x01'
        struct.pack_into('<HHIQQ', data, 16, 2, 183, 1, 0x100000000, 64)
        struct.pack_into('<HH', data, 54, 56, 1)
        struct.pack_into('<IIQQQQQQ', data, 64, 1, 5, 4096, 0x100000000, 0x100000000, 64, 64, 4096)
        displacement = (-12) & ((1 << 21) - 1)
        adr = 0x10000009 | ((displacement & 3) << 29) | ((displacement >> 2) << 5)
        struct.pack_into('<6I', data, 4096, 0xd503201f, 0xd4000001, 0xd4000001, adr, 0xf900013f, 0x14000000)
        names = dict(native_start=0, write_svc=4, exit_svc=8, fault=16, loop_start=20, loop_end=24)
        symbols = '\n'.join(f'{i}: {0x100000000 + off:016x} 0 NOTYPE GLOBAL DEFAULT 1 {name}' for i, (name, off) in enumerate(names.items()))
        self.assertEqual(builder.inspect(data, symbols)['entry'], 0x100000000)
        for offset, value in ((4096 + 4, 0xd4000021), (4096 + 16, 0xf900003f), (4096 + 20, 0x14000001),
                              (4096 + 12, 0x10000009)):
            bad = bytearray(data); struct.pack_into('<I', bad, offset, value)
            with self.assertRaises(ValueError): builder.inspect(bad, symbols)
        for extra in ('\n9: 0000000040200000 0 NOTYPE GLOBAL DEFAULT ABS kernel_text',
                      '\n9: 0000000040300000 0 NOTYPE GLOBAL DEFAULT ABS kernel_data',
                      '\n9: 0000000000000000 0 NOTYPE GLOBAL DEFAULT UND unresolved',
                      '\n9: 0000000100000004 0 NOTYPE GLOBAL DEFAULT 1 write_svc'):
            with self.assertRaises(ValueError): builder.inspect(data, symbols + extra)
        bad = bytearray(data); struct.pack_into('<Q', bad, 32, 2**64 - 1)
        with self.assertRaises(ValueError): builder.inspect(bad, symbols)


if __name__ == '__main__':
    unittest.main()
