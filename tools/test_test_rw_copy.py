#!/usr/bin/env python3
"""Synthetic mutation coverage only; these inputs are not guest evidence."""
import struct
import unittest
from unittest.mock import patch
import build_rw_copy as b
import test_rw_copy as h
from test_kernel import CASES as BASELINE
from test_test_launcher import SYMBOLS as OLD_SYMBOLS, FIXUPS
from test_launcher import SETUP

SYMBOLS = dict(OLD_SYMBOLS, arm_root=0x40200000, __kernel_end=0x40400000)
EXECUTION = dict(returncode=0, timed_out=False, launch_error=None)


def serial():
    lines = ['HMRWCOPY BEGIN passes=2 cases=12']
    for p in range(2):
        for name in h.CASES:
            lines.append(f'HMRWCOPY CASE pass={p} name={name} before=1000')
            if name == 'translations':
                for stage in range(2):
                    lines.append(f'HMRWCOPY ROOT pass={p} stage={stage} during=0x40500000 restored=0x40200000 controls=1')
                    for index in range(12):
                        slot = h.SLOTS[index // 2]; access = 'rw'[index % 2]
                        physical = 0 if slot == 495 else 0x40600000 + (index // 2) * 4096
                        active = 15 if slot == 495 else (31 if access == 'w' and slot not in (2, 3) else physical)
                        lines.append(f'HMRWCOPY PROBE pass={p} stage={stage} index={index} va={b.BASE + slot * 4096:#x} access={access} before=0xb active={active:#x} after=0xb physical={physical:#x}')
            lines.append(f'HMRWCOPY PASS pass={p} name={name} frames=35 during=965 after=1000')
    lines += ['HMRWCOPY END cases=24', f'HMTEST BEGIN {len(BASELINE)}']
    lines += [f'HMTEST PASS {name}' for name in BASELINE]
    lines += [f'HMTEST END {len(BASELINE)}', 'Bootstrap tests passed.']
    return ('\n'.join(lines) + '\n').encode()


def trace():
    lines = []
    def writes(items):
        lines.extend(f'gic_cpu_write cpu 0 iface write at {offset:#x} {value:#x}' for offset, value in items)
    def returned(pc): lines.append(f'Exception return from AArch64 EL1 to AArch64 EL1 PC {pc:#x}')
    def exception(number, name, esr, pc, vector, far=None):
        lines.extend([f'Taking exception {number} [{name}] on CPU 0', '...from EL1 to EL1', f'...with ESR {esr >> 26:#x}/{esr:#x}'])
        if far is not None: lines.append(f'...with FAR {far:#x}')
        lines.extend([f'...with ELR {pc:#x}', f'...to EL1 PC {vector:#x} PSTATE 0x3c5'])
    for _ in range(2):
        writes(SETUP)
        for _ in range(8):
            exception(5, 'IRQ', 0, SYMBOLS['arm_timer_window_begin'], SYMBOLS['arm_vectors'] + 0x280)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            writes(((16, 30),)); returned(SYMBOLS['arm_timer_window_begin'])
        writes(((0, 0),) + SETUP + ((0, 0),))
    for _ in range(4):
        for pc, resume, far, esr in FIXUPS:
            exception(3 if esr == 0x8600000f else 4, 'Prefetch Abort' if esr == 0x8600000f else 'Data Abort', esr, pc, SYMBOLS['arm_vectors'] + 0x200, far)
            returned(resume)
    writes((SETUP + ((0, 0),)) * 2)
    lines += ['Taking exception 16 [Semihosting call] on CPU 0', '...from EL1 to EL1', '...handling as semihosting call 0x20']
    return ('\n'.join(lines) + '\n').encode()


def fixture():
    data = bytearray(7 * 4096)
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHIQQQ', data, 16, 2, 183, 1, b.BASE + 4096, 64, 0)
    struct.pack_into('<HH', data, 54, 56, 5)
    offset = 4096
    for i, (slot, flags, size) in enumerate(b.GEOMETRY):
        struct.pack_into('<IIQQQQQQ', data, 64 + i * 56, 1, flags, offset, b.BASE + slot * 4096, b.BASE + slot * 4096, size, size, 4096)
        payload = {0: b'\x10' * 4096, 1: b'\0\0\0\x14' + b'\x21' * 4092, 2: b'\x32' * 4096 + b'\x43' * 4096, 4: b'\x54' * 4096, 6: b'\x76' * 4096}[slot]
        data[offset:offset + size] = payload; offset += size
    return bytes(data)


class CopyTests(unittest.TestCase):
    def test_full_synthetic_profile_and_execution_gate(self):
        with patch.object(h, 'TRACE_GRAMMAR_REVIEWED', True):
            self.assertTrue(h.validate(serial(), trace(), EXECUTION, SYMBOLS, FIXUPS)['passed'])
            for change in (dict(returncode=1), dict(returncode=None), dict(timed_out=True), dict(launch_error='missing')):
                self.assertFalse(h.validate(serial(), trace(), {**EXECUTION, **change}, SYMBOLS, FIXUPS)['passed'])
        with patch.object(h, 'TRACE_GRAMMAR_REVIEWED', False):
            self.assertFalse(h.validate(serial(), trace(), EXECUTION, SYMBOLS, FIXUPS)['passed'])

    def test_every_copy_record_mandatory_exactly_once(self):
        raw = serial()
        for line in raw.splitlines(keepends=True):
            if line.startswith(b'HMRWCOPY'):
                for bad in (raw.replace(line, b'', 1), raw.replace(line, line + line, 1)):
                    with self.subTest(line=line), self.assertRaises(ValueError): h.validate_serial(bad, SYMBOLS)

    def test_permission_address_and_resource_mutations(self):
        raw = serial()
        for old, new in ((b'frames=35', b'frames=34'), (b'after=1000', b'after=999'), (b'before=1000', b'before=1001'),
                         (b'active=0x1f', b'active=0x20'), (b'active=0xf', b'active=0xb'), (b'before=0xb', b'before=0x20'),
                         (b'controls=1', b'controls=0'), (b'physical=0x40600000', b'physical=0x40601000'),
                         (b'va=0x100002000', b'va=0x100002001'), (b'active=0x1f', b'active=0x11f')):
            with self.subTest(old=old), self.assertRaises(ValueError): h.validate_serial(raw.replace(old, new, 1), SYMBOLS)
        for bad in (raw.replace(b'0x40500000', b'0x40200000'), raw.replace(b'0x40600000', b'0x40500000'), raw.replace(b'0x40601000', b'0x40600000')):
            with self.assertRaises(ValueError): h.validate_serial(bad, SYMBOLS)

    def test_baseline_order_and_complete_exit(self):
        raw = serial()
        with self.assertRaises(ValueError): h.validate_serial(b'HMTEST BEGIN 41\n' + raw, SYMBOLS)
        traced = trace()
        for old, new in ((b'acknowledged irq 30', b'acknowledged irq 33'), (b'...from EL1 to EL1', b'...from EL0 to EL1'),
                         (b'0x96000006', b'0x96000007'), (b'call 0x20', b'call 0x18'), (b'CPU 0', b'CPU 1')):
            with self.subTest(old=old), self.assertRaises(ValueError): h.validate_trace(traced.replace(old, new, 1), SYMBOLS, FIXUPS)
        for line in traced.splitlines(keepends=True):
            if line.startswith((b'gic_', b'Exception return')):
                with self.assertRaises(ValueError): h.validate_trace(traced.replace(line, b'', 1), SYMBOLS, FIXUPS)
        for bad in (traced[:-1], traced + b'Exception return from AArch64 EL1 to AArch64 EL0 PC 0x100001000\n'):
            with self.assertRaises(ValueError): h.validate_trace(bad, SYMBOLS, FIXUPS)

    def test_exact_parse_only_fixture(self):
        raw = fixture()
        self.assertEqual(b.inspect(raw)['pages'], 6)
        for offset, fmt, value in ((24, '<Q', b.BASE), (54, '<H', 55), (56, '<H', 4), (68, '<I', 7),
                                   (64 + 56 + 16, '<Q', b.BASE), (64 + 32, '<Q', 4095), (4096, '<B', 0)):
            changed = bytearray(raw); struct.pack_into(fmt, changed, offset, value)
            with self.subTest(offset=offset), self.assertRaises(ValueError): b.inspect(changed)
        for size in (0, 63, 300, len(raw) - 1):
            with self.assertRaises(ValueError): b.inspect(raw[:size])


if __name__ == '__main__':
    unittest.main()
