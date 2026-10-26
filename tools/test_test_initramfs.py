#!/usr/bin/env python3
"""Failure-sensitive host inventory and GNU cpio evidence parser tests."""
import unittest
import build_initramfs as builder
import check_initramfs as harness

EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}
ARTIFACTS = {'stdout': {'within_limit': True}, 'stderr': {'within_limit': True}}


def report():
    lines = ['running 16 tests'] + [f'test {case} ... ok' for case in harness.CASES]
    lines += ['test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s']
    return ('\n' + '\n'.join(lines) + '\n\n').encode()


class InitramfsHarnessTests(unittest.TestCase):
    def test_exact_host_inventory(self):
        self.assertTrue(harness.validate_execution(report(), EXECUTION, ARTIFACTS)['passed'])
        self.assertEqual(len(harness.CASES), 16)
        self.assertIn('gnu_reference_payload_and_elf', harness.CASES)

    def test_every_test_and_delimiter_is_mandatory_once(self):
        raw = report()
        for line in raw.splitlines(keepends=True):
            if line.startswith((b'test ', b'running ')):
                with self.subTest(line=line):
                    self.assertFalse(harness.validate(raw.replace(line, b'', 1))['passed'])
                    self.assertFalse(harness.validate(raw.replace(line, line + line, 1))['passed'])

    def test_skip_filter_wrong_summary_and_wrong_test_module(self):
        raw = report()
        for old, new in ((b'... ok', b'... ignored'), (b'... ok', b'... FAILED'),
                (b'16 passed', b'15 passed'), (b'0 ignored', b'1 ignored'),
                (b'0 filtered out', b'1 filtered out'), (b'0 failed', b'1 failed'),
                (b'0 measured', b'1 measured'), (b'running 16 tests', b'running 0 tests'),
                (b'test alignment_and_hex_fields', b'test native::tests::alignment_and_hex_fields')):
            self.assertFalse(harness.validate(raw.replace(old, new, 1))['passed'])
        self.assertFalse(harness.validate(raw + b'unknown test output\n')['passed'])

    def test_order_truncation_encoding_and_size(self):
        raw = report()
        first, second = [f'test {name} ... ok\n'.encode() for name in harness.CASES[:2]]
        for bad in (b'', raw[:-2], raw.replace(first + second, second + first), raw + b'\xff\n',
                    raw + b'x' * (harness.MAX_LOG_BYTES + 1)):
            self.assertFalse(harness.validate(bad)['passed'])

    def test_perfect_output_does_not_hide_execution_failure(self):
        for changes in ({'returncode': 1}, {'returncode': None}, {'returncode': -9},
                        {'timed_out': True}, {'launch_error': 'missing cargo'}):
            self.assertFalse(harness.validate_execution(report(), {**EXECUTION, **changes}, ARTIFACTS)['passed'])
        for artifacts in ({}, {'stdout': {'within_limit': True}},
                {**ARTIFACTS, 'stdout': {'within_limit': False}}, {**ARTIFACTS, 'stderr': {}}):
            self.assertFalse(harness.validate_execution(report(), EXECUTION, artifacts)['passed'])

    def test_deterministic_archive_layout_and_source_payload(self):
        elf = b'\x7fELF' + bytes(range(60))
        entries = builder.payloads(elf)
        data = builder.newc(entries)
        self.assertEqual(data, builder.newc(entries))
        self.assertEqual(data[:6], b'070701')
        self.assertEqual(len(data) % 512, 0)
        self.assertEqual([path for path, _, _ in entries], list(builder.ORDER))
        # Independently walk the fixed newc wire fields, including payload boundaries.
        at, paths, payloads = 0, [], []
        while True:
            self.assertEqual(data[at:at + 6], b'070701')
            fields = [int(data[at + 6 + 8 * i:at + 14 + 8 * i], 16) for i in range(13)]
            name_start = at + 110
            name_end = name_start + fields[11]
            name = data[name_start:name_end - 1].decode('ascii')
            self.assertEqual(data[name_end - 1], 0)
            payload_start = (name_end + 3) & ~3
            self.assertFalse(any(data[name_end:payload_start]))
            payload_end = payload_start + fields[6]
            after = (payload_end + 3) & ~3
            self.assertFalse(any(data[payload_end:after]))
            if name == 'TRAILER!!!':
                self.assertEqual(fields, [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 11, 0])
                self.assertLessEqual(len(data) - after, 511)
                self.assertFalse(any(data[after:]))
                break
            paths.append(name)
            payloads.append(data[payload_start:payload_end])
            at = after
        self.assertEqual(paths, list(builder.ORDER))
        self.assertEqual(payloads, [b'', elf, b'', builder.BANNER])
        manifest = builder.manifest(data, entries)
        self.assertEqual(manifest['entries'][1]['sha256'], builder.sha(elf))
        self.assertEqual(manifest['sha256'], builder.sha(data))

    def test_gnu_listing_requires_exact_names_order_and_completion(self):
        entries = builder.payloads(b'\x7fELF' + b'x' * 60)
        raw = b'bin\nbin/hello\netc\netc/banner\n'
        self.assertTrue(builder.validate_listing(raw, entries))
        for bad in (raw[:-1], raw.replace(b'bin/hello\n', b''), raw + b'etc/banner\n',
                    raw.replace(b'bin/hello', b'./bin/hello'), raw.replace(b'etc\n', b'etc\nextra\n'),
                    raw.replace(b'bin\nbin/hello', b'bin/hello\nbin'), raw + b'\xff\n'):
            self.assertFalse(builder.validate_listing(bad, entries))

    def test_gnu_verbose_listing_checks_types_modes_links_owners_and_sizes(self):
        entries = builder.payloads(b'\x7fELF' + b'x' * 60)
        raw = (b'drwxr-xr-x 2 0 0 0 Jan  1  1970 bin\n'
               b'-rwxr-xr-x 1 0 0 64 Jan  1  1970 bin/hello\n'
               b'drwxr-xr-x 2 0 0 0 Jan  1  1970 etc\n' +
               f'-rw-r--r-- 1 0 0 {len(builder.BANNER)} Jan  1  1970 etc/banner\n'.encode())
        self.assertTrue(builder.validate_listing(raw, entries, True))
        self.assertTrue(builder.validate_listing(raw.replace(b'drwxr-xr-x 2', b'drwxr-xr-x 1'), entries, True))
        for old, new in ((b'drwxr-xr-x', b'lrwxr-xr-x'), (b'-rwxr-xr-x', b'-rw-r--r--'),
                (b'drwxr-xr-x 2', b'drwxr-xr-x 0'), (b'-rwxr-xr-x 1', b'-rwxr-xr-x 2'),
                (b'1 0 0 64', b'1 1 0 64'), (b'1 0 0 64', b'1 0 1 64'),
                (b'1 0 0 64', b'1 0 0 63'), (b'Jan  1  1970', b'Jan  1  2026')):
            self.assertFalse(builder.validate_listing(raw.replace(old, new, 1), entries, True))


if __name__ == '__main__':
    unittest.main()
