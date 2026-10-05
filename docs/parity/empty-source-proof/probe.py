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

Since the empty-extract-report slice every case is also compared with the C# CLI
(csharp-cli-oracle.json): `validate --output json --full --dry-run` (`full`: exit
code zero or not, and `success`) and `validate --output json --dry-run`
(`basic_cli`: no error, and the ComponentValidation stage's error lines). Cases in
CLI_ONLY have no InstallAsync result; their plain `validate` and `install` are
compared with the C# CLI `install` (which validates first) instead.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR]
           [--cli-reference DIR] [--record]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. With --cli-reference (the d31ccbd3
build output holding ModSync.Core.dll), the C# CLI is run live on a copy with a
HoloPatcher stub under `unshare -Urnm` (a tmpfs over `/Override` when it exists),
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-cli-oracle.json.
See ../empty-source-contract.md and ../empty-extract-report-contract.md.
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
    # An empty Extract beside an archive that exists: C# validation skips it, and the
    # install fails at that step (MissingSourceFiles).
    "extract_empty_beside_archive": [
        step("Extract", [], ""),
        step("Extract", ["<<modDirectory>>/m.zip"], ""),
        step("Copy", ["<<modDirectory>>/m/c.txt"], "<<kotorDirectory>>/Override"),
    ],
}
# Recorded against the C# CLI only (csharp-oracle.json predates them).
CLI_ONLY = ("extract_empty_beside_archive",)


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
    if any("<<modDirectory>>/m.zip" in (sources or []) for _, sources, _, _ in instructions):
        with zipfile.ZipFile(os.path.join(mods, "m.zip"), "w") as archive:
            archive.writestr("c.txt", "C\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, mods, game


def tree(game):
    result = {}
    for directory, dirs, files in os.walk(game):
        dirs[:] = [d for d in dirs if d != ".modsync"]  # C# CLI bookkeeping
        for name in files:
            full = os.path.join(directory, name)
            with open(full) as handle:
                result[os.path.relpath(full, game).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def report_view(mode, code, stdout):
    """`full`: exit code zero or not, and `success`; `basic_cli`: no error, and the
    ComponentValidation stage's error lines."""
    report = json.loads(stdout[stdout.index("{"):])
    if mode == "full":
        return [code == 0, report.get("success")]
    stage = next((s for s in report.get("stages") or [] if s["stage"] == "ComponentValidation"), {})
    errors = [line for line in stage.get("messages") or [] if line.startswith("ERROR")]
    return [report.get("errorCount") == 0, errors]


REPORT_FLAGS = {"full": ["--full", "--dry-run"], "basic_cli": ["--dry-run"]}


def run_rust(binary, mode, instructions, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, instructions)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "dryrun":
        argv += ["--output", "json", "--dry-run"]
    if mode in REPORT_FLAGS:
        argv += ["--output", "json"] + REPORT_FLAGS[mode]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    if mode in REPORT_FLAGS:
        return report_view(mode, done.returncode, done.stdout)
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


def run_cli(reference, mode, instructions, work):
    """The C# CLI: the two reports, or `install` (it validates first, without
    --skip-validation)."""
    root = tempfile.mkdtemp(prefix=f"csharp-cli-{mode}-", dir=work)
    path, mods, game = fixture(root, instructions)
    script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
    head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
    common = ["-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv = head + ["install"] + common + ["--no-checkpoint", "-y"]
    else:
        argv = head + ["validate"] + common + ["--output", "json"] + REPORT_FLAGS[mode]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               DOTNET_ROLL_FORWARD="Major", DOTNET_CLI_TELEMETRY_OPTOUT="1")
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "install":
        accepted = done.returncode == 0
        return {"accepted": accepted, "tree": tree(game) if accepted else None}
    return report_view(mode, done.returncode, done.stdout)


# Rust mode -> (reference mode, compare the dry-run skip classification).
# `validate --output json --dry-run` is the DryRunValidator counterpart. Plain
# `validate` is Rust's strict install gate, so it must make the install decision
# (see COMPATIBILITY.md: missing sources are strict outside the dry-run report).
MODES = {"dryrun": ("validate", True), "validate": ("install", False), "install": ("install", False)}

# Known differences: name -> (Rust modes, reason). The Rust result must be a refusal
# where the reference accepts, or the check fails. Since the extract-destination slice
# (../extract-destination-contract.md) Rust's basic report fails an Extract with a
# Destination (the C# CLI basic report fails these cases on its empty-Source archive
# check first); this reference's dry run is DryRunValidator alone.
# Since the empty-extract-report slice (../empty-extract-report-contract.md) Rust's
# basic report fails an Extract with no Source in a component with no archive, as the
# C# CLI basic report does (`Missing Required Archives: []`, compared in `basic_cli`).
_CLI = ("Rust's basic report fails it as the C# CLI basic report does (`Missing "
        "Required Archives: []`); this reference runs DryRunValidator alone")
KNOWN = {name: (("dryrun",), _CLI) for name in ("extract_empty", "extract_blank_nodest", "extract_nokey")}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--cli-reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="empty-source-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.cli_reference:
            reference = stub_reference(os.path.abspath(args.cli_reference), work)
            cli_oracle = {
                name: {mode: run_cli(reference, mode, steps, work) for mode in ("full", "basic_cli", "install")}
                for name, steps in CASES.items()
            }
            if args.record:
                with open(CLI_ORACLE, "w") as handle:
                    json.dump(cli_oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(CLI_ORACLE) as handle:
                cli_oracle = json.load(handle)
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, steps, work) for mode in ("validate", "install")}
                for name, steps in CASES.items() if name not in CLI_ONLY
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
            for mode in REPORT_FLAGS:
                actual = run_rust(args.bin, mode, steps, work)
                expected = cli_oracle[name][mode]
                verdict = "match" if actual == expected else "FAIL"
                matched += verdict == "match"
                failed += verdict == "FAIL"
                print(f"{verdict:5} {name}/{mode}: rust={actual} | C# CLI={expected}")
            for mode, (reference_mode, compare_skip) in MODES.items():
                if name in CLI_ONLY:
                    if mode == "dryrun":
                        continue  # compared as basic_cli
                    expected, compare_skip = cli_oracle[name]["install"], False
                else:
                    expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, steps, work)
                expected_tree = expected["tree"] if mode == "install" else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if compare_skip:
                    same = same and actual["skipped"] == expected["skipped"]
                is_known = name in KNOWN and mode in KNOWN[name][0]
                if same and not is_known:
                    verdict = "match"
                elif (is_known and not same and not actual["accepted"]
                      and not cli_oracle[name]["basic_cli"][0]):
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
