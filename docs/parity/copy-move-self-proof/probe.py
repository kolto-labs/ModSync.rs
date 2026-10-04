#!/usr/bin/env python3
"""Copy/Move onto itself proof: drives the production `modsync`.

Each case is a disposable mod/game fixture (plain files, optionally read-only
or symlinked) plus a TOML guide. The guide copies or moves a file into the
directory that already holds it, so the target is the source itself, and then
copies `after.txt` into `Override` as a marker that the component went on. The
Rust binary runs plain `validate` (the strict install gate) and `install` on
fresh copies. Both decisions, and the final game and mod trees after a
successful install, are compared with the reference install. Reference results
are in csharp-oracle.json.

The reference result is ModComponent.InstallAsync with the real file system
(accepted on InstallExitCode.Success), from the oracle in csharp-oracle/ built
against ModSync.Core from d31ccbd3 (the delete-strict oracle, unchanged). The
oracle also records the reference dry run (DryRunValidator); it is kept for the
contract's notes and is not compared (see ../copy-move-self-contract.md).

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run.
"""
import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
GUID = "44444444-4444-4444-4444-444444444444"
OV = "<<kotorDirectory>>/Override"
X = OV + "/x.txt"
MARKER = ("Copy", ["<<modDirectory>>/after.txt"], OV, None)
BASE = {"game/Override/x.txt": "X\n", "mods/after.txt": "AFTER\n"}


def with_files(extra):
    files = dict(BASE)
    files.update(extra)
    return files


# name -> (files {root-relative path: text}, extras {"readonly": [...], "links": {path: target}}, instructions)
CASES = {
    "copy_false": (BASE, {}, [("Copy", [X], OV, False), MARKER]),
    "copy_true": (BASE, {}, [("Copy", [X], OV, True), MARKER]),
    "copy_omitted": (BASE, {}, [("Copy", [X], OV, None), MARKER]),
    "move_false": (BASE, {}, [("Move", [X], OV, False), MARKER]),
    "move_true": (BASE, {}, [("Move", [X], OV, True), MARKER]),
    "move_omitted": (BASE, {}, [("Move", [X], OV, None), MARKER]),
    "copy_wildcard_false": (
        with_files({"game/Override/y.txt": "Y\n"}),
        {},
        [("Copy", [OV + "/*.txt"], OV, False), MARKER],
    ),
    "copy_mixed_false": (
        with_files({"mods/z.txt": "Z\n"}),
        {},
        [("Copy", [X, "<<modDirectory>>/z.txt"], OV, False), MARKER],
    ),
    "move_mixed_false": (
        with_files({"mods/z.txt": "Z\n"}),
        {},
        [("Move", [X, "<<modDirectory>>/z.txt"], OV, False), MARKER],
    ),
    "copy_mod_directory_false": (
        with_files({"mods/m.txt": "M\n"}),
        {},
        [("Copy", ["<<modDirectory>>/m.txt"], "<<modDirectory>>", False), MARKER],
    ),
    "copy_subdirectory_false": (
        with_files({"game/Override/sub/s.txt": "S\n"}),
        {},
        [("Copy", [OV + "/sub/s.txt"], OV + "/sub", False), MARKER],
    ),
    "copy_readonly_false": (BASE, {"readonly": ["game/Override/x.txt"]}, [("Copy", [X], OV, False), MARKER]),
    "copy_source_case_false": (BASE, {}, [("Copy", [OV + "/X.TXT"], OV, False), MARKER]),
    "copy_destination_case_false": (BASE, {}, [("Copy", [X], "<<kotorDirectory>>/OVERRIDE", False), MARKER]),
    "copy_symlink_false": (
        BASE,
        {"links": {"game/Override/l.txt": "x.txt"}},
        [("Copy", [OV + "/l.txt"], OV, False), MARKER],
    ),
    "rename_self_false": (BASE, {}, [("Rename", [X], "x.txt", False), MARKER]),
}


# Deliberate Rust refusals: the reference accepts these; Rust must refuse them
# (counted as known differences, and as failures if Rust ever accepts).
KNOWN = {
    "copy_symlink_false": "Rust refuses symlinks in install trees",
}


def toml_string(value):
    return json.dumps(value)


def guide(instructions):
    lines = [
        "[[thisMod]]",
        f'Guid = "{GUID}"',
        'Name = "Self Target Probe"',
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


def fixture(root, case):
    files, extras, instructions = case
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    os.makedirs(os.path.join(game, "Override"))
    os.makedirs(mods)
    os.makedirs(os.path.join(root, "scratch"))
    for relative, text in files.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    for relative, target in extras.get("links", {}).items():
        os.symlink(target, os.path.join(root, relative))
    for relative in extras.get("readonly", []):
        os.chmod(os.path.join(root, relative), stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, mods, game


def tree(base):
    result = {}
    for directory, _, files in os.walk(base):
        for name in files:
            full = os.path.join(directory, name)
            key = os.path.relpath(full, base).replace(os.sep, "/")
            if os.path.islink(full):
                result[key] = "-> " + os.readlink(full)
                continue
            with open(full) as handle:
                result[key] = handle.read()
    return dict(sorted(result.items()))


def trees(mods, game):
    return {"game": tree(game), "mods": tree(mods)}


def make_writable(root):
    for directory, _, files in os.walk(root):
        for name in files:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    argv = [binary, mode, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    make_writable(root)
    return result


def run_csharp(dll, reference, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, mode, path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):])
    if mode == "dryrun":
        result = {"accepted": not raw["Errors"], "tree": None, "raw": raw}
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
        # The final tree is recorded for failures too (the contract notes C#'s
        # deleted file); only accepted trees are compared.
        result = {"accepted": accepted, "tree": trees(mods, game), "raw": raw}
    make_writable(root)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="copy-move-self-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, case, work) for mode in ("dryrun", "install")}
                for name, case in CASES.items()
            }
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            expected = oracle[name]["install"]
            # Plain validate is Rust's strict install gate: compare its decision
            # with the reference install. Install also compares the final trees.
            for mode in ("validate", "install"):
                actual = run_rust(args.bin, mode, case, work)
                expected_tree = expected["tree"] if mode == "install" and expected["accepted"] else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if name in KNOWN and expected["accepted"] and not actual["accepted"]:
                    known += 1
                    verdict = "known"
                elif same and name not in KNOWN:
                    matched += 1
                    verdict = "match"
                else:
                    failed += 1
                    verdict = "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
