#!/usr/bin/env python3
"""Prove the preinstall validator rejects impossible instructions and accepts a valid guide.

Each case in fixtures/validator/cases.json names a guide, whether `modsync validate`
must accept it, and (for rejections) a message the output must contain. Every case runs
in a fresh temporary game and mod tree; the trees are hashed before and after so a
validator that writes, deletes or chmods anything fails the gate too.
"""

import argparse
import hashlib
import json
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures" / "validator"
TIMEOUT = 120


def load_cases(path):
    cases = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(cases, list) or not cases:
        raise ValueError("cases.json must be a non-empty list")
    names = set()
    for case in cases:
        if set(case) - {"name", "guide", "accept", "expect", "why"} or not {"name", "guide", "accept"} <= set(case):
            raise ValueError(f"Malformed case: {case}")
        if case["name"] in names:
            raise ValueError(f"Duplicate case: {case['name']}")
        names.add(case["name"])
        if not case["accept"] and not case.get("expect"):
            raise ValueError(f"Rejection case {case['name']} needs an expected message")
    if not any(case["accept"] for case in cases) or all(case["accept"] for case in cases):
        raise ValueError("Cases need at least one accepted and one rejected guide")
    return cases


def snapshot(root):
    """Map every path under root to (kind, mode, sha256, size); symlinks are recorded, not followed."""
    state = {}
    for path in sorted(Path(root).rglob("*")):
        info = path.lstat()
        rel = path.relative_to(root).as_posix()
        if stat.S_ISLNK(info.st_mode):
            state[rel] = ("link", str(path.readlink()))
        elif stat.S_ISDIR(info.st_mode):
            state[rel] = ("dir", stat.S_IMODE(info.st_mode))
        else:
            state[rel] = ("file", stat.S_IMODE(info.st_mode),
                          hashlib.sha256(path.read_bytes()).hexdigest(), info.st_size)
    return state


def prepare(work, guide, fixtures):
    game, mods = work / "game", work / "mods"
    (game / "Override").mkdir(parents=True)
    shutil.copytree(fixtures / "mods", mods)
    target = work / "guide.toml"
    shutil.copyfile(guide, target)
    return game, mods, target


def run_case(binary, case, fixtures=FIXTURES):
    with tempfile.TemporaryDirectory(prefix="validator-gate-") as temp:
        work = Path(temp)
        game, mods, guide = prepare(work, fixtures / case["guide"], fixtures)
        before = snapshot(work)
        try:
            process = subprocess.run(
                [str(binary), "validate", "--input", str(guide), "--game-dir", str(game),
                 "--source-dir", str(mods), "--non-interactive"],
                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=TIMEOUT, check=False)
        except subprocess.TimeoutExpired:
            failure = f"validator did not finish within {TIMEOUT}s"
        except OSError as error:
            failure = f"validator could not start ({type(error).__name__})"
        else:
            failure = None
        after = snapshot(work)
    if failure:
        # A hung or unstartable validator is a failed case, never a crashed gate.
        return {"case": case["name"], "accept": case["accept"], "exit": None,
                "ok": False, "problems": [failure]}, ""
    output = process.stdout + process.stderr
    problems = []
    if before != after:
        changed = sorted(key for key in before.keys() | after.keys() if before.get(key) != after.get(key))
        problems.append("validation changed the game or mod tree: " + ", ".join(changed[:10]))
    if case["accept"] and process.returncode != 0:
        problems.append(f"valid guide rejected (exit {process.returncode})")
    if not case["accept"]:
        if process.returncode == 0:
            problems.append("impossible guide accepted (exit 0)")
        elif case["expect"] not in output:
            problems.append(f"rejected without the expected reason {case['expect']!r}")
    if case["accept"] and case.get("expect") and case["expect"] not in output:
        problems.append(f"accepted output lacks {case['expect']!r}")
    return {"case": case["name"], "accept": case["accept"], "exit": process.returncode,
            "ok": not problems, "problems": problems}, output


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--cases", type=Path, default=FIXTURES / "cases.json")
    parser.add_argument("--show-output", action="store_true", help="print each case's validator output")
    args = parser.parse_args(argv)
    if not args.binary.is_file():
        print(json.dumps({"event": "validator_gate_failed", "reason": "binary not found"}), file=sys.stderr)
        return 2
    try:
        cases = load_cases(args.cases)
    except (OSError, ValueError) as error:
        print(json.dumps({"event": "validator_gate_failed", "reason": f"unusable cases file: {error}"}),
              file=sys.stderr)
        return 2
    failed = 0
    for case in cases:
        result, output = run_case(args.binary.resolve(), case, args.cases.parent)
        print(json.dumps({"event": "validator_case", **result}), flush=True)
        if args.show_output:
            print(output[-4000:], flush=True)
        if not result["ok"]:
            failed += 1
            print(f"::error title=Preinstall validator::{case['name']}: {'; '.join(result['problems'])}",
                  file=sys.stderr, flush=True)
    print(json.dumps({"event": "validator_gate", "cases": len(cases), "failed": failed}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
