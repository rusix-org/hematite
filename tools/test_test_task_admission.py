#!/usr/bin/env python3
"""Failure-sensitive task-admission host inventory and execution evidence tests."""
import unittest
import check_task_admission as harness

EXECUTION = {'returncode': 0, 'timed_out': False, 'launch_error': None}
ARTIFACTS = {'stdout': {'within_limit': True}, 'stderr': {'within_limit': True}}


def report():
    lines = ['running 16 tests'] + [f'test {case} ... ok' for case in harness.CASES]
    lines += ['test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s']
    return ('\n' + '\n'.join(lines) + '\n\n').encode()


class TaskAdmissionHarnessTests(unittest.TestCase):
    def test_exact_host_inventory(self):
        self.assertTrue(harness.validate_execution(report(), EXECUTION, ARTIFACTS)['passed'])
        self.assertEqual(len(harness.CASES), 16)
        self.assertIn('all_task_operations', harness.CASES)

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
                (b'test all_task_operations', b'test native::tests::all_task_operations')):
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



if __name__ == "__main__":
    unittest.main()
