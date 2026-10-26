#!/usr/bin/env python3
"""Failure-sensitive checks for the independent ELF reference/report boundary."""

import copy
import unittest

import check_elf as harness

HEADER = """ELF Header:
  Class:                             ELF64
  Data:                              2's complement, little endian
  Type:                              EXEC (Executable file)
  Machine:                           AArch64
  Entry point address:               0x100000000
  Number of program headers:         3
"""
PROGRAM = """Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x001000 0x0000000100000000 0x0000000100000000 0x000010 0x000010 R E 0x1000
  LOAD           0x002000 0x0000000100001000 0x0000000100001000 0x000010 0x000010 R   0x1000
  LOAD           0x003000 0x0000000100002000 0x0000000100002000 0x000010 0x002010 RW  0x1000
"""
DATA = bytes((i * 31 + 7) % 256 for i in range(0x3010))


def reference():
    return harness.check_reference(HEADER, PROGRAM, DATA)


def report(ref):
    lines = [f'HMELF IMAGE entry={ref["entry"]} pages={ref["pages"]} segments=3']
    for item in ref["segments"]:
        lines.append("HMELF LOAD " + " ".join(f"{key}={item[key]}" for key in
                     ("va", "filesz", "memsz", "flags", "page_start", "page_end", "file_hash")))
    lines.append(f"HMELF HOST PASS cases=5 mutations=32 truncations={len(DATA) + 1} random=4096")
    return "\n".join(lines) + "\n"


class ReferenceTests(unittest.TestCase):
    def test_reference_and_report_agree(self):
        ref = reference()
        self.assertTrue(ref["passed"])
        self.assertTrue(harness.check_report(report(ref), ref, len(DATA))["passed"])

    def test_foreign_header_identity_fails(self):
        for before, after in (("ELF64", "ELF32"), ("little endian", "big endian"),
                              ("EXEC (Executable file)", "DYN (Shared object file)"),
                              ("AArch64", "Advanced Micro Devices X86-64"),
                              ("0x100000000", "0x100000001"),
                              ("headers:         3", "headers:         4")):
            self.assertFalse(harness.check_reference(HEADER.replace(before, after), PROGRAM, DATA)["passed"])

    def test_bad_program_permissions_bss_and_bounds_fail(self):
        for changed in (PROGRAM.replace("R E", "RWE"), PROGRAM.replace("RW ", "R  "),
                        PROGRAM.replace("0x002010", "0x000010"),
                        PROGRAM.replace("0x003000", "0x008000"),
                        PROGRAM.replace("0x1000", "0x2000"),
                        PROGRAM.replace("0x0000000100002000", "0x0000000100001000"),
                        PROGRAM.replace("0x0000000100002000", "0x0000000100100000"),
                        PROGRAM.replace("  LOAD", "  NOTE", 1)):
            self.assertFalse(harness.check_reference(HEADER, changed, DATA)["passed"])

    def test_file_byte_mismatch_fails(self):
        ref = reference()
        changed = bytearray(DATA)
        changed[0x2000] ^= 1
        other = harness.check_reference(HEADER, PROGRAM, changed)
        self.assertTrue(other["passed"])
        self.assertFalse(harness.check_report(report(other), ref, len(DATA))["passed"])

    def test_metadata_mismatch_fails(self):
        ref = reference()
        for field in ("va", "filesz", "memsz", "flags", "page_start", "page_end", "file_hash"):
            changed = copy.deepcopy(ref)
            changed["segments"][1][field] += 1
            self.assertFalse(harness.check_report(report(changed), ref, len(DATA))["passed"])
        for field in ("entry", "pages"):
            changed = copy.deepcopy(ref)
            changed[field] += 1
            self.assertFalse(harness.check_report(report(changed), ref, len(DATA))["passed"])

    def test_incomplete_duplicate_or_extra_records_fail(self):
        ref = reference()
        good = report(ref)
        lines = good.splitlines(keepends=True)
        for changed in (good[:-1], "".join(lines[:-1]), "".join(lines[:2] + lines[3:]),
                        "".join(lines[:2] + [lines[1]] + lines[2:]), good + "extra\n",
                        good.replace("cases=5", "cases=4"),
                        good.replace("mutations=32", "mutations=31"),
                        good.replace("random=4096", "random=4095"),
                        good.replace(f"truncations={len(DATA) + 1}", "truncations=0"),
                        good.replace("flags=4", "flags=04")):
            self.assertFalse(harness.check_report(changed, ref, len(DATA))["passed"])


if __name__ == "__main__":
    unittest.main()
