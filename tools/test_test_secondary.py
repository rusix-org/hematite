#!/usr/bin/env python3
"""Failure-sensitive tests for the secondary lifecycle host parser."""

import unittest

import test_secondary as harness
from test_kernel import CASES


SYMBOLS = {"arm_secondary_root": 0x40210000,
           "__secondary_stack_guard": 0x40300000,
           "__secondary_stack_top": 0x40311000}
EXECUTION = {"returncode": 0, "timed_out": False, "launch_error": None}
INFO = b"HMSMP INFO version=65537 cpu_on=0 cpu_off=0 affinity_info=0\n"
EXIT_TRACE = (b"Taking exception 16 [Semihosting call] on CPU 0\n"
              b"...handling as semihosting call 0x20\n")
CPU0_HVC = b"Taking exception 11 [Hypervisor Call] on CPU 0\n"
CPU1_HVC = b"Taking exception 11 [Hypervisor Call] on CPU 1\n"
TRACE = CPU0_HVC + CPU1_HVC * 2 + EXIT_TRACE


def console():
    records = [b"hematite bootstrap\n", b"HMSMP BEGIN 2\n", INFO]
    for generation in (1, 2):
        records.append((f"HMSMP SESSION generation={generation} cpu=1 on=0 returned=1 off=1 "
                        f"checks=255 root={SYMBOLS['arm_secondary_root']} "
                        f"sp={SYMBOLS['__secondary_stack_top'] - 128}\n").encode())
    records += [b"HMSMP PASS lifecycle\n", b"HMSMP END 2\n",
                f"HMTEST BEGIN {len(CASES)}\n".encode()]
    records += [f"HMTEST PASS {case}\n".encode() for case in CASES]
    records.append(f"HMTEST END {len(CASES)}\n".encode())
    return b"".join(records)


def negative(name):
    marker, code = harness.NEGATIVES[name]
    raw = b"HMSMP BEGIN 2\n" + INFO + f"HMSMP EXPECT {marker}\n".encode()
    trace = CPU0_HVC + EXIT_TRACE.replace(b"on CPU 0", b"on CPU 1")
    result = {**EXECUTION, "returncode": code}
    return raw, trace, result


