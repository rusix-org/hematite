import unittest

from test_wait_blocked import validate


class Blocked(unittest.TestCase):
    def test_requires_registration_and_retention(self):
        result = {"timed_out": True, "returncode": -9, "launch_error": None}
        good = b"hematite\nHMWAIT BLOCKED\n"
        self.assertTrue(all(validate(good, result).values()))
        for console in (b"", good[:-1], good + b"HMWAIT BLOCKED\n",
                        good + b"HMTEST BEGIN 33\n", good + b"HMEXCEPTION FATAL\n"):
            self.assertFalse(all(validate(console, result).values()))
        for change in ({"timed_out": False}, {"returncode": 0}, {"launch_error": "missing"}):
            self.assertFalse(all(validate(good, result | change).values()))


if __name__ == "__main__":
    unittest.main()
