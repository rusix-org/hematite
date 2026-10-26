#!/usr/bin/env python3
"""Negative checks for the bootstrap harness's fail-closed result contract."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import test_kernel as harness


def valid_report():
    lines = ["hematite bootstrap", "HMTEST BEGIN 9"]
    lines.extend(f"HMTEST PASS {case}" for case in harness.CASES)
    lines.append("HMTEST END 9")
    return ("\n".join(lines) + "\n").encode("ascii")


class ReportTests(unittest.TestCase):
    def assert_rejected(self, data, status=33, **kwargs):
        result = harness.validate_run(data, status, **kwargs)
        self.assertFalse(result["passed"])
        self.assertTrue(result["errors"])

    def test_complete_report_and_crlf(self):
        for data in (valid_report(), valid_report().replace(b"\n", b"\r\n")):
            with self.subTest(data=data):
                self.assertTrue(harness.validate_run(data, 33)["passed"])

    def test_boot_is_not_a_test_pass(self):
        self.assert_rejected(b"hematite bootstrap\n", 33)

    def test_every_case_is_required_and_cannot_be_skipped_or_failed(self):
        for case in harness.CASES:
            record = f"HMTEST PASS {case}\n".encode()
            for replacement in (b"", record.replace(b"PASS", b"FAIL"),
                                record.replace(b"PASS", b"SKIP"), record + record):
                with self.subTest(case=case, replacement=replacement):
                    self.assert_rejected(valid_report().replace(record, replacement))

    def test_summary_and_inventory_must_be_exact(self):
        for old, new in (
            (b"HMTEST BEGIN 9\n", b""),
            (b"HMTEST END 9\n", b""),
            (b"HMTEST BEGIN 9", b"HMTEST BEGIN 8"),
            (b"HMTEST END 9", b"HMTEST END 10"),
            (b"HMTEST PASS nx", b"HMTEST PASS imaginary"),
            (b"HMTEST PASS nx\nHMTEST PASS smep",
             b"HMTEST PASS smep\nHMTEST PASS nx"),
        ):
            with self.subTest(old=old, new=new):
                self.assert_rejected(valid_report().replace(old, new))

    def test_malformed_extra_and_truncated_records(self):
        for data in (
            valid_report()[:-1],
            valid_report() + b"HMTEST PASS extra\n",
            valid_report() + valid_report(),
            valid_report().replace(b"HMTEST PASS nx", b"prefix HMTEST PASS nx"),
            valid_report().replace(b"HMTEST PASS nx", b"HMTEST PASS nx \t"),
            valid_report().replace(b"HMTEST PASS nx", b"HMTEST PASS nx\xff"),
            valid_report().replace(b"HMTEST PASS nx\n", b"HMTEST PASS nx\x0b"),
            valid_report().replace(b"HMTEST PASS nx\n", b"HMTEST PASS nx\nPANIC\n"),
        ):
            with self.subTest(data=data):
                self.assert_rejected(data)

    def test_exit_and_timeout_are_independent_requirements(self):
        for status in (0, 1, 65, 124, -9, None):
            with self.subTest(status=status):
                self.assert_rejected(valid_report(), status)
        self.assert_rejected(valid_report(), timed_out=True)
        self.assert_rejected(valid_report(), launch_error="missing qemu")


class ExecutionTests(unittest.TestCase):
    def test_launch_failure_preserves_command_log_and_result(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with patch.object(harness.subprocess, "run", side_effect=FileNotFoundError("qemu")):
                result = harness.run_qemu(["missing-qemu"], output)
            self.assertFalse(result["passed"])
            self.assertEqual(json.loads((output / "command.json").read_text()), ["missing-qemu"])
            self.assertEqual((output / "serial.log").read_bytes(), b"")
            self.assertEqual(json.loads((output / "result.json").read_text()), result)

    def test_timeout_retains_partial_serial_and_fails(self):
        partial = b"HMTEST BEGIN 9\nHMTEST PASS write_protect\n"

        def timeout(command, **kwargs):
            kwargs["stdout"].write(partial)
            raise subprocess.TimeoutExpired(command, kwargs["timeout"])

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with patch.object(harness.subprocess, "run", side_effect=timeout):
                result = harness.run_qemu(["qemu"], output)
            self.assertFalse(result["passed"])
            self.assertTrue(result["timed_out"])
            self.assertEqual((output / "serial.log").read_bytes(), partial)


if __name__ == "__main__":
    unittest.main()
