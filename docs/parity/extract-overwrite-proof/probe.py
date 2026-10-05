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

Since the extract-destination slice an Extract Destination is never used, as in
the C# CLI and GUI, whose validation clears it before the install; the reference
InstallAsync alone extracts into it. So every case with a Destination is compared
with the C# CLI `install` instead (csharp-cli-oracle.json) and counted as a known
difference from InstallAsync; see ../extract-destination-contract.md.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR]
           [--cli-reference DIR] [--record]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. With --cli-reference (the d31ccbd3
build output holding ModSync.Core.dll), the C# CLI is run live on a copy with a
HoloPatcher stub under `unshare -Urnm` (a tmpfs over `/Override` when it exists),
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-cli-oracle.json.
See ../extract-overwrite-contract.md.
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
CLI_ORACLE = os.path.join(HERE, "csharp-cli-oracle.json")
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


# Cases whose Extract names a Destination: compared with the C# CLI install, which
# clears it in validation (ComponentValidation, AttemptFixes) and extracts next to
# the archive, as Rust does since extract-destination.
CLI_CASES = [name for name, case in CASES.items()
             if any(action == "Extract" and destination for action, _, destination, _ in case[2])]
# The C# CLI also refuses stale_false, stale_true and stale_omitted, for a reason that
# predates extract-destination: its validation (`VerifyExtractPaths`) finds no archive
# member for the Copy Source `out/x.txt`, a loose file here (`Failed to find
# 'out/x.txt' in any archives!`). Rust refuses them too since loose-source
# (../loose-source-contract.md).


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
    for directory, dirs, files in os.walk(base):
        dirs[:] = [d for d in dirs if d != ".modsync"]  # C# CLI bookkeeping
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


def run_cli(reference, case, work):
    """The C# CLI `install` (it validates first, without --skip-validation)."""
    root = tempfile.mkdtemp(prefix="csharp-cli-", dir=work)
    path, mods, game = fixture(root, case)
    script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
    argv = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll"),
            "install", "-i", path, "-s", mods, "-g", game, "--no-checkpoint", "-y"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               DOTNET_ROLL_FORWARD="Major", DOTNET_CLI_TELEMETRY_OPTOUT="1")
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    accepted = done.returncode == 0
    return {"accepted": accepted, "tree": trees(mods, game) if accepted else None}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--cli-reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="extract-overwrite-proof-")
    try:
        if args.cli_reference:
            reference = stub_reference(os.path.abspath(args.cli_reference), work)
            cli_oracle = {name: run_cli(reference, CASES[name], work) for name in CLI_CASES}
            if args.record:
                with open(CLI_ORACLE, "w") as handle:
                    json.dump(cli_oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(CLI_ORACLE) as handle:
                cli_oracle = json.load(handle)
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
                expected_tree = expected["tree"] if mode == "install" else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if name in CLI_CASES:
                    cli = cli_oracle[name]
                    cli_tree = cli["tree"] if mode == "install" else None
                    as_cli = actual["accepted"] == cli["accepted"] and actual["tree"] == cli_tree
                    verdict = ("match" if same else "known") if as_cli else "FAIL"
                    expected_tree = f"{expected_tree} | C# CLI accepted={cli['accepted']} tree={cli_tree}"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
