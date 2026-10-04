#!/usr/bin/env python3
"""Root destination proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose Copy, Move,
CleanList or DelDuplicate step names an install root itself as its Destination
(`<<kotorDirectory>>`, `<<gameDirectory>>`, `<<modDirectory>>`, with or without a
trailing separator). C# resolves the Destination with ReplaceCustomVariables and
FixPathFormatting, which trims the trailing separator, so the root is an existing
directory: the archive/destination stage (ComponentValidation.ParseDestinationWithAction)
passes and DelDuplicate (Instruction.DeleteDuplicateFile) runs in the root.

The Rust binary runs `validate --output json --dry-run` (the validation pipeline:
archive/destination stage, FOMOD gate and dry run), plain `validate` and `install`
on fresh copies. The pipeline is compared with the reference
InstallationValidationPipeline (DryRun, file selection): the archive/destination
stage's Passed flag, the dry-run decision and the DryRunSkipped classification.
Plain validate and install are compared with the reference install, and for an
accepted install the final game and mod files (names and contents). Reference
results are in csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated with
`unshare -rn`) instead of using the recorded results; --record also rewrites
csharp-oracle.json from that run. See ../root-destination-contract.md.
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
GUID = "77777777-7777-7777-7777-777777777777"
OPTION = "77777777-7777-7777-7777-777777777778"
KD = "<<kotorDirectory>>"
GD = "<<gameDirectory>>"
MD = "<<modDirectory>>"
OV = KD + "/Override"

# Every fixture starts from these files. The game root holds a TGA/TPC pair and a
# file the mod list names; the mod root holds its own TGA/TPC pair.
FILES = {
    "game/chitin.key": "KEY\n",
    "game/x.tga": "XTGA\n",
    "game/x.tpc": "XTPC\n",
    "game/r.txt": "R\n",
    "game/Override/o.txt": "O\n",
    "game/Override/q.tga": "QTGA\n",
    "game/Override/q.tpc": "QTPC\n",
    "mods/m.txt": "M\n",
    "mods/l.csv": "Mandatory Deletions,r.txt\n",
    "mods/y.tga": "YTGA\n",
    "mods/y.tpc": "YTPC\n",
}


def copy(destination, source=MD + "/m.txt"):
    return ("Copy", [source], destination, None)


def move(destination):
    return ("Move", [MD + "/m.txt"], destination, None)


def cleanlist(destination):
    return ("CleanList", [MD + "/l.csv"], destination, None)


def deldup(destination):
    return ("DelDuplicate", [], destination, ".tga")


# name -> (instructions, option instructions)
CASES = {
    "copy_override_control": ([copy(OV)], []),
    "copy_kotor": ([copy(KD)], []),
    "copy_kotor_slash": ([copy(KD + "/")], []),
    "copy_kotor_backslash": ([copy(KD + "\\")], []),
    "copy_game": ([copy(GD)], []),
    "copy_game_slash": ([copy(GD + "/")], []),
    "copy_to_mod_root": ([copy(MD, KD + "/Override/o.txt")], []),
    "move_kotor": ([move(KD)], []),
    "cleanlist_kotor": ([cleanlist(KD)], []),
    "cleanlist_kotor_slash": ([cleanlist(KD + "/")], []),
    "deldup_override_control": ([deldup(OV)], []),
    "deldup_kotor": ([deldup(KD)], []),
    "deldup_kotor_slash": ([deldup(KD + "/")], []),
    "deldup_game": ([deldup(GD)], []),
    "deldup_mod_root": ([deldup(MD)], []),
    "copy_then_deldup_kotor": ([copy(KD), deldup(KD)], []),
    "option_copy_kotor": ([("Choose", [OPTION], None, None)], [copy(KD)]),
    "copy_kotor_double_slash": ([copy(KD + "//")], []),
    "copy_kotor_dot": ([copy(KD + "/.")], []),
    "copy_override_dot": ([copy(OV + "/.")], []),
    "cleanlist_mod_root": ([("CleanList", [MD + "/l.csv"], MD, None)], []),
    "deldup_kotor_upper_extension": ([("DelDuplicate", [], KD, ".TGA")], []),
    "deldup_kotor_extension_sources": ([("DelDuplicate", [".tga", ".tpc"], KD, ".tpc")], []),
    "delete_source_kotor": ([("Delete", [KD], None, None)], []),
    "rename_destination_kotor": ([("Rename", [KD + "/r.txt"], KD, None)], []),
    "missing_folder_control": ([copy(KD + "/Missing")], []),
}


def toml_string(value):
    return json.dumps(value)


def step(lines, table, action, sources, destination, arguments):
    lines += ["", f"[[{table}]]", f"Action = {toml_string(action)}"]
    lines.append("Source = [" + ", ".join(toml_string(s) for s in sources) + "]")
    if destination is not None:
        lines.append(f"Destination = {toml_string(destination)}")
    if arguments is not None:
        lines.append(f"Arguments = {toml_string(arguments)}")


def guide(instructions, options):
    lines = ["[[thisMod]]", f'Guid = "{GUID}"', 'Name = "Root Destination Probe"', "IsSelected = true"]
    for instruction in instructions:
        step(lines, "thisMod.Instructions", *instruction)
    if options:
        lines += ["", "[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "One"', "IsSelected = true"]
        for instruction in options:
            step(lines, "thisMod.Options.Instructions", *instruction)
    return "\n".join(lines) + "\n"


def fixture(root, case):
    instructions, options = case
    for relative in ("scratch", "home", "xdg"):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    for relative, text in FILES.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions, options))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def files(base):
    result = {}
    for directory, _, names in os.walk(base):
        for name in names:
            full = os.path.join(directory, name)
            with open(full) as handle:
                result[os.path.relpath(full, base).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def trees(mods, game):
    return {"game": files(game), "mods": files(mods)}


def make_writable(root):
    for directory, _, names in os.walk(root):
        for name in names:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "pipeline":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    if mode == "pipeline":
        report = json.loads(done.stdout)
        stages = {stage["stage"]: stage for stage in report.get("stages", [])}
        issues = report.get("dryRun", {}).get("issues", [])
        component = stages.get("ComponentValidation", {})
        result["accepted"] = component.get("passed", False)
        result["dry_run"] = not any(issue.get("severity") == "Error" for issue in issues)
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
        result["stderr_tail"] = component.get("messages", [])[:1] if not result["accepted"] else []
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
    if mode == "pipeline":
        stages = {stage["Stage"]: stage for stage in raw["Stages"]}
        result = {
            "accepted": stages["ComponentValidation"]["Passed"],
            "tree": None,
            "dry_run": not raw["DryRunErrors"],
            "skipped": any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["DryRunWarnings"]),
            "raw": raw,
        }
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
        # The final tree is recorded for failures too; only accepted trees are compared.
        result = {"accepted": accepted, "tree": trees(mods, game), "raw": raw}
    make_writable(root)
    return result


# Known differences: name -> (Rust modes, reason). The Rust result must be a
# refusal (a failed stage, dry-run error or refused verb) where the reference
# accepts, or the check fails.
KNOWN = {
    "rename_destination_kotor": (
        ("pipeline", "validate", "install"),
        "Rename to a bare placeholder: C# renames r.txt to a file named '<<kotorDirectory>>';"
        " Rust resolves the placeholder to the game directory and refuses",
    ),
}


# Rust mode -> reference mode.
MODES = {"pipeline": "pipeline", "validate": "install", "install": "install"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="root-destination-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, case, work) for mode in ("pipeline", "install")}
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
            for mode, reference_mode in MODES.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, case, work)
                expected_tree = expected["tree"] if mode == "install" and expected["accepted"] else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                extra = ""
                if mode == "pipeline":
                    same = same and actual["dry_run"] == expected["dry_run"] and actual["skipped"] == expected["skipped"]
                    extra = (f" dry_run={actual['dry_run']}/{expected['dry_run']}"
                             f" skipped={actual['skipped']}/{expected['skipped']}")
                is_known = name in KNOWN and mode in KNOWN[name][0]
                refused = not actual["accepted"] or (mode == "pipeline" and not actual["dry_run"])
                if same and not is_known:
                    matched += 1
                    verdict = "match"
                elif is_known and not same and refused:
                    known += 1
                    verdict = "known"
                else:
                    failed += 1
                    verdict = "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}{extra}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
