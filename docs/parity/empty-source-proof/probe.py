#!/usr/bin/env python3
"""Empty Source list proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide. The guide first
copies `a.txt` and `b.txt` into `Override`, then runs the case's steps, at
least one of which has no usable Source: `Source = []`, `Source = [""]` (blank
entries are dropped at load), or no Source key. The Rust binary runs
`validate --output json --dry-run`, plain `validate` and `install` on fresh
copies. The dry-run report's decision and its DryRunSkipped classification are
compared with the reference dry run. Plain validate is the strict install gate,
so its decision is compared with the reference install. The install decision,
and the final game tree after a successful install, are compared with the
reference install. Reference results are in csharp-oracle.json.

The reference result is DryRunValidator.ValidateInstallationAsync (validate:
accepted when it reports no Error issue) and ModComponent.InstallAsync with the
real file system (install: accepted on InstallExitCode.Success), from the
oracle in csharp-oracle/ built against ModSync.Core from d31ccbd3.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. See ../empty-source-contract.md.
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
GUID = "22222222-2222-2222-2222-222222222222"
COPY_AB = ("Copy", ["<<modDirectory>>/a.txt", "<<modDirectory>>/b.txt"], "<<kotorDirectory>>/Override", None)
OV = "<<kotorDirectory>>/Override/"


NO_SOURCE = None  # omit the Source key entirely


def delete(sources, overwrite):
    return ("Delete", sources if sources is NO_SOURCE else [OV + s for s in sources], "", overwrite)


def step(action, sources, destination, overwrite=None):
    return (action, sources, destination, overwrite)


# name -> instructions [(action, sources or NO_SOURCE, destination, overwrite or None)]
CASES = {
    "delete_empty_omitted": [COPY_AB, step("Delete", [], "")],
    "delete_empty_false": [COPY_AB, step("Delete", [], "", False)],
    "delete_empty_true": [COPY_AB, step("Delete", [], "", True)],
    "delete_blank_false": [COPY_AB, step("Delete", [""], "", False)],
    "delete_blank_true": [COPY_AB, step("Delete", [""], "", True)],
    "delete_nokey_false": [COPY_AB, delete(NO_SOURCE, False)],
    "delete_nokey_true": [COPY_AB, delete(NO_SOURCE, True)],
    "delete_empty_false_then_later": [COPY_AB, step("Delete", [], "", False), delete(["b.txt"], False)],
    "delete_empty_true_then_later": [COPY_AB, step("Delete", [], "", True), delete(["b.txt"], False)],
    "copy_empty": [COPY_AB, step("Copy", [], "<<kotorDirectory>>/Override")],
    "copy_blank": [COPY_AB, step("Copy", [""], "<<kotorDirectory>>/Override")],
    "move_empty": [COPY_AB, step("Move", [], "<<kotorDirectory>>/Override")],
    "move_empty_true": [COPY_AB, step("Move", [], "<<kotorDirectory>>/Override", True)],
    "extract_empty": [COPY_AB, step("Extract", [], "<<kotorDirectory>>/Override")],
    "extract_blank_nodest": [COPY_AB, step("Extract", [""], "")],
    "extract_nokey": [COPY_AB, step("Extract", NO_SOURCE, "<<kotorDirectory>>/Override")],
    "rename_empty_control": [COPY_AB, step("Rename", [], "x.txt")],
}


def toml_string(value):
    return json.dumps(value)


def guide(instructions):
    lines = [
        "[[thisMod]]",
        f'Guid = "{GUID}"',
        'Name = "Empty Source Probe"',
        "IsSelected = true",
    ]
    for action, sources, destination, overwrite in instructions:
        lines += ["", "[[thisMod.Instructions]]", f"Action = {toml_string(action)}"]
        if sources is not NO_SOURCE:
            lines.append("Source = [" + ", ".join(toml_string(s) for s in sources) + "]")
        if destination:
            lines.append(f"Destination = {toml_string(destination)}")
        if overwrite is not None:
            lines.append(f"Overwrite = {'true' if overwrite else 'false'}")
    return "\n".join(lines) + "\n"


def fixture(root, instructions):
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    os.makedirs(os.path.join(game, "Override"))
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
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "dryrun":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": tree(game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    if mode == "dryrun":
        report = json.loads(done.stdout)
        issues = report.get("dryRun", {}).get("issues", [])
        result["accepted"] = report["errorCount"] == 0
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
    return result


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
        skipped = any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["Warnings"])
        return {"accepted": accepted, "tree": None, "skipped": skipped, "raw": raw}
    accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
    return {"accepted": accepted, "tree": tree(game) if accepted and mode == "install" else None, "raw": raw}


# Rust mode -> (reference mode, compare the dry-run skip classification).
# `validate --output json --dry-run` is the DryRunValidator counterpart. Plain
# `validate` is Rust's strict install gate, so it must make the install decision
# (see COMPATIBILITY.md: missing sources are strict outside the dry-run report).
MODES = {"dryrun": ("validate", True), "validate": ("install", False), "install": ("install", False)}

# Known differences: name -> (Rust modes, reason). The Rust result must be a refusal
# where the reference accepts, or the check fails. Since the extract-destination slice
# (../extract-destination-contract.md) Rust's basic report fails an Extract with a
# Destination, as the C# CLI's does (ComponentValidation); this reference's dry run is
# DryRunValidator alone.
_DESTINATION = ("an Extract Destination fails Rust's basic report, as in the C# CLI's "
                "(extract-destination); this reference runs DryRunValidator alone")
KNOWN = {"extract_empty": (("dryrun",), _DESTINATION), "extract_nokey": (("dryrun",), _DESTINATION)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="empty-source-proof-")
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
        matched = known = failed = 0
        for name, steps in CASES.items():
            for mode, (reference_mode, compare_skip) in MODES.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, steps, work)
                expected_tree = expected["tree"] if mode == "install" else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if compare_skip:
                    same = same and actual["skipped"] == expected["skipped"]
                is_known = name in KNOWN and mode in KNOWN[name][0]
                if same and not is_known:
                    verdict = "match"
                elif is_known and not same and not actual["accepted"]:
                    verdict = "known"
                else:
                    verdict = "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                skip = f" skipped={actual['skipped']}/{expected['skipped']}" if compare_skip else ""
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}{skip}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
