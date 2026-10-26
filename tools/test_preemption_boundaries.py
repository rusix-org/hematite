import unittest

from check_preemption_boundaries import validate


class Boundaries(unittest.TestCase):
    def test_observed_delivery_required(self):
        ranges = {"start": (0x100, 0x110), "yield": (0x200, 0x210),
                  "complete": (0x300, 0x310)}
        trace = b"".join(f"Taking exception 5 [IRQ] on CPU 0\n...with ELR {pc:#x}\n".encode()
                         for pc in (0x100, 0x204, 0x308))
        self.assertTrue(validate(trace, ranges)["passed"])
        for damaged in (trace.replace(b"0x308", b"0x310"),
                        trace.replace(b"...with ELR 0x204\n", b""),
                        trace.replace(b"0x204", b"broken"),
                        trace.replace(b"0x204\n", b"0x204\n...with ELR 0x204\n")):
            self.assertFalse(validate(damaged, ranges)["passed"])
        self.assertFalse(validate(trace, {})["passed"])


if __name__ == "__main__":
    unittest.main()
