#!/usr/bin/env python3
"""Failure-sensitive checks for per-CPU SMP trace and report attribution."""

import unittest

import test_smp as harness
from test_kernel import CASES as BASELINE_CASES


def entry(cpu):
    return (f"Taking exception 5 [IRQ] on CPU {cpu}\n"
            f"...from EL1 to EL1\n...to EL1 PC {0x40201280 + cpu * 4096:#x} PSTATE 0x4003c5\n").encode()


def ack(cpu):
    return f"123@12.123456:gic_acknowledge_irq cpu {cpu} acknowledged irq 30\n".encode()


def eoi(cpu):
    return f"123@12.123456:gic_cpu_write cpu {cpu} iface write at 0x00000010 0x0000001e\n".encode()


class TraceTests(unittest.TestCase):
    def test_interleaved_complete_transactions(self):
        raw = entry(0) + entry(1) + ack(1) + ack(0) + eoi(0) + eoi(1)
        self.assertTrue(harness.validate_irq_trace(raw, (1, 1))["passed"])

    def test_untagged_returns_do_not_choose_cpu(self):
        resume = b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40203000\n"
        raw = entry(0) + resume + entry(1) + ack(0) + resume + ack(1) + eoi(1) + eoi(0)
        self.assertTrue(harness.validate_irq_trace(raw, (1, 1))["passed"])

    def test_peer_ticks_cannot_satisfy_local_report(self):
        raw = entry(0) + ack(0) + eoi(0) + entry(0) + ack(0) + eoi(0)
        self.assertFalse(harness.validate_irq_trace(raw, (1, 1))["passed"])

    def test_wrong_cpu_ack_and_eoi_fail(self):
        for raw in (entry(0) + ack(1) + eoi(1), entry(0) + ack(0) + eoi(1),
                    entry(0) + ack(0) + eoi(0).replace(b"0000001e", b"0000001f")):
            self.assertFalse(harness.validate_irq_trace(raw)["passed"])

    def test_vector_table_belongs_to_reported_cpu(self):
        raw = entry(1) + ack(1) + eoi(1)
        roots = (0x40201000, 0x40202000)
        self.assertTrue(harness.validate_irq_trace(raw, (0, 1), vectors=roots)["passed"])
        self.assertFalse(harness.validate_irq_trace(raw.replace(b"0x40202280", b"0x40201280"),
                                                   (0, 1), vectors=roots)["passed"])

    def test_nested_or_ambiguous_entry_fails(self):
        for raw in (entry(0) + entry(0) + ack(0) + eoi(0),
                    b"Taking exception 5 [IRQ] on CPU 0\n" + entry(1) + ack(1) + eoi(1)):
            self.assertFalse(harness.validate_irq_trace(raw)["passed"])

    def test_malformed_or_foreign_trace_fails(self):
        good = entry(0) + ack(0) + eoi(0)
        for raw in (b"", good[:-1], good + b"\xff\n", good.replace(b"CPU 0", b"CPU 2"),
                    good.replace(b"irq 30", b"irq 31"), good.replace(b"cpu 0", b"vcpu 0"),
                    good.replace(b"0x40201280", b"0x40201200"), good.replace(b"0x4003c5", b"0x0"),
                    good.replace(b"0x00000010", b"0x00001000")):
            self.assertFalse(harness.validate_irq_trace(raw)["passed"])

    def test_timeout_may_end_only_with_final_partial_transactions(self):
        self.assertTrue(harness.validate_irq_trace(entry(0) + ack(0), allow_partial=True)["passed"])
        self.assertFalse(harness.validate_irq_trace(entry(0) + ack(0))["passed"])
        self.assertFalse(harness.validate_irq_trace(entry(0) + entry(0), allow_partial=True)["passed"])

    def test_unexpected_peer_fault_fails_positive(self):
        raw = b"Taking exception 4 [Data Abort] on CPU 1\n...from EL1 to EL1\n"
        self.assertFalse(harness.validate_irq_trace(raw)["passed"])
        self.assertTrue(harness.validate_irq_trace(raw, peer_fault=True)["passed"])


