#!/usr/bin/env python3
"""Mutation tests for strict owned-space guest evidence parsing."""
import unittest
import test_space as h
from test_kernel import CASES

SYMBOLS = {'arm_root': 0x40010000, '__text_start': 0x40080000, '__bss_start': 0x40100000}
EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}
TRACE = b'Taking exception 11 [Hypervisor Call] on CPU 0\nTaking exception 16 [Semihosting call] on CPU 0\n...handling as semihosting call 0x20\n'


def console(cpus=1):
    lines = ['HMSPACE BEGIN 5', f'HMSPACE INFO cpus={cpus} peer_off=1 image_pages=5 required=33',
             f'HMSPACE ARENA va=2147483648 pa={0x40200000}']
    maps = [(va, 0x40300000 + n * 4096, flag) for n, (va, flag) in enumerate(zip(h.PAGE_VAS, h.FLAGS))]
    queries = []
    for va, pa, flag in maps:
        lines.append(f'HMSPACE MAP va={va} pa={pa} flags={flag}')
        queries += [(va + 17, 0, 11, pa), (va + 17, 1, 11, pa if flag == 6 else 31),
                    (pa + 17, 3, pa, 31 if flag == 5 else pa)]
    queries += [(h.BASE + n, 0, 11, 15) for n in (0x1ef000, 0x100000, 0x10000)]
    queries += [(va, 0, 31, 31) for va in (SYMBOLS['__text_start'], SYMBOLS['__bss_start'], 0x9000000, 0x8000000)]
    queries.append((0x80000000, 2, 0x40200000, 11))
    for iteration in range(2):
        for index, (va, access, boot, active) in enumerate(queries):
            if index % 16 == 0:
                lines.append(f'HMSPACE ROOT pass={iteration} batch={index // 16} active={0x40400000} restored={SYMBOLS["arm_root"]} controls=1')
            lines.append(f'HMSPACE PROBE pass={iteration} index={index} va={va} access={access} before={boot} active={active} after={boot}')
    for name, during, after, frames, attempts in [('fixture', 967, 1000, 33, 1), ('budgets', 1000, 1000, 33, 33),
            ('pressure', 8, 1000, 33, 1), ('geometry', 716, 1000, 284, 2), ('forget', 967, 967, 33, 1)]:
        lines += [f'HMSPACE METRIC case={name} before=1000 during={during} after={after} frames={frames} attempts={attempts}', 'HMSPACE PASS ' + name]
    lines += ['HMSPACE END 5', f'HMTEST BEGIN {len(CASES)}'] + ['HMTEST PASS ' + case for case in CASES] + [f'HMTEST END {len(CASES)}']
    return ('\n'.join(lines) + '\n').encode()


