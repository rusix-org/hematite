#!/usr/bin/env python3
"""Mutation tests for strict timer trace validation, independent of the guest."""

import unittest

from check_timer_trace import validate_trace


ENTRY = b"Taking exception 5 [IRQ] on CPU 0\n"
FROM = b"...from EL1 to EL1\n"
VECTOR = b"...to EL1 PC 0x40200a80 PSTATE 0x600003c5\n"
ACK = b"gic_acknowledge_irq cpu 0 acknowledged irq 30\n"
EOI = b"gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n"
RETURN = b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40202020\n"
CYCLE = ENTRY + FROM + b"...with ELR 0x40202020\n" + VECTOR + ACK + EOI + RETURN
SETUP = b"gic_cpu_write cpu 0 iface write at 0x00000000 0x00000001\n"


class TimerTraceTests(unittest.TestCase):
    def rejected(self, trace, expected=1):
        result = validate_trace(trace, expected)
        self.assertFalse(result["passed"], result)
        self.assertTrue(result["errors"])

    def test_complete_sequences_and_unrelated_faults(self):
        fault = (b"Taking exception 4 [Data Abort] on CPU 0\n"
                 b"...from EL1 to EL1\n...to EL1 PC 0x40200a00 PSTATE 0x3c5\n"
                 b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40201111\n")
        result = validate_trace(SETUP + CYCLE * 16 + fault, 16)
        self.assertTrue(result["passed"], result)
        self.assertEqual(set(result["counts"].values()), {16})

    def test_timestamped_trace_events(self):
        trace = CYCLE.replace(ACK, b"100@123.000456:" + ACK)
        trace = trace.replace(EOI, b"100@123.000457:" + EOI)
        self.assertTrue(validate_trace(trace, 1)["passed"])

    def test_every_required_record_is_required(self):
        for record in (ENTRY, FROM, VECTOR, ACK, EOI, RETURN):
            with self.subTest(record=record):
                self.rejected(CYCLE.replace(record, b""))

    def test_duplicate_records_and_extra_interrupts(self):
        for record in (ENTRY, FROM, VECTOR, ACK, EOI, RETURN):
            with self.subTest(record=record):
                self.rejected(CYCLE.replace(record, record * 2))
        self.rejected(CYCLE * 2)

    def test_wrong_order_and_outside_irq_operations(self):
        self.rejected(CYCLE.replace(ACK + EOI, EOI + ACK))
        self.rejected(ACK + EOI + CYCLE)
        self.rejected(EOI + CYCLE)
        self.rejected(CYCLE.replace(ACK + EOI, ACK + RETURN + EOI))

    def test_wrong_ids_and_special_id_eoi(self):
        for irq in (29, 31, 1020, 1021, 1022, 1023):
            with self.subTest(irq=irq):
                self.rejected(CYCLE.replace(b"acknowledged irq 30", f"acknowledged irq {irq}".encode()))
                self.rejected(CYCLE.replace(b"0x0000001e", f"0x{irq:08x}".encode()))
        # Matching low interrupt ID is insufficient: EOIR uses the entire IAR.
        self.rejected(CYCLE.replace(b"0x0000001e", b"0x0000041e"))

    def test_wrong_cpu_vector_mode_and_alternate_eoi(self):
        for old, new in (
            (b"on CPU 0", b"on CPU 1"), (b"cpu 0", b"cpu 1"),
            (b"cpu 0", b"vcpu 0"), (b"0x40200a80", b"0x40200a00"),
            (b"0x600003c5", b"0x60000345"), (b"from EL1 to EL1", b"from EL0 to EL1"),
            (b"0x00000010", b"0x00000024"), (b"0x00000010", b"0x00001000"),
        ):
            with self.subTest(new=new):
                self.rejected(CYCLE.replace(old, new))

    def test_nested_fault(self):
        self.rejected(CYCLE.replace(ACK, b"Taking exception 4 [Data Abort] on CPU 0\n" + ACK))

    def test_missing_malformed_and_truncated_input(self):
        self.rejected(b"")
        self.rejected(CYCLE[:-1])
        self.rejected(CYCLE + b"\xff\n")
        for token in (b"acknowledged irq", b"iface write at", b"[IRQ]"):
            with self.subTest(token=token):
                self.rejected(CYCLE.replace(token, token + b" malformed"))
        for expected in (0, -1, True, None):
            with self.subTest(expected=expected):
                self.rejected(CYCLE, expected)


if __name__ == "__main__":
    unittest.main()