class OverflowTests(unittest.TestCase):
    def test_peer_far_and_vector(self):
        trace = (b"Taking exception 4 [Data Abort] on CPU 1\n...from EL1 to EL1\n"
                 b"...with ESR 0x25/0x96000007\n...with FAR 0x40300010\n"
                 b"...with ELR 0x40209000\n...to EL1 PC 0x40202200 PSTATE 0x4003c5\n"
                 b"Taking exception 5 [IRQ] on CPU 0\n"
                 b"Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40203000\n"
                 b"Taking exception 16 [Semihosting call] on CPU 1\n")
        self.assertTrue(harness.peer_overflow_trace(trace, 0x40300000, 0x40202000, (0x40209000, 0x40209100))["passed"])
        for probe in (None, (), (0x40209000, 0x40209000), (0x40209000, None)):
            self.assertFalse(harness.peer_overflow_trace(trace, 0x40300000, 0x40202000, probe)["passed"])
        for bad in (trace.replace(b"0x40300010", b"0x40301000"),
                    trace.replace(b"0x96000007", b"0x9600000f"),
                    trace.replace(b"0x40209000", b"0x40209100"),
                    trace.replace(b"0x40202200", b"0x40201200"),
                    trace.replace(b"Data Abort] on CPU 1", b"Data Abort] on CPU 0"),
                    trace.replace(b"...with FAR", b"Taking exception 5 [IRQ] on CPU 0\n...with FAR"),
                    trace.replace(b"Taking exception 16", b"Taking exception 11 [Hypervisor Call] on CPU 1\nTaking exception 16")):
            self.assertFalse(harness.peer_overflow_trace(bad, 0x40300000, 0x40202000, (0x40209000, 0x40209100))["passed"])


SYMBOLS = {"arm_secondary_root": 0x40210000, "__secondary_stack_guard": 0x40500000,
           "__secondary_stack_top": 0x40511000, "arm_vectors": 0x40201000,
           "arm_secondary_vectors": 0x40202000, "arm_smp_large_frame_range": [0x40209000, 0x40209100]}
for prefix, base in (("", 0x40300000), ("secondary_", 0x40400000)):
    for worker in range(4):
        SYMBOLS[f"__{prefix}thread{worker}_guard"] = base + worker * 0x11000
        SYMBOLS[f"__{prefix}thread{worker}_top"] = base + (worker + 1) * 0x11000
EXECUTION = {"returncode": 0, "timed_out": False, "launch_error": None}
HVC0 = b"Taking exception 11 [Hypervisor Call] on CPU 0\n...handled as PSCI call\n"
HVC1 = HVC0.replace(b"CPU 0", b"CPU 1")
EXIT0 = b"Taking exception 16 [Semihosting call] on CPU 0\n...handling as semihosting call 0x20\n"


def case_irqs(name):
    both = ("progress_contexts", "mixed_truncation", "allocation_isolation", "allocation_reentry")
    return int(name in both or name == "mutex"), int(name in both)


def positive_console():
    lines = ["hematite bootstrap", f"HMSCHED BEGIN {len(harness.CASES)}"]
    for generation, (name, completed0, completed1) in enumerate(harness.CASES, 1):
        irq0, irq1 = case_irqs(name)
        if name in harness.ALLOCATION_CASES:
            allocations, peak, drops = {
                "allocation_isolation": (1024, 24, 0),
                "allocation_exhaustion": (6000, 3000, 16),
                "allocation_reentry": (1280, 12, 512),
            }[name]
            lines.append(f"HMALLOC case={name} free_before=3000 free_after=3000 "
                         f"allocations={allocations} peak={peak} drops={drops}")
        worker0 = SYMBOLS["__thread0_top"] - 128 if completed0 else 0
        worker1 = SYMBOLS["__secondary_thread0_top"] - 128 if completed1 else 0
        lines.append(f"HMSCHED SESSION generation={generation} cpu0=0 cpu1=1 "
                     f"completed0={completed0} completed1={completed1} irq0={irq0} irq1={irq1} "
                     f"off=1 checks=255 root={SYMBOLS['arm_secondary_root']} "
                     f"sp={SYMBOLS['__secondary_stack_top'] - 128} worker0={worker0} worker1={worker1}")
        lines.append(f"HMSCHED PASS {name}")
    lines += [f"HMSCHED END {len(harness.CASES)}", f"HMTEST BEGIN {len(BASELINE_CASES)}"]
    lines += [f"HMTEST PASS {name}" for name in BASELINE_CASES]
    lines.append(f"HMTEST END {len(BASELINE_CASES)}")
    return ("\n".join(lines) + "\n").encode()


