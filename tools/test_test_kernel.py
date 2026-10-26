#!/usr/bin/env python3
"""Negative checks for the bootstrap harness's fail-closed result contract."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import test_kernel as harness
import test_boot_profiles as profiles


def valid_report():
    lines = ["hematite bootstrap", "HMTEST BEGIN 9"]
    lines.extend(f"HMTEST PASS {case}" for case in harness.CASES)
    lines.append("HMTEST END 9")
    return ("\n".join(lines) + "\n").encode("ascii")


class ReportTests(unittest.TestCase):
    def assert_rejected(self, data, status=0, **kwargs):
        result = harness.validate_run(data, status, **kwargs)
        self.assertFalse(result["passed"])
        self.assertTrue(result["errors"])

    def test_complete_report_and_crlf(self):
        for data in (valid_report(), valid_report().replace(b"\n", b"\r\n")):
            with self.subTest(data=data):
                self.assertTrue(harness.validate_run(data, 0)["passed"])
                self.assertTrue(harness.validate_run(data, 33, arch="x86_64")["passed"])

    def test_boot_is_not_a_test_pass(self):
        self.assert_rejected(b"hematite bootstrap\n", 0)
        self.assert_rejected(b"", 0)
        self.assert_rejected(b"", 33, arch="x86_64")

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
            (b"HMTEST PASS nx\nHMTEST PASS user_execute_protect",
             b"HMTEST PASS user_execute_protect\nHMTEST PASS nx"),
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
        for status in (1, 33, 65, 124, -9, None):
            with self.subTest(status=status):
                self.assert_rejected(valid_report(), status)
        self.assert_rejected(valid_report(), 0, arch="x86_64")
        self.assert_rejected(valid_report(), 33, arch="aarch64")
        self.assert_rejected(valid_report(), timed_out=True)
        self.assert_rejected(valid_report(), launch_error="missing qemu")


class ArchitectureTests(unittest.TestCase):
    def test_aarch64_is_default_and_selects_semihosting(self):
        command = harness.qemu_command(Path("kernel.elf"), "tcg")
        self.assertEqual(command[0], "qemu-system-aarch64")
        self.assertIn("virt,virtualization=off,secure=off", command)
        self.assertIn("enable=on,target=native", command)
        self.assertNotIn("isa-debug-exit,iobase=0xf4,iosize=0x04", command)

    def test_x86_remains_explicit_and_supports_kvm(self):
        command = harness.qemu_command(Path("kernel.elf"), "kvm", "x86_64")
        self.assertEqual(command[0], "qemu-system-x86_64")
        self.assertEqual(command[command.index("-cpu") + 1], "host")
        self.assertIn("isa-debug-exit,iobase=0xf4,iosize=0x04", command)

    def test_unavailable_architecture_or_accelerator_is_not_silently_changed(self):
        with self.assertRaises(ValueError):
            harness.qemu_command(Path("kernel.elf"), "kvm", "aarch64")
        with self.assertRaises(ValueError):
            harness.qemu_command(Path("kernel.elf"), "tcg", "unknown")
        with self.assertRaises(ValueError):
            harness.validate_run(valid_report(), 0, arch="unknown")

    def test_arm_cpu_rejection_requires_exact_guest_marker_and_exit(self):
        result = {"returncode": 1, "timed_out": False, "launch_error": None}
        self.assertTrue(all(profiles.rejection_checks(b"HMBOOT REJECT PAN\r\n", result).values()))
        for data in (b"", b"qemu: invalid CPU\n", b"prefix HMBOOT REJECT PAN\n",
                     b"HMBOOT REJECT PAN",
                     b"HMBOOT REJECT PAN\nHMBOOT REJECT PAN\n",
                     b"HMBOOT REJECT PAN\nHMTEST BEGIN 9\n"):
            with self.subTest(data=data):
                self.assertFalse(all(profiles.rejection_checks(data, result).values()))
        for update in ({"returncode": 0}, {"timed_out": True}, {"launch_error": "no qemu"}):
            with self.subTest(update=update):
                self.assertFalse(all(profiles.rejection_checks(
                    b"HMBOOT REJECT PAN\n", {**result, **update}).values()))
        self.assertTrue(all(profiles.rejection_checks(
            b"", {**result, "returncode": 35}, "x86_64").values()))


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
