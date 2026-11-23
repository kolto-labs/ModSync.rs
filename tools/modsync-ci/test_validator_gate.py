import contextlib
import io
import json
import os
import stat
import tempfile
import textwrap
import unittest
from pathlib import Path

import validator_gate as gate


def fake(directory, body):
    """A stand-in `modsync` that runs body with the guide path in $GUIDE and game dir in $GAME."""
    path = Path(directory) / "modsync"
    path.write_text(textwrap.dedent("""\
        #!/usr/bin/env python3
        import sys
        args = sys.argv[1:]
        GUIDE = args[args.index("--input") + 1]
        GAME = args[args.index("--game-dir") + 1]
        text = open(GUIDE).read()
        """) + textwrap.dedent(body))
    path.chmod(path.stat().st_mode | stat.S_IXUSR)
    return path


# Rejects exactly the guides whose expected messages it knows, by printing them.
HONEST = """
import json, os
cases = json.load(open(os.environ["GATE_CASES"]))
for case in cases:
    if not case["accept"] and open(os.path.join(os.path.dirname(os.environ["GATE_CASES"]), case["guide"])).read() == text:
        print("Error: " + case["expect"]); sys.exit(1)
sys.exit(0)
"""


class Gate(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        os.environ["GATE_CASES"] = str(gate.FIXTURES / "cases.json")
        self.addCleanup(os.environ.pop, "GATE_CASES", None)

    def run_gate(self, body):
        binary = fake(self.temp.name, body)
        with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
            code = gate.main(["--binary", str(binary)])
        events = [json.loads(line) for line in out.getvalue().splitlines()]
        return code, {e["case"]: e for e in events if e["event"] == "validator_case"}

    def test_a_validator_that_rejects_each_impossible_guide_for_its_reason_passes(self):
        code, cases = self.run_gate(HONEST)
        self.assertEqual(code, 0)
        self.assertTrue(all(case["ok"] for case in cases.values()))

    def test_a_validator_that_accepts_everything_fails_every_rejection_case(self):
        code, cases = self.run_gate("sys.exit(0)\n")
        self.assertEqual(code, 1)
        self.assertEqual(sorted(name for name, case in cases.items() if not case["ok"]),
                         sorted(c["name"] for c in gate.load_cases(gate.FIXTURES / "cases.json")
                                if not c["accept"]))

    def test_a_validator_that_rejects_everything_fails_the_positive_control(self):
        code, cases = self.run_gate("print('Error: Source matches no files Unsafe path'); sys.exit(1)\n")
        self.assertEqual(code, 1)
        self.assertFalse(cases["valid-guide"]["ok"])

    def test_rejecting_for_the_wrong_reason_fails(self):
        code, cases = self.run_gate("print('Error: something unrelated'); sys.exit(1)\n")
        self.assertEqual(code, 1)
        self.assertIn("expected reason", cases["missing-source"]["problems"][0])

    def test_a_validator_that_writes_to_the_game_directory_fails(self):
        code, cases = self.run_gate(
            "open(GAME + '/Override/written.tga', 'w').write('x')\n" + HONEST)
        self.assertEqual(code, 1)
        self.assertIn("changed the game or mod tree", cases["valid-guide"]["problems"][0])


class Cases(unittest.TestCase):
    def test_shipped_cases_are_well_formed_and_every_guide_exists(self):
        cases = gate.load_cases(gate.FIXTURES / "cases.json")
        for case in cases:
            self.assertTrue((gate.FIXTURES / case["guide"]).is_file(), case["guide"])

    def refused(self, cases):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "cases.json"
            path.write_text(json.dumps(cases))
            with self.assertRaises(ValueError):
                gate.load_cases(path)

    VALID = {"name": "ok", "guide": "ok.toml", "accept": True}
    REJECT = {"name": "bad", "guide": "bad.toml", "accept": False, "expect": "reason"}

    def test_a_case_set_without_a_positive_control_is_refused(self):
        self.refused([self.REJECT])

    def test_a_case_set_without_a_rejection_is_refused(self):
        self.refused([self.VALID])

    def test_an_empty_or_non_list_case_set_is_refused(self):
        self.refused([])
        self.refused({"cases": []})

    def test_a_case_missing_a_required_field_is_refused(self):
        for field in ("name", "guide", "accept"):
            self.refused([self.VALID, {k: v for k, v in self.REJECT.items() if k != field}])

    def test_a_rejection_without_an_expected_reason_is_refused(self):
        self.refused([self.VALID, {**self.REJECT, "expect": None}])
        self.refused([self.VALID, {**self.REJECT, "expect": ""}])

    def test_unknown_fields_and_duplicate_names_are_refused(self):
        self.refused([self.VALID, {**self.REJECT, "typo": 1}])
        self.refused([self.VALID, self.REJECT, dict(self.REJECT)])

    def test_the_smallest_valid_case_set_is_accepted(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "cases.json"
            path.write_text(json.dumps([self.VALID, self.REJECT]))
            self.assertEqual(len(gate.load_cases(path)), 2)


if __name__ == "__main__":
    unittest.main()
