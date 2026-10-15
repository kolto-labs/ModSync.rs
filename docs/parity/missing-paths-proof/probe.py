#!/usr/bin/env python3
"""Missing paths proof: drives the production `modsync`.

C# `validate` and `install` name a guide, game folder or mod folder that does not
exist (CLI/ModBuildConverter.cs:3030-3040, :3056-3078, :3318-3328, :3349-3353):
`Input file not found: <path>`, `Game directory not found: <path>`, `Source directory
not found: <path>`, as typed, the guide first, then the game folder, then the mod
folder; with `--output json` the message is the report's `error` field. Rust said
`No such file or directory (os error 2)` for either folder, naming neither, and `Read
<path>: No such file or directory (os error 2)` for the guide. Exit codes were already
the same (1).

Each case is a disposable fixture: `game/chitin.key`, `mods/a/x.txt`, a TOML guide `g.toml`
(one component copying `a/x.txt` to `Override`), a regular file `afile` and an empty
folder `adir`. Each command runs in the fixture with relative paths, under `unshare
-Urnm` on both sides, with `HOME` and `XDG_*` inside the fixture.

Checks per case:
  exit      the exit code is zero or not;
  message   with `--output json`, the report's `error` field; otherwise the first
            `Input file|Game directory|Source directory not found: <path>` text in the
            output (None when there is none);
  unchanged every file under `game` and `mods` is as before (no install started).
`conflicts` (no C# verb) and plain `validate` with complete folders are Rust-only
expectations (`RUST_ONLY`).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy (without `Logs`) holding a HoloPatcher stub (`exit 2`) under
`unshare -Urnm` (no network, a tmpfs over `/Override` when it exists) with
DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it the
recorded results are used. See ../missing-paths-contract.md.
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
FILES = {"game/chitin.key": "KEY\n", "game/Override/g.txt": "g\n", "mods/a/x.txt": "x\n",
         "afile": "not a folder\n", "g.toml": GUIDE}
J = ["--output", "json"]

# name: argv after the program (paths relative to the fixture)
CASES = {
    "game_missing_json_dry": ["validate", "-i", "g.toml", "-s", "mods", "-g", "nogame"] + J + ["--dry-run"],
    "game_missing_json_full": ["validate", "-i", "g.toml", "-s", "mods", "-g", "nogame"] + J + ["--full"],
    "mods_missing_json_full": ["validate", "-i", "g.toml", "-s", "nomods", "-g", "game"] + J + ["--full"],
    "both_missing_json_full": ["validate", "-i", "g.toml", "-s", "nomods", "-g", "nogame"] + J + ["--full"],
    "mods_missing_json_dro": ["validate", "-i", "g.toml", "-s", "nomods", "-g", "game"] + J + ["--full", "--dry-run-only"],
    "game_file_text_dry": ["validate", "-i", "g.toml", "-s", "mods", "-g", "afile", "--dry-run"],
    "mods_missing_text_full": ["validate", "-i", "g.toml", "-s", "nomods", "-g", "game", "--full"],
    "input_missing_json_full": ["validate", "-i", "nope.toml", "-s", "nomods", "-g", "nogame"] + J + ["--full"],
    "input_missing_json_basic": ["validate", "-i", "nope.toml"] + J,
    "input_dir_text": ["validate", "-i", "adir", "-s", "mods", "-g", "game"],
    "install_game_missing": ["install", "-i", "g.toml", "-s", "mods", "-g", "nogame"],
    "install_mods_missing": ["install", "-i", "g.toml", "-s", "nomods", "-g", "game"],
    "install_mods_file": ["install", "-i", "g.toml", "-s", "afile", "-g", "game"],
    "install_both_missing": ["install", "-i", "g.toml", "-s", "nomods", "-g", "nogame"],
    "install_input_missing": ["install", "-i", "nope.toml", "-s", "mods", "-g", "nogame"],
    "install_input_dir": ["install", "-i", "adir", "-s", "nomods", "-g", "game"],
    # Known: C#'s plain validate ignores -s/-g and fails its FOMOD stage instead.
    "plain_mods_missing": ["validate", "-i", "g.toml", "-s", "nomods", "-g", "game"],
    # Controls: every path exists.
    "ok_json_dry": ["validate", "-i", "g.toml", "-s", "mods", "-g", "game"] + J + ["--dry-run"],
    "ok_json_full": ["validate", "-i", "g.toml", "-s", "mods", "-g", "game"] + J + ["--full"],
}

# Rust only (no C# counterpart): name -> (argv, expected {check: value}).
RUST_ONLY = {
    "conflicts_game_missing": (["conflicts", "-i", "g.toml", "-s", "mods", "-g", "nogame"],
                               {"exit": False, "message": "Game directory not found: nogame",
                                "unchanged": True}),
    "conflicts_input_missing": (["conflicts", "-i", "nope.toml", "-s", "mods", "-g", "game"],
                                {"exit": False, "message": "Input file not found: nope.toml",
                                 "unchanged": True}),
}

# name -> {check: (why, Rust's documented value)}
KNOWN = {
    "plain_mods_missing": {
        "message": ("C#'s plain validate ignores -s/-g and fails its FOMOD stage; Rust uses "
                    "the folders and names the missing one",
                    "Source directory not found: nomods"),
    },
}

MESSAGE = re.compile(r"(?:Input file|Game directory|Source directory) not found: \S+")


def fixture(root):
    for name, text in FILES.items():
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    for folder in ("adir", "home", "xdg"):
        os.makedirs(os.path.join(root, folder), exist_ok=True)


def tree(root):
    out = []
    for folder in ("game", "mods"):
        for current, dirs, files in os.walk(os.path.join(root, folder)):
            dirs.sort()
            for name in sorted(files):
                full = os.path.join(current, name)
                with open(full, "rb") as handle:
                    out.append([os.path.relpath(full, root), handle.read().decode("utf-8", "replace")])
    # An install snapshot or pending marker beside the folders counts as a change.
    out += [[name, "beside"] for name in sorted(os.listdir(root)) if name.startswith(".modsync")]
    return out


def run_one(head, prefix, argv, work, label, env_extra=None):
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    fixture(root)
    before = tree(root)
    script = prefix + 'exec "$@"'
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg", "data"),
               XDG_CACHE_HOME=os.path.join(root, "xdg", "cache"), **(env_extra or {}))
    done = subprocess.run(["unshare", "-Urnm", "sh", "-c", script, "sh"] + head + argv,
                          capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if "--output" in argv:
        try:
            text = done.stdout
            message = json.loads(text[text.index("{"):]).get("error")
        except ValueError:
            message = "<no JSON report>"
    else:
        found = MESSAGE.search(done.stdout + done.stderr)
        message = found.group(0) if found else None
    return {"exit": done.returncode == 0, "message": message, "unchanged": tree(root) == before}


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
    work = tempfile.mkdtemp(prefix="missing-paths-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
