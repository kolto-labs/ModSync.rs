#!/usr/bin/env python3
"""Source order proof: drives the production `modsync`.

C# `Instruction.SetRealPaths` lists a step's files in Source order
(`PathHelper.EnumerateFilesWithWildcards` walks the authored Sources in turn, and
`Distinct` keeps a file at its first place), and Copy, Move and Extract handle them
in that order, so when two Sources write the same file the later one wins. Rust
sorted the files by path, so the file from the alphabetically last Source won.
Each case is a disposable mod/game fixture plus a TOML guide: `mods/a/x.txt` is `A`,
`mods/b/x.txt` is `B`, `mods/b/y.txt` is `BY`, and `game/Override` has `old.txt`.
Both sides run on a fresh copy each:

  validate  (Rust only) plain `validate -i G -s mods -g game`, which decides as the
            C# CLI `install` (it validates first);
  install   `install -i G -s mods -g game --no-checkpoint` (C# also gets `-y`):
            exit code zero or not, and every file under `game/` and `mods/` with
            its text afterwards (C#'s `.modsync` folder left out).

The Extract cases (`pack.zip` and `pack.7z`, both extracting to `mods/pack/`) are
in ../extract-overwrite-proof.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live on a copy with a HoloPatcher stub under `unshare -Urnm` (no network, a tmpfs
over `/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also
rewrites csharp-oracle.json. Without it the recorded results are used. See
../source-order-contract.md.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
MD, OV = "<<modDirectory>>", "<<kotorDirectory>>/Override"
GUID = "55555555-6666-7777-8888-999999999991"


def step(action, sources, destination=OV, overwrite=True):
    return (action, [MD + "/" + source for source in sources], destination, overwrite)


CASES = {
    # Changed: the later Source's file wins.
    "copy_ba": [step("Copy", ["b/x.txt", "a/x.txt"])],
    "copy_wildcards_ba": [step("Copy", ["b/*", "a/*"])],
    "move_ba": [step("Move", ["b/x.txt", "a/x.txt"])],
    # A Source named twice keeps its first place.
    "copy_ba_b": [step("Copy", ["b/x.txt", "a/x.txt", "b/x.txt"])],
    # Overwrite = false: the first Source's file is kept.
    "copy_ba_false": [step("Copy", ["b/x.txt", "a/x.txt"], overwrite=False)],
    # Unchanged controls: Source order equals path order.
    "copy_ab": [step("Copy", ["a/x.txt", "b/x.txt"])],
    "move_ab": [step("Move", ["a/x.txt", "b/x.txt"])],
    "copy_ab_false": [step("Copy", ["a/x.txt", "b/x.txt"], overwrite=False)],
    "copy_one": [step("Copy", ["b/x.txt"])],
}

FILES = {"game/chitin.key": "KEY\n", "game/Override/old.txt": "old\n", "mods/a/x.txt": "A\n",
         "mods/b/x.txt": "B\n", "mods/b/y.txt": "BY\n"}


def guide(instructions):
    lines = ["[[thisMod]]", f'Guid = "{GUID}"', 'Name = "Source Order"', "IsSelected = true"]
    for action, sources, destination, overwrite in instructions:
        lines += ["", "[[thisMod.Instructions]]", f"Action = {json.dumps(action)}",
                  f"Source = {json.dumps(sources)}", f"Destination = {json.dumps(destination)}",
                  f"Overwrite = {'true' if overwrite else 'false'}"]
    return "\n".join(lines) + "\n"


def fixture(root, instructions):
    os.makedirs(os.path.join(root, "home"))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    """Every file under game/ and mods/ with its text."""
    out = []
    for folder in ("game", "mods"):
        for current, dirs, files in os.walk(os.path.join(root, folder)):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                full = os.path.join(current, name)
                with open(full) as handle:
                    out.append([os.path.relpath(full, root).replace(os.sep, "/"), handle.read()])
    return sorted(out)


def run_one(argv_head, instructions, work, label, mode, csharp, extra_env=None):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, instructions)
    common = ["-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    else:
        argv = argv_head + ["validate"] + common
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "install":
        return [done.returncode == 0, tree(root)]
    return done.returncode == 0


def stub_reference(reference, work):
    """A copy of the reference build with a HoloPatcher stub in its Resources folder."""
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    stub = os.path.join(copy_dir, "Resources", "holopatcher")
    with open(stub, "w") as handle:
        handle.write("#!/bin/sh\nexit 2\n")
    os.chmod(stub, 0o755)
    return copy_dir


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="source-order-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: {"install": run_one(head, steps, work, "csharp", "install", True, env)}
                      for name, steps in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = failed = 0
        binary = [os.path.abspath(args.bin)]
        for name, steps in CASES.items():
            expected = oracle[name]["install"]
            for mode in ("validate", "install"):
                actual = run_one(binary, steps, work, "rust", mode, False)
                same = actual == (expected[0] if mode == "validate" else expected)
                verdict = "match" if same else "FAIL"
                matched += same
                failed += not same
                detail = "" if same else f" | rust={actual} | reference install={expected}"
                print(f"{verdict:5} {name}/{mode}{detail}")
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
