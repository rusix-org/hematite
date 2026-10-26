#!/usr/bin/env python3
"""Failure-injection checks for the strict scheduling report validator."""
import unittest

from test_preemption import CASES, validate, validate_trace
from test_kernel import CASES as BOOT_CASES


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.lines = [f"HMPREEMPT BEGIN {len(CASES)}"]
        self.lines += [f"HMPREEMPT PASS {name}" for name in CASES]
        self.lines += [f"HMPREEMPT END {len(CASES)}"]
        self.post = [f"HMTEST BEGIN {len(BOOT_CASES)}"]
        self.post += [f"HMTEST PASS {name}" for name in BOOT_CASES]
        self.post += [f"HMTEST END {len(BOOT_CASES)}"]

    def report(self, lines=None):
        return ("\n".join(self.lines if lines is None else lines)
                + "\nHMPREEMPT IRQ 0000000000000001\n"
                + "\n".join(self.post) + "\n").encode()

    def test_success(self):
        self.assertTrue(validate(self.report(), 0)["passed"])

    def test_missing_duplicate_reorder_failure(self):
        for lines in (self.lines[:2] + self.lines[3:],
                      self.lines[:2] + self.lines[1:],
                      self.lines[:1] + list(reversed(self.lines[1:-1])) + self.lines[-1:],
                      [line.replace("PASS", "FAIL") for line in self.lines]):
            with self.subTest(lines=lines):
                self.assertFalse(validate(self.report(lines), 0)["passed"])

    def test_process_failure(self):
        for args in ((1, False, None), (0, True, None), (0, False, "missing")):
            self.assertFalse(validate(self.report(), *args)["passed"])

    def test_bad_framing(self):
        for report in (self.report()[:-1], self.report().replace(b"PASS", b"PASS\xff", 1),
                       self.report().replace(b"\n", b"\nnoise\n", 1),
                       b"HMTEST BEGIN 32\n" + self.report(),
                       self.report() + b"HMEXCEPTION FATAL\n"):
            self.assertFalse(validate(report, 0)["passed"])

    def test_count(self):
        for report in (self.report().replace(b"0000000000000001", b"0000000000000000"),
                       self.report() + b"HMPREEMPT IRQ 0000000000000001\n",
                       self.report().split(b"HMPREEMPT IRQ")[0]):
            self.assertFalse(validate(report, 0)["passed"])

    def test_post_regression_required_and_ordered(self):
        for report in (self.report().split(b"HMTEST")[0],
                       self.report().replace(b"HMTEST PASS nx\n", b""),
                       self.report().replace(b"HMTEST PASS nx", b"HMTEST FAIL nx"),
                       self.report().replace(b"HMTEST PASS nx\n", b"HMTEST PASS nx\nHMTEST PASS nx\n"),
                       ("\n".join(self.post) + "\n").encode() + self.report().split(b"HMTEST")[0]):
            self.assertFalse(validate(report, 0)["passed"])

    def test_trace_transactions(self):
        trace = (b"Taking exception 5 [IRQ] on CPU 0\n...from EL1 to EL1\n"
                 b"...to EL1 PC 0x40200280 PSTATE 0x3c5\n"
                 b"gic_acknowledge_irq cpu 0 acknowledged irq 30\n"
                 b"gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n")
        self.assertTrue(validate_trace(trace, 1)["passed"])
        for damaged in (trace.replace(b"irq 30", b"irq 29"),
                        trace.replace(b"0x0000001e", b"0x0000001f"),
                        trace + trace, trace[:-1],
                        trace + b"Exception return malformed\n",
                        trace + b"Exception return from AArch64 EL1 to AArch64 EL0 PC 0x40200000\n",
                        trace.replace(b"...from EL1 to EL1\n", b""),
                        trace.replace(b"gic_acknowledge_irq", b"Exception return\ngic_acknowledge_irq")):
            self.assertFalse(validate_trace(damaged, 1)["passed"])

    def test_post_fault_transactions(self):
        irq = (b"Taking exception 5 [IRQ] on CPU 0\n...from EL1 to EL1\n"
               b"...to EL1 PC 0x40200280 PSTATE 0x3c5\n"
               b"gic_acknowledge_irq cpu 0 acknowledged irq 30\n"
               b"gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n")
        fault = (b"Taking exception 4 [Data Abort] on CPU 0\n...from EL1 to EL1\n"
                 b"...with ESR 0x25/0x96000006\n...with FAR 0x0\n...with ELR 0x4020139c\n"
                 b"...to EL1 PC 0x40200200 PSTATE 0x4003c5\n"
                 b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x402013a4\n")
        self.assertTrue(validate_trace(irq + fault, 1)["passed"])
        for damaged in (fault.replace(b"...from EL1 to EL1\n", b""),
                        fault.replace(b"0x40200200", b"0x40200280"),
                        fault.replace(b"0x4003c5", b"0x400345"),
                        fault.replace(b"...with FAR 0x0\n", b""),
                        fault.replace(b"Exception return", b"Exception return broken"),
                        fault.split(b"Exception return")[0],
                        fault.replace(b"...with FAR 0x0\n", irq + b"...with FAR 0x0\n")):
            self.assertFalse(validate_trace(irq + damaged, 1)["passed"])


if __name__ == "__main__":
    unittest.main()
