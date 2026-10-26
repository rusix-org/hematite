#!/usr/bin/env python3
"""Failure injection for expected preemption hangs and fatal guard faults."""

import unittest

from test_preemption_negative import no_timer_checks, overflow_checks


class NegativeTests(unittest.TestCase):
    def setUp(self):
        self.result = {"returncode": 2, "timed_out": False, "launch_error": None}
        self.console = b"HMPREEMPT EXPECT OVERFLOW\nHMEXCEPTION FATAL\n"
        self.first_entry = b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40200000\n"
        self.trace = (b"Taking exception 5 [IRQ] on CPU 0\n"
                      b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40200000\n"
                      b"Taking exception 4 [Data Abort] on CPU 0\n"
                      b"...with ESR 0x25/0x96000007\n...with FAR 0x40300010\n"
                      b"Taking exception 16 [Semihosting call] on CPU 0\n"
                      b"...handling as semihosting call 0x20\n")

    def accepts(self, console=None, trace=None, result=None, guard=0x40300000):
        return all(overflow_checks(self.console if console is None else console,
                                   self.trace if trace is None else trace,
                                   self.result if result is None else result, guard).values())

    def test_overflow_with_prior_irq(self):
        self.assertTrue(self.accepts())

    def test_overflow_bad_process(self):
        for change in ({"returncode": 0}, {"timed_out": True}, {"launch_error": "missing"}):
            self.assertFalse(self.accepts(result=self.result | change))

    def test_overflow_markers(self):
        for console in (self.console[:-1], self.console * 2,
                        self.console.replace(b"OVERFLOW", b"NO-TIMER"),
                        self.console + b"HMPREEMPT END 4\n",
                        self.console + b"HMTEST BEGIN 32\n",
                        self.console.replace(b"FATAL", b"NESTED")):
            self.assertFalse(self.accepts(console=console))

    def test_overflow_guard_and_trace(self):
        for trace in (b"", self.trace[:-1], self.trace.replace(b"0x40300010", b"0x40400000"),
                      self.trace.replace(b"0x96000007", b"0x96000047"),
                      self.trace.replace(b"Taking exception 16", b"Exception return\nTaking exception 16"),
                      self.trace.replace(b"[IRQ]", b"[FIQ]"),
                      self.trace.replace(b"Taking exception 16", b"Taking exception 4 [Data Abort] on CPU 0\nTaking exception 16")):
            self.assertFalse(self.accepts(trace=trace))
        self.assertFalse(self.accepts(guard=None))

    def test_no_timer_success(self):
        result = {"returncode": -9, "timed_out": True, "launch_error": None}
        self.assertTrue(all(no_timer_checks(b"HMPREEMPT EXPECT NO-TIMER\n", self.first_entry, result, 0x40200000).values()))
        for symbol in (None, 0x40200004):
            self.assertFalse(all(no_timer_checks(b"HMPREEMPT EXPECT NO-TIMER\n", self.first_entry, result, symbol).values()))

    def test_no_timer_requires_exactly_one_first_entry(self):
        result = {"returncode": -9, "timed_out": True, "launch_error": None}
        for trace in (b"", self.first_entry[:-1], self.first_entry * 2,
                      self.first_entry.replace(b"PC 0x40200000", b"PC invalid"),
                      self.first_entry.replace(b"to AArch64 EL1", b"to AArch64 EL0"),
                      self.first_entry + b"Taking exception 5 [IRQ] on CPU 0\n"):
            self.assertFalse(all(no_timer_checks(b"HMPREEMPT EXPECT NO-TIMER\n", trace, result, 0x40200000).values()))

    def test_no_timer_rejects_early_exit_or_unlaunched(self):
        result = {"returncode": -9, "timed_out": True, "launch_error": None}
        for change in ({"returncode": 0}, {"timed_out": False}, {"launch_error": "missing"}):
            self.assertFalse(all(no_timer_checks(b"HMPREEMPT EXPECT NO-TIMER\n", self.first_entry, result | change, 0x40200000).values()))

    def test_no_timer_missing_wrong_or_completed_marker(self):
        result = {"returncode": -9, "timed_out": True, "launch_error": None}
        marker = b"HMPREEMPT EXPECT NO-TIMER\n"
        for console in (b"", marker[:-1], marker * 2, self.console,
                        marker + b"HMPREEMPT PASS preempt_progress\n", marker + b"HMEXCEPTION FATAL\n"):
            self.assertFalse(all(no_timer_checks(console, self.first_entry, result, 0x40200000).values()))
        self.assertFalse(all(no_timer_checks(marker, self.trace, result, 0x40200000).values()))


if __name__ == "__main__":
    unittest.main()
