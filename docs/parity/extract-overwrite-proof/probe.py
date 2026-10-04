#!/usr/bin/env python3
"""Extract Overwrite=false proof: drives the production `modsync`.

Each case is a disposable mod/game fixture (ZIP archives, optional files that
already exist where the archive will extract) plus a TOML guide. The guide
extracts the archives, and in most cases then copies the extracted `x.txt` into
`Override`. The Rust binary runs plain `validate` (the strict install gate) and
`install` on fresh copies. Both decisions, and the final game tree and the
extracted mod files after a successful install, are compared with the reference
install. Reference results are in csharp-oracle.json.

The reference result is ModComponent.InstallAsync with the real file system
(accepted on InstallExitCode.Success), from the oracle in csharp-oracle/ built
against ModSync.Core from d31ccbd3. The oracle also records the reference dry
run (DryRunValidator) for each case; it is kept for the contract's notes and is
not compared (see ../extract-overwrite-contract.md).

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. See ../extract-overwrite-contract.md.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
GUID = "33333333-3333-3333-3333-333333333333"
OUT = "<<modDirectory>>/out"
OV = "<<kotorDirectory>>/Override"
COPY_OUT = ("Copy", [OUT + "/x.txt"], OV, None)


def extract(archives, destination, overwrite):
    return ("Extract", ["<<modDirectory>>/" + a for a in archives], destination, overwrite)


# name -> (archives {zip: {entry: text}}, existing {root-relative path: text}, instructions)
PACK = {"pack.zip": {"x.txt": "NEW\n", "y.txt": "Y\n"}}
TWO = {"a.zip": {"x.txt": "FROM A\n"}, "b.zip": {"x.txt": "FROM B\n"}}
STALE = {"mods/out/x.txt": "OLD\n"}
CASES = {
    "stale_false": (PACK, STALE, [extract(["pack.zip"], OUT, False), COPY_OUT]),
    "stale_true": (PACK, STALE, [extract(["pack.zip"], OUT, True), COPY_OUT]),
    "stale_omitted": (PACK, STALE, [extract(["pack.zip"], OUT, None), COPY_OUT]),
    "fresh_false": (PACK, {}, [extract(["pack.zip"], OUT, False), COPY_OUT]),
    "stale_default_destination_false": (
        PACK,
        {"mods/pack/x.txt": "OLD\n"},
        [extract(["pack.zip"], "", False), ("Copy", ["<<modDirectory>>/pack/x.txt"], OV, None)],
    ),
    "two_archives_false": (TWO, {}, [extract(["a.zip", "b.zip"], OUT, False), COPY_OUT]),
    "two_archives_true": (TWO, {}, [extract(["a.zip", "b.zip"], OUT, True), COPY_OUT]),
    "two_steps_false": (
        TWO,
        {},
        [extract(["a.zip"], OUT, False), extract(["b.zip"], OUT, False), COPY_OUT],
    ),
    "game_override_false": (PACK, {"game/Override/x.txt": "OLD\n"}, [extract(["pack.zip"], OV, False)]),
    "game_override_true": (PACK, {"game/Override/x.txt": "OLD\n"}, [extract(["pack.zip"], OV, True)]),
}


def toml_string(value):
    return json.dumps(value)


def guide(instructions):
    lines = [
        "[[thisMod]]",
        f'Guid = "{GUID}"',
        'Name = "Extract Probe"',
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
    archives, existing, instructions = case
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    os.makedirs(os.path.join(game, "Override"))
    os.makedirs(mods)
    os.makedirs(os.path.join(root, "scratch"))
    for name, entries in archives.items():
        with zipfile.ZipFile(os.path.join(mods, name), "w") as archive:
            for entry, text in entries.items():
                archive.writestr(zipfile.ZipInfo(entry, (2020, 1, 2, 3, 4, 6)), text)
    for relative, text in existing.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, mods, game


def tree(base, skip_archives=False):
    result = {}
    for directory, _, files in os.walk(base):
        for name in files:
            if skip_archives and name.endswith(".zip"):
                continue
            full = os.path.join(directory, name)
            with open(full) as handle:
                result[os.path.relpath(full, base).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def trees(mods, game):
    return {"game": tree(game), "mods": tree(mods, skip_archives=True)}


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    argv = [binary, mode, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    return {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }


def run_csharp(dll, reference, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, mode, path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):])
    if mode == "dryrun":
        return {"accepted": not raw["Errors"], "tree": None, "raw": raw}
    accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
    return {"accepted": accepted, "tree": trees(mods, game) if accepted else None, "raw": raw}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="extract-overwrite-proof-")
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
        matched = failed = 0
        for name, case in CASES.items():
            expected = oracle[name]["install"]
            # Plain validate is Rust's strict install gate: compare its decision
            # with the reference install. Install also compares the final trees.
            for mode in ("validate", "install"):
                actual = run_rust(args.bin, mode, case, work)
                expected_tree = expected["tree"] if mode == "install" else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if same:
                    matched += 1
                else:
                    failed += 1
                verdict = "match" if same else "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