class SpaceTests(unittest.TestCase):
    def reject(self, raw=None, trace=TRACE, execution=EXECUTION, symbols=SYMBOLS, cpus=1):
        self.assertFalse(h.validate(console() if raw is None else raw, trace, execution, symbols, cpus)['passed'])

    def test_complete_profiles(self):
        for cpus in (1, 2):
            result = h.validate(console(cpus), TRACE, EXECUTION, SYMBOLS, cpus)
            self.assertTrue(result['passed'], result['errors'])

    def test_inventory_mutations(self):
        raw = console()
        for record in raw.splitlines(keepends=True):
            if record.startswith(b'HMSPACE'):
                with self.subTest(record=record[:100]):
                    self.reject(raw.replace(record, b'', 1))
                    self.reject(raw.replace(record, record + record, 1))

    def test_record_value_mutations(self):
        raw = console()
        for before, after in [(b'peer_off=1', b'peer_off=0'), (b'image_pages=5', b'image_pages=4'),
            (b'flags=5', b'flags=6'), (b'controls=1', b'controls=0'), (b'access=0', b'access=2'),
            (b'active=31', b'active=15'), (b'before=11', b'before=31'), (b'after=11', b'after=31'),
            (b'during=967', b'during=966'), (b'after=967', b'after=1000'),
            (b'attempts=33', b'attempts=32'), (b'frames=284', b'frames=283'),
            (b'during=8', b'during=9'), (b'index=70', b'index=71')]:
            with self.subTest(after=after):
                self.assertIn(before, raw)
                self.reject(raw.replace(before, after, 1))

    def test_bad_physical_addresses(self):
        raw = console()
        for address in (0, 0x3ffff000, 0x41000000, 0x40300001, 0x40200000):
            self.reject(raw.replace(f'pa={0x40300000}'.encode(), f'pa={address}'.encode(), 1))
        for root in (0, 0x40400001, 0x40300000, 0x40200000, SYMBOLS['arm_root']):
            self.reject(raw.replace(f'active={0x40400000}'.encode(), f'active={root}'.encode(), 1))
        self.reject(raw.replace(f'active={0x40300000}'.encode(), f'active={0x40301000}'.encode(), 1))
        self.reject(symbols={})
        self.reject(symbols={**SYMBOLS, 'arm_root': 0})

    def test_malformed_truncated_nonascii_and_foreign(self):
        raw = console()
        for bad in (raw.replace(b'HMSPACE INFO', b'junk HMSPACE INFO'),
                    raw.replace(b'cpus=1', b'cpus=01'), raw.replace(b'cpus=1', b'cpus=' + b'9' * 5000),
                    raw.replace(b'HMSPACE INFO', b'\xffHMSPACE INFO'),
                    raw.replace(b'HMSPACE END 5\n', b'HMSPACE END 5'),
                    raw.replace(b'HMSPACE INFO', b'\nHMSPACE INFO'),
                    raw.replace(b'HMSPACE INFO', b'junk\nHMSPACE INFO'), raw + b'HMSPACE END 5',
                    raw + b'HMEXCEPTION FATAL\n', raw + b'HMSMP PASS\n'):
            self.reject(bad)

    def test_baseline_required_and_ordered(self):
        raw = console()
        split = raw.index(b'HMTEST BEGIN')
        self.reject(raw[:split])
        self.reject(raw[split:] + raw[:split])
        self.reject(raw.replace(('HMTEST PASS ' + CASES[0]).encode(), b'HMTEST FAIL bad'))

    def test_execution_failure(self):
        for changes in ({'returncode': 1}, {'returncode': None}, {'timed_out': True}, {'launch_error': 'QEMU missing'}):
            self.reject(execution={**EXECUTION, **changes})
        self.reject(cpus=2)
        self.reject(cpus=3)

    def test_exception_trace_failure(self):
        for trace in (b'', TRACE[:-1], TRACE + TRACE, TRACE + b'junk\n', TRACE.replace(b'CPU 0', b'CPU 1'),
                      b'Taking exception 1 [Undefined Instruction] on CPU 0\n' + TRACE,
                      b'Taking exception 5 [IRQ] on CPU 1\n' + TRACE):
            self.reject(trace=trace)
        self.reject(console(2), trace=TRACE[TRACE.index(b'Taking exception 16'):], cpus=2)

    def test_par_fault_decode_and_offset(self):
        self.assertTrue(h.par_matches(0x40300000, 0x40300011))
        self.assertFalse(h.par_matches(0x40301000, 0x40300011))
        self.assertFalse(h.par_matches(0x40300001, 0x40300011))
        self.assertTrue(h.par_matches(31, fault=15))
        self.assertFalse(h.par_matches(30, fault=15))
        self.assertFalse(h.par_matches(15, fault=15))
        for stage2 in (0x100, 0x200, 0x300):
            self.assertFalse(h.par_matches(31 | stage2, fault=15))
            self.reject(console().replace(b'active=31', f'active={31 | stage2}'.encode(), 1))


if __name__ == '__main__':
    unittest.main()