def positive_trace():
    counts = [sum(case_irqs(name)[cpu] for name, _, _ in harness.CASES) for cpu in (0, 1)]
    return (HVC0 + (entry(0) + ack(0) + eoi(0)) * (counts[0] + 16)
            + (entry(1) + ack(1) + eoi(1)) * counts[1] + HVC1 * len(harness.CASES) + EXIT0)


class ReportTests(unittest.TestCase):
    def test_complete_positive(self):
        self.assertTrue(harness.validate(positive_console(), positive_trace(), EXECUTION, SYMBOLS)["passed"])

    def test_exact_inventory_generations_and_counts(self):
        raw = positive_console()
        for before, after in ((b"HMSCHED BEGIN 11", b"HMSCHED BEGIN 7"),
                              (b"generation=2", b"generation=1"), (b"cpu1=1", b"cpu1=0"),
                              (b"completed0=2", b"completed0=1"), (b"off=1", b"off=0"),
                              (b"checks=255", b"checks=127"),
                              (b"HMSCHED PASS mutex\n", b""),
                              (b"HMSCHED END 11\n", b"HMSCHED PASS limits_reuse\nHMSCHED END 11\n"),
                              (b"generation=1 ", b"generation=" + b"9" * 5000 + b" ")):
            self.assertFalse(harness.validate(raw.replace(before, after), positive_trace(), EXECUTION, SYMBOLS)["passed"])

    def test_allocation_metrics_inventory_and_position(self):
        raw = positive_console()
        metrics = [line for line in raw.splitlines(keepends=True) if line.startswith(b"HMALLOC")]
        self.assertEqual(len(metrics), 3)
        for bad in (raw.replace(metrics[0], b""), raw + metrics[0],
                    raw.replace(metrics[0], metrics[0] + metrics[0]),
                    raw.replace(metrics[0], metrics[1]).replace(metrics[2], metrics[0]),
                    metrics[0] + raw.replace(metrics[0], b""),
                    raw.replace(metrics[0], metrics[0].replace(b"HMALLOC", b"prefix HMALLOC")),
                    raw.replace(metrics[0], metrics[0].replace(b"free_before=3000", b"free_before=03000")),
                    raw.replace(metrics[0], metrics[0].replace(b"case=allocation_isolation", b"case=unknown")),
                    raw + metrics[0].rstrip(b"\n"),
                    raw.replace(metrics[0], metrics[0].replace(b"case=", b"case=\xff"))):
            result = harness.validate(bad, positive_trace(), EXECUTION, SYMBOLS)
            self.assertFalse(result["passed"])
            self.assertFalse(result["checks"]["allocation_inventory_and_metrics"])

    def test_allocation_metrics_exact_accounting(self):
        raw = positive_console()
        for before, after in ((b"free_after=3000", b"free_after=2999"),
                              (b"free_before=3000", b"free_before=0"),
                              (b"free_before=3000 free_after=3000", b"free_before=4097 free_after=4097"),
                              (b"allocations=1024", b"allocations=1023"),
                              (b"peak=24 drops=0", b"peak=23 drops=0"),
                              (b"peak=24 drops=0", b"peak=24 drops=1"),
                              (b"allocations=6000", b"allocations=3000"),
                              (b"peak=3000", b"peak=2999"),
                              (b"drops=16", b"drops=15"),
                              (b"allocations=1280", b"allocations=1279"),
                              (b"peak=12", b"peak=0"),
                              (b"peak=12", b"peak=3001"),
                              (b"drops=512", b"drops=511"),
                              (b"case=allocation_reentry free_before=3000 free_after=3000",
                               b"case=allocation_reentry free_before=2999 free_after=2999")):
            result = harness.validate(raw.replace(before, after), positive_trace(), EXECUTION, SYMBOLS)
            self.assertFalse(result["passed"])
            self.assertFalse(result["checks"]["allocation_inventory_and_metrics"])

    def test_allocation_preemption_required_on_both_cpus(self):
        raw = positive_console()
        for generation in (9, 11):
            line = next(line for line in raw.splitlines() if line.startswith(
                f"HMSCHED SESSION generation={generation} ".encode()))
            for field in (b"irq0=1", b"irq1=1"):
                changed = line.replace(field, field[:-1] + b"0")
                result = harness.validate(raw.replace(line, changed), positive_trace(), EXECUTION, SYMBOLS)
                self.assertFalse(result["checks"]["exact_sessions_and_elf_addresses"])

    def test_runtime_addresses_match_elf_symbols(self):
        raw = positive_console()
        for before, after in ((f"root={SYMBOLS['arm_secondary_root']}".encode(), b"root=0"),
                              (f"sp={SYMBOLS['__secondary_stack_top'] - 128}".encode(), b"sp=0"),
                              (f"worker0={SYMBOLS['__thread0_top'] - 128}".encode(), b"worker0=0"),
                              (f"worker1={SYMBOLS['__secondary_thread0_top'] - 128}".encode(),
                               f"worker1={SYMBOLS['__secondary_thread0_guard']}".encode()),
                              (b"worker0=0 worker1=0", b"worker0=16 worker1=0")):
            self.assertFalse(harness.validate(raw.replace(before, after), positive_trace(), EXECUTION, SYMBOLS)["passed"])
        self.assertFalse(harness.validate(raw, positive_trace(), EXECUTION, {})["passed"])

    def test_report_and_trace_local_counts_must_agree(self):
        raw, trace = positive_console(), positive_trace()
        for bad in (raw.replace(b"irq1=1", b"irq1=0"), raw.replace(b"irq0=1", b"irq0=2")):
            self.assertFalse(harness.validate(bad, trace, EXECUTION, SYMBOLS)["passed"])
        for bad in (trace.replace(HVC1, b""), trace + HVC1, trace.replace(ack(1), ack(0)),
                    trace.replace(HVC0, b"")):
            self.assertFalse(harness.validate(raw, bad, EXECUTION, SYMBOLS)["passed"])

    def test_baseline_and_clean_termination_required(self):
        raw = positive_console()
        split = raw.index(b"HMTEST BEGIN")
        for bad in (raw[:split], raw[split:] + raw[:split], raw.replace(b"HMTEST PASS wx", b"HMTEST FAIL wx"),
                    raw + b"HMSCHED END 11", raw.replace(b"HMSCHED PASS mutex", b"HMSCHED PASS mut\xffex")):
            self.assertFalse(harness.validate(bad, positive_trace(), EXECUTION, SYMBOLS)["passed"])
        for change in ({"returncode": 1}, {"timed_out": True}, {"launch_error": "QEMU missing"}):
            self.assertFalse(harness.validate(raw, positive_trace(), {**EXECUTION, **change}, SYMBOLS)["passed"])


