#!/usr/bin/env python3
"""Missing guides proof: drives the production `modsync`.

C# `convert` and `merge` name a guide that does not exist before reading anything
(CLI/ModBuildConverter.cs:1966-1970, :2473-2483): `Input file not found: <path>`
for convert, `Existing file not found: <path>` then `Incoming file not found:
<path>` for merge (and `convert --merge`, which C# hands to merge at :1905-1944),
the path as typed. `File.Exists` is false for a folder, so a folder gets the same
message. Rust said `Read <path>: No such file or directory (os error 2)`, and for a
folder `Read <path>: Archive source must be a regular file, ...`. Exit codes were
already the same (1).

Each case is a disposable fixture: a TOML guide `g.toml` (one component), an empty
folder `adir`; `nope.toml` and `nope2.toml` do not exist. Each command runs in the
fixture with relative paths, under `unshare -Urnm` on both sides, with `HOME` and
`XDG_*` inside the fixture.

Checks per case:
  exit      the exit code is zero or not;
  message   the first `Input|Existing|Incoming file not found: <path>` text in stdout
            and stderr (None when there is none);
  written   the sorted names in the fixture's top folder that were not there before
            (an output file, or none).
Rust's positional forms (no C# counterpart) are Rust-only expectations (`RUST_ONLY`).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy (without `Logs`) holding a HoloPatcher stub (`exit 2`) under
`unshare -Urnm` (no network, a tmpfs over `/Override` when it exists) with
DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it the
recorded results are used. See ../missing-guides-contract.md.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
STUB = "#!/bin/sh\nexit 2\n"
GUIDE = """[[thisMod]]
Guid = "5a5a5a5a-1111-2222-3333-444444444441"
Name = "A"
IsSelected = true

[[thisMod.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/a/x.txt"]
Destination = "<<kotorDirectory>>/Override"
Overwrite = true
"""
FILES = {"g.toml": GUIDE}

# name: argv after the program (paths relative to the fixture)
CASES = {
    "convert_missing": ["convert", "-i", "nope.toml"],
    "convert_missing_out": ["convert", "-i", "nope.toml", "-o", "out.toml"],
    "convert_missing_json": ["convert", "-i", "nope.toml", "-f", "json", "-o", "out.json"],
    "convert_dir": ["convert", "-i", "adir", "-o", "out.toml"],
    "merge_existing_missing": ["merge", "-e", "nope.toml", "-n", "g.toml", "-o", "out.toml"],
    "merge_incoming_missing": ["merge", "-e", "g.toml", "-n", "nope.toml", "-o", "out.toml"],
    "merge_both_missing": ["merge", "-e", "nope.toml", "-n", "nope2.toml"],
    "merge_existing_dir": ["merge", "-e", "adir", "-n", "g.toml"],
    "merge_incoming_dir": ["merge", "-e", "g.toml", "-n", "adir", "-o", "out.toml"],
    "legacy_existing_missing": ["convert", "--merge", "-e", "nope.toml", "-n", "g.toml", "-o", "out.toml"],
    "legacy_incoming_missing": ["convert", "--merge", "-e", "g.toml", "-n", "nope.toml"],
    # Controls: every guide exists.
    "convert_ok": ["convert", "-i", "g.toml", "-o", "out.toml"],
    "merge_ok": ["merge", "-e", "g.toml", "-n", "g.toml", "-o", "out.toml"],
}

# Rust only (no C# counterpart): name -> (argv, expected {check: value}).
RUST_ONLY = {
    "convert_positional_missing": (["convert", "nope.toml", "out.toml"],
                                   {"exit": False, "message": "Input file not found: nope.toml",
                                    "written": []}),
    "merge_positional_incoming_missing": (["merge", "g.toml", "nope.toml", "out.toml"],
                                          {"exit": False,
                                           "message": "Incoming file not found: nope.toml",
                                           "written": []}),
}

# name -> {check: (why, Rust's documented value)}
KNOWN = {}

MESSAGE = re.compile(r"(?:Input|Existing|Incoming) file not found: \S+")


def fixture(root):
    for name, text in FILES.items():
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    for folder in ("adir", "home", "xdg"):
        os.makedirs(os.path.join(root, folder), exist_ok=True)


def run_one(head, prefix, argv, work, label, env_extra=None):
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    fixture(root)
    before = set(os.listdir(root))
    script = prefix + 'exec "$@"'
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg", "data"),
               XDG_CACHE_HOME=os.path.join(root, "xdg", "cache"), **(env_extra or {}))
    done = subprocess.run(["unshare", "-Urnm", "sh", "-c", script, "sh"] + head + argv,
                          capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    found = MESSAGE.search(done.stdout + done.stderr)
    return {"exit": done.returncode == 0, "message": found.group(0) if found else None,
            "written": sorted(set(os.listdir(root)) - before)}


def stub_reference(reference, work):
    """A copy of the reference build with a HoloPatcher stub in its Resources."""
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    stub = os.path.join(copy_dir, "Resources", "holopatcher")
    with open(stub, "w") as handle:
        handle.write(STUB)
    os.chmod(stub, 0o755)
    return copy_dir


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="missing-guides-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            head = ["dotnet", os.path.join(reference, "ModSync.Core.dll")]
            prefix = "if [ -d /Override ]; then mount -t tmpfs none /Override; fi; "
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_one(head, prefix, argv, work, "csharp", env)
                      for name, argv in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        rust = [os.path.abspath(args.bin)]
        matched = known = failed = 0

        def verdict(name, check, actual, expected):
            nonlocal matched, known, failed
            same = actual == expected
            if check in KNOWN.get(name, {}):
                result = "known" if not same and actual == KNOWN[name][check][1] else "FAIL"
            else:
                result = "match" if same else "FAIL"
            matched += result == "match"
            known += result == "known"
            failed += result == "FAIL"
            tail = f" | rust={actual!r} | expected={expected!r}" if result == "FAIL" else ""
            print(f"{result:5} {name}/{check}{tail}")

        for name, argv in CASES.items():
            actual = run_one(rust, "", argv, work, "rust")
            for check in sorted(oracle[name]):
                verdict(name, check, actual[check], oracle[name][check])
        for name, (argv, expected) in RUST_ONLY.items():
            actual = run_one(rust, "", argv, work, "rust")
            for check in sorted(expected):
                verdict(name, check, actual[check], expected[check])
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