class LifecycleTests(unittest.TestCase):
    def test_complete_positive(self):
        self.assertTrue(harness.validate(console(), TRACE, EXECUTION, SYMBOLS)["passed"])

    def test_missing_duplicate_and_out_of_order_records(self):
        raw = console()
        for bad in (raw.replace(INFO, b""), raw.replace(INFO, INFO + INFO),
                    raw.replace(b"generation=2", b"generation=1"),
                    raw.replace(b"generation=1", b"generation=3"),
                    raw.replace(b"HMSMP PASS lifecycle\n", b""),
                    raw.replace(b"HMSMP END 2\n", b"HMSMP END 1\n"),
                    raw + b"HMSMP PASS lifecycle\n"):
            with self.subTest(bad=bad[:150]):
                self.assertFalse(harness.validate(bad, TRACE, EXECUTION, SYMBOLS)["passed"])

    def test_malformed_or_partial_report(self):
        raw = console()
        for bad in (raw.replace(INFO, INFO[:-1]), raw.replace(INFO, b"\xff" + INFO),
                    raw.replace(INFO, INFO + b"\n"), raw.replace(INFO, INFO + b"junk\n"),
                    raw + b"HMSMP END 2", raw.replace(b"generation=1", b"generation=01"),
                    raw.replace(b"version=65537", b"version=" + b"9" * 5000)):
            with self.subTest(bad=bad[:130]):
                self.assertFalse(harness.validate(bad, TRACE, EXECUTION, SYMBOLS)["passed"])

    def test_status_version_identity_and_mask(self):
        raw = console()
        for before, after in ((b"version=65537", b"version=2"),
                              (b"version=65537", b"version=4294967295"),
                              (b"version=65537", b"version=131072"),
                              (b"cpu_on=0", b"cpu_on=-1"), (b"on=0 returned", b"on=-4 returned"),
                              (b"cpu=1", b"cpu=0"), (b"off=1 checks", b"off=0 checks"),
                              (b"returned=1", b"returned=0"), (b"checks=255", b"checks=127")):
            with self.subTest(after=after):
                self.assertFalse(harness.validate(raw.replace(before, after), TRACE, EXECUTION, SYMBOLS)["passed"])

    def test_root_and_stack_bounds_match_elf(self):
        raw = console()
        root = str(SYMBOLS["arm_secondary_root"]).encode()
        sp = str(SYMBOLS["__secondary_stack_top"] - 128).encode()
        for bad in (raw.replace(b"root=" + root, b"root=0"),
                    raw.replace(b"sp=" + sp, f"sp={SYMBOLS['__secondary_stack_guard']}".encode()),
                    raw.replace(b"sp=" + sp, f"sp={SYMBOLS['__secondary_stack_top']}".encode()),
                    raw.replace(b"sp=" + sp, f"sp={SYMBOLS['__secondary_stack_top'] - 1}".encode())):
            self.assertFalse(harness.validate(bad, TRACE, EXECUTION, SYMBOLS)["passed"])
        for symbols in ({}, {**SYMBOLS, "arm_secondary_root": None},
                        {**SYMBOLS, "__secondary_stack_top": SYMBOLS["__secondary_stack_top"] + 4096}):
            self.assertFalse(harness.validate(raw, TRACE, EXECUTION, symbols)["passed"])

    def test_full_baseline_required_after_join(self):
        raw = console()
        baseline = raw[raw.index(b"HMTEST BEGIN"):]
        report = raw[:raw.index(b"HMTEST BEGIN")]
        for bad in (report, baseline + report, raw.replace(b"HMTEST PASS wx\n", b""),
                    raw.replace(b"HMTEST PASS wx", b"HMTEST FAIL wx")):
            self.assertFalse(harness.validate(bad, TRACE, EXECUTION, SYMBOLS)["passed"])

    def test_launch_exit_and_timeout_fail_closed(self):
        for change in ({"returncode": 1}, {"returncode": None}, {"timed_out": True},
                       {"launch_error": "missing QEMU"}):
            self.assertFalse(harness.validate(console(), TRACE, {**EXECUTION, **change}, SYMBOLS)["passed"])

    def test_trace_must_be_complete_with_correct_terminal_cpu(self):
        for trace in (b"", TRACE[:-1], TRACE.replace(b"CPU 0", b"CPU 1"), TRACE + b"extra\n",
                      TRACE + TRACE, b"Taking exception 5 [IRQ] on CPU 1\n" + TRACE,
                      b"Taking exception 1 [Undefined Instruction] on CPU 1\n" + TRACE,
                      b"Taking exception 11 [Hypervisor Call] on CPU 2\n" + TRACE):
            self.assertFalse(harness.validate(console(), trace, EXECUTION, SYMBOLS)["passed"])

    def test_hvc_evidence_matches_lifecycle(self):
        for trace in (TRACE.replace(CPU0_HVC, b""), CPU0_HVC + CPU1_HVC + EXIT_TRACE,
                      CPU0_HVC + CPU1_HVC * 3 + EXIT_TRACE):
            self.assertFalse(harness.validate(console(), trace, EXECUTION, SYMBOLS)["passed"])
        for name in harness.NEGATIVES:
            raw, trace, result = negative(name)
            for bad in (trace.replace(CPU0_HVC, b""), CPU1_HVC + trace):
                self.assertFalse(harness.validate(raw, bad, result, SYMBOLS, name)["passed"])

    def test_both_negative_profiles(self):
        for name in harness.NEGATIVES:
            raw, trace, execution = negative(name)
            self.assertTrue(harness.validate(raw, trace, execution, SYMBOLS, name)["passed"])

    def test_single_cpu_topology_rejection(self):
        raw = b"hematite bootstrap\nHMSMP REJECT TOPOLOGY\n"
        result = {**EXECUTION, "returncode": 1}
        self.assertTrue(harness.validate(raw, EXIT_TRACE, result, SYMBOLS, "topology")["passed"])
        for bad in (b"", raw[:-1], raw + INFO, raw + b"HMSMP PASS lifecycle\n",
                    raw + b"HMTEST BEGIN 39\n", raw.replace(b"REJECT", b"EXPECT")):
            self.assertFalse(harness.validate(bad, EXIT_TRACE, result, SYMBOLS, "topology")["passed"])
        for change in ({"returncode": 0}, {"returncode": 2}, {"timed_out": True},
                       {"launch_error": "missing QEMU"}):
            self.assertFalse(harness.validate(raw, EXIT_TRACE, {**result, **change}, SYMBOLS, "topology")["passed"])
        for trace in (EXIT_TRACE.replace(b"CPU 0", b"CPU 1"),
                      b"Taking exception 11 [Hypervisor Call] on CPU 1\n" + EXIT_TRACE,
                      b"Taking exception 11 [Hypervisor Call] on CPU 0\n" + EXIT_TRACE):
            self.assertFalse(harness.validate(raw, trace, result, SYMBOLS, "topology")["passed"])

    def test_negative_cannot_pass_for_wrong_failure_or_normal_output(self):
        for name in harness.NEGATIVES:
            raw, trace, execution = negative(name)
            for bad in (raw.replace(b"HMSMP EXPECT", b"HMSMP FAIL"), raw + b"HMSMP PASS lifecycle\n",
                        raw + b"HMTEST BEGIN 39\n", raw + b"HMEXCEPTION FATAL\n",
                        raw[:-1], raw.replace(INFO, b"")):
                self.assertFalse(harness.validate(bad, trace, execution, SYMBOLS, name)["passed"])
            for change in ({"returncode": 0}, {"returncode": 2}, {"returncode": -9},
                           {"timed_out": True}, {"launch_error": "bad launch"}):
                self.assertFalse(harness.validate(raw, trace, {**execution, **change}, SYMBOLS, name)["passed"])
            for bad_trace in (b"", trace.replace(b"CPU 1", b"CPU 0"), trace[:-1]):
                self.assertFalse(harness.validate(raw, bad_trace, execution, SYMBOLS, name)["passed"])


if __name__ == "__main__":
    unittest.main()