class NegativeTests(unittest.TestCase):
    def test_peer_timer_withheld_after_actual_entry(self):
        raw = (f"HMSCHED EXPECT NO-PEER-TIMER\nHMSCHED STARTED cpu=1 "
               f"sp={SYMBOLS['__secondary_thread0_top'] - 128}\n").encode()
        trace = HVC0 + entry(0) + ack(0) + eoi(0)
        result = {**EXECUTION, "returncode": -9, "timed_out": True}
        self.assertTrue(harness.validate(raw, trace, result, SYMBOLS, "no-peer-timer")["passed"])
        for bad in (raw.split(b"HMSCHED STARTED")[0], raw.replace(b"cpu=1", b"cpu=0"),
                    raw.replace(str(SYMBOLS['__secondary_thread0_top'] - 128).encode(), b"0")):
            self.assertFalse(harness.validate(bad, trace, result, SYMBOLS, "no-peer-timer")["passed"])
        for bad in (HVC0, trace + entry(1) + ack(1) + eoi(1), trace + HVC1):
            self.assertFalse(harness.validate(raw, bad, result, SYMBOLS, "no-peer-timer")["passed"])
        self.assertFalse(harness.validate(raw, trace, EXECUTION, SYMBOLS, "no-peer-timer")["passed"])

    def test_both_blocked_observed_and_retained(self):
        raw = b"HMSCHED EXPECT ALL-BLOCKED\nHMSCHED BLOCKED cpu0=1 cpu1=1\n"
        result = {**EXECUTION, "returncode": -9, "timed_out": True}
        self.assertTrue(harness.validate(raw, HVC0, result, SYMBOLS, "all-blocked")["passed"])
        for bad in (raw.replace(b"cpu1=1", b"cpu1=0"), raw.split(b"HMSCHED BLOCKED")[0],
                    raw + b"HMSCHED PASS all_blocked\n", raw + b"HMTEST BEGIN 39\n"):
            self.assertFalse(harness.validate(bad, HVC0, result, SYMBOLS, "all-blocked")["passed"])
        for change in ({"returncode": 0}, {"timed_out": False}, {"launch_error": "bad launch"}):
            self.assertFalse(harness.validate(raw, HVC0, {**result, **change}, SYMBOLS, "all-blocked")["passed"])

    def test_negative_profile_rejects_any_allocation_output(self):
        raw = b"HMSCHED EXPECT ALL-BLOCKED\nHMSCHED BLOCKED cpu0=1 cpu1=1\n"
        execution = {**EXECUTION, "returncode": -9, "timed_out": True}
        for extra in (b"HMALLOC case=allocation_isolation free_before=3000 free_after=3000 allocations=1024 peak=24 drops=0\n",
                      b"garbage HMALLOC\n"):
            result = harness.validate(raw + extra, HVC0, execution, SYMBOLS, "all-blocked")
            self.assertFalse(result["passed"])
            self.assertFalse(result["checks"]["no_allocation_report"])

    def test_peer_overflow_exit_is_specific(self):
        raw = b"HMSCHED EXPECT PEER-OVERFLOW\n"
        guard = SYMBOLS["__secondary_thread0_guard"]
        vector = SYMBOLS["arm_secondary_vectors"] + 0x200
        trace = HVC0 + (f"Taking exception 4 [Data Abort] on CPU 1\n...from EL1 to EL1\n"
                        f"...with ESR 0x25/0x96000007\n...with FAR {guard:#x}\n"
                        f"...with ELR 0x40209000\n...to EL1 PC {vector:#x} PSTATE 0x4003c5\n").encode()
        trace += EXIT0.replace(b"CPU 0", b"CPU 1")
        result = {**EXECUTION, "returncode": 5}
        self.assertTrue(harness.validate(raw, trace, result, SYMBOLS, "peer-overflow")["passed"])
        for change in ({"returncode": 2}, {"returncode": 0}, {"timed_out": True}):
            self.assertFalse(harness.validate(raw, trace, {**result, **change}, SYMBOLS, "peer-overflow")["passed"])
        self.assertFalse(harness.validate(raw, trace.replace(b"CPU 1", b"CPU 0"), result, SYMBOLS, "peer-overflow")["passed"])
        self.assertFalse(harness.validate(raw, b"Taking exception 4 [Data Abort] on CPU 0\n" + trace,
                                          result, SYMBOLS, "peer-overflow")["passed"])


if __name__ == "__main__":
    unittest.main()
