#!/usr/bin/env python3
"""Rename Overwrite=false collision proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide. The Rust binary
runs `validate` and `install` on fresh copies; the accept/reject decision of
each, and the final game tree after a successful install, are compared with the
reference result recorded in csharp-oracle.json.

The reference result is DryRunValidator.ValidateInstallationAsync (validate:
accepted when it reports no Error issue) and ModComponent.InstallAsync with the
real file system (install: accepted on InstallExitCode.Success), from the
oracle in csharp-oracle/ built against ModSync.Core from d31ccbd3.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. See ../rename-overwrite-contract.md.
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
GUID = "11111111-1111-1111-1111-111111111111"
COPY_AB = ("Copy", ["<<modDirectory>>/a.txt", "<<modDirectory>>/b.txt"], "<<kotorDirectory>>/Override", None)
COPY_A = ("Copy", ["<<modDirectory>>/a.txt"], "<<kotorDirectory>>/Override", None)
COPY_A_SUB = ("Copy", ["<<modDirectory>>/a.txt"], "<<kotorDirectory>>/Sub", None)
RENAME = "Rename"

# name -> instructions [(action, sources, destination, overwrite or None)]
CASES = {
    "rename_clash_false": [COPY_AB, (RENAME, ["<<kotorDirectory>>/Override/a.txt"], "b.txt", False)],
    "rename_clash_default": [COPY_AB, (RENAME, ["<<kotorDirectory>>/Override/a.txt"], "b.txt", None)],
    "rename_clash_true": [COPY_AB, (RENAME, ["<<kotorDirectory>>/Override/a.txt"], "b.txt", True)],
    "rename_free_false": [COPY_A, (RENAME, ["<<kotorDirectory>>/Override/a.txt"], "b.txt", False)],
    "rename_clash_one_of_two": [
        COPY_AB,
        COPY_A_SUB,
        (RENAME, ["<<kotorDirectory>>/Sub/a.txt", "<<kotorDirectory>>/Override/a.txt"], "b.txt", False),
    ],
    "rename_clash_wildcard": [COPY_AB, (RENAME, ["<<kotorDirectory>>/Override/a.*"], "b.txt", False)],
    "rename_clash_then_later": [
        COPY_AB,
        (RENAME, ["<<kotorDirectory>>/Override/a.txt"], "b.txt", False),
        ("Delete", ["<<kotorDirectory>>/Override/b.txt"], "", None),
    ],
    "copy_clash_false": [COPY_AB, ("Copy", ["<<modDirectory>>/b.txt"], "<<kotorDirectory>>/Override", False)],
    "move_clash_false": [COPY_AB, ("Move", ["<<modDirectory>>/b.txt"], "<<kotorDirectory>>/Override", False)],
}


def toml_string(value):
    return json.dumps(value)


def guide(instructions):
    lines = [
        "[[thisMod]]",
        f'Guid = "{GUID}"',
        'Name = "Rename Probe"',
        "IsSelected = true",
    ]
    for action, sources, destination, overwrite in instructions:
        lines += ["", "[[thisMod.Instructions]]", f"Action = {toml_string(action)}"]
        lines.append("Source = [" + ", ".join(toml_string(s) for s in sources) + "]")
        if destination:
            lines.append(f"Destination = {toml_string(destination)}")
        if overwrite is not None:
            lines.append(f"Overwrite = {'true' if overwrite else 'false'}")
    return "\n".join(lines) + "\n"


def fixture(root, instructions):
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    for sub in ("Override", "Sub"):
        os.makedirs(os.path.join(game, sub))
    os.makedirs(mods)
    os.makedirs(os.path.join(root, "scratch"))
    for name, text in (("a.txt", "A\n"), ("b.txt", "B\n")):
        with open(os.path.join(mods, name), "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, mods, game


def tree(game):
    result = {}
    for directory, _, files in os.walk(game):
        for name in files:
            full = os.path.join(directory, name)
            with open(full) as handle:
                result[os.path.relpath(full, game).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def run_rust(binary, mode, instructions, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, instructions)
    argv = [binary, mode, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    return {
        "accepted": accepted,
        "tree": tree(game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }


def run_csharp(dll, reference, mode, instructions, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, instructions)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, "dryrun" if mode == "validate" else "install", path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):])
    if mode == "validate":
        accepted = not raw["Errors"]
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
    return {"accepted": accepted, "tree": tree(game) if accepted and mode == "install" else None, "raw": raw}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="rename-overwrite-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, steps, work) for mode in ("validate", "install")}
                for name, steps in CASES.items()
            }
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = failed = 0
        for name, steps in CASES.items():
            for mode in ("validate", "install"):
                expected = oracle[name][mode]
                actual = run_rust(args.bin, mode, steps, work)
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected["tree"]
                if same:
                    matched += 1
                else:
                    failed += 1
                verdict = "match" if same else "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected['tree']}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
