#!/usr/bin/env python3
"""Failure-sensitive tests for the exact native-policy host test gate."""
import unittest
import check_native as harness

EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}
ARTIFACTS = {'stdout': {'within_limit': True}, 'stderr': {'within_limit': True}}


def report():
    lines = ['running 13 tests']
    lines += [f'test {name} ... ok' for name in harness.EXPECTED_CASES]
    lines += ['test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s']
    return ('\n' + '\n'.join(lines) + '\n\n').encode()


class NativeHarnessTests(unittest.TestCase):
    def test_complete_success(self):
        self.assertTrue(harness.validate_execution(report(), EXECUTION, ARTIFACTS)['passed'])

    def test_every_inventory_record_required_once(self):
        raw = report()
        for record in raw.splitlines(keepends=True):
            if record.startswith((b'test ', b'running ')):
                with self.subTest(record=record):
                    self.assertFalse(harness.validate(raw.replace(record, b'', 1))['passed'])
                    self.assertFalse(harness.validate(raw.replace(record, record + record, 1))['passed'])

    def test_wrong_order_or_test_name(self):
        raw = report()
        first, second = [f'test {name} ... ok\n'.encode() for name in harness.EXPECTED_CASES[:2]]
        self.assertFalse(harness.validate(raw.replace(first + second, second + first))['passed'])
        self.assertFalse(harness.validate(raw.replace(first, b'test native::tests::unknown ... ok\n'))['passed'])
        self.assertFalse(harness.validate(raw + b'unknown extra output\n')['passed'])

    def test_skips_filters_failures_and_incomplete_summary(self):
        raw = report()
        for before, after in ((b'... ok', b'... ignored'), (b'... ok', b'... FAILED'),
                (b'0 ignored', b'1 ignored'), (b'0 filtered out', b'1 filtered out'),
                (b'0 failed', b'1 failed'), (b'13 passed', b'12 passed'),
                (b'0 measured', b'1 measured'), (b'test result: ok', b'test result: FAILED'),
                (b'running 13 tests', b'running 0 tests')):
            with self.subTest(after=after):
                self.assertFalse(harness.validate(raw.replace(before, after, 1))['passed'])
        self.assertFalse(harness.validate(b'running 0 tests\n')['passed'])

    def test_truncation_encoding_and_extra_content(self):
        raw = report()
        for bad in (b'', raw[:-2], raw + b'\xff\n', raw.replace(b'... ok', b'... ok\r'),
                    raw.replace(b'finished in 0.00s', b'finished in unknown'),
                    raw + b'x' * (harness.MAX_LOG_BYTES + 1)):
            self.assertFalse(harness.validate(bad)['passed'])

    def test_success_text_cannot_hide_execution_failure(self):
        for changes in ({'returncode': 1}, {'returncode': None}, {'returncode': -9},
                        {'timed_out': True}, {'launch_error': 'missing cargo'}):
            with self.subTest(changes=changes):
                self.assertFalse(harness.validate_execution(report(), {**EXECUTION, **changes}, ARTIFACTS)['passed'])
        self.assertFalse(harness.validate_execution(report(), {}, ARTIFACTS)['passed'])

    def test_missing_or_oversized_artifacts_fail(self):
        for artifacts in ({}, {'stdout': {'within_limit': True}},
                {**ARTIFACTS, 'stderr': {'within_limit': False}},
                {**ARTIFACTS, 'stdout': {}}, {**ARTIFACTS, 'extra': {'within_limit': True}}):
            self.assertFalse(harness.validate_execution(report(), EXECUTION, artifacts)['passed'])


if __name__ == '__main__':
    unittest.main()
