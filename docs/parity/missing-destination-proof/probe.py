#!/usr/bin/env python3
"""Missing destination proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose components Copy,
Move, CleanList or DelDuplicate into folders that exist (`Override`) or not (`Nope`).
The Rust binary runs `validate --output json --dry-run` (the validation pipeline that
the desktop Validate step also shows) on a fresh copy, and the reference runs the
same command line through the pinned C# CLI (`ModSync.Core.dll validate`, d31ccbd3).

C# `ComponentValidation.ParseDestinationWithAction` (ComponentValidation.cs:246-369)
adds `Destination cannot be found! Got '<full path>'` for each such step whose
Destination folder does not exist and goes on to the next step. The pipeline
(InstallationValidationPipeline.cs:374-425) prints each error as
`ERROR: <component>: <message>` and counts every error.

Messages hold the fixture's absolute path; the probe replaces it with `<root>` on
both sides before comparing.

Three checks per case:
  decision  exit code and `success`;
  counts    `errorCount`, `warningCount`, `passedCount`;
  stage     the ComponentValidation stage: passed, hasWarnings, summary, messages.

Usage:
  probe.py [--bin target/debug/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI
is run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major;
--record also rewrites csharp-oracle.json. Without it the recorded results are
used. See ../missing-destination-contract.md.
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
KD, GD, MD = "<<kotorDirectory>>", "<<gameDirectory>>", "<<modDirectory>>"
G1 = "44444444-5555-6666-7777-888888888881"
G2 = "44444444-5555-6666-7777-888888888882"
OPTION = "44444444-5555-6666-7777-8888888888a1"


def step(action, sources, destination=None, extra=""):
    lines = ["[[{table}]]", f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def component(guid, name, steps, options=(), selected=True, option_selected=True):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}",
             f"IsSelected = {'true' if selected else 'false'}", ""]
    for s in steps:
        lines += [line.format(table="thisMod.Instructions") for line in s] + [""]
    for option_steps in options:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "Option"',
                  f"IsSelected = {'true' if option_selected else 'false'}", ""]
        for s in option_steps:
            lines += [line.format(table="thisMod.Options.Instructions") for line in s] + [""]
    return "\n".join(lines)


def copy(destination, source=MD + "/m.txt", action="Copy"):
    return step(action, [source], destination)


OK = copy(KD + "/Override")
NOPE = copy(KD + "/Nope")
NOPE2 = copy(KD + "/Nope2")
CHOOSE = step("Choose", [OPTION])
EXTRACT = step("Extract", [MD + "/a.zip"])

# name -> guide
CASES = {
    "control_override": component(G1, "A", [OK]),
    "control_case": component(G1, "A", [copy(KD + "/override")]),
    "control_extract_copy": component(G1, "A", [EXTRACT, copy(KD + "/Override", MD + "/a/x.txt")]),
    "missing": component(G1, "A", [NOPE]),
    "two_missing": component(G1, "A", [NOPE, NOPE2]),
    "missing_between_present": component(G1, "A", [OK, NOPE, OK, NOPE2]),
    "backslash_nested": component(G1, "A", [copy(KD + "\\Nope\\Deeper")]),
    "trailing_separator": component(G1, "A", [copy(KD + "/Nope/")]),
    "case_of_parent": component(G1, "A", [copy(KD + "/override/Nope")]),
    "game_directory": component(G1, "A", [copy(GD + "/Nope")]),
    "mod_directory": component(G1, "A", [copy(MD + "/Nope")]),
    "move": component(G1, "A", [copy(KD + "/Nope", action="Move")]),
    "cleanlist": component(G1, "A", [copy(KD + "/Nope", MD + "/l.csv", action="CleanList")]),
    "empty_destination": component(G1, "A", [copy("")]),
    "no_destination": component(G1, "A", [copy(None)]),
    "option_missing": component(G1, "A", [CHOOSE], options=[[NOPE]]),
    "base_and_option_missing": component(G1, "A", [NOPE, CHOOSE], options=[[NOPE2]]),
    "extract_then_copy_missing": component(G1, "A", [EXTRACT, copy(KD + "/Nope", MD + "/a/x.txt")]),
    "missing_archive_and_destination": component(G1, "A", [step("Extract", [MD + "/nope.zip"]), NOPE]),
    "two_components": component(G1, "A", [NOPE]) + "\n" + component(G2, "B", [OK, NOPE2]),
    "second_component_missing": component(G1, "A", [OK]) + "\n" + component(G2, "B", [NOPE]),
    "unselected_option_missing": component(G1, "A", [OK], options=[[NOPE]], option_selected=False),
    # Known differences (see KNOWN).
    "duplicate_destination": component(G1, "A", [NOPE, NOPE]),
    "same_folder_two_components": component(G1, "A", [NOPE]) + "\n" + component(G2, "B", [NOPE]),
    "deldup": component(G1, "A", [step("DelDuplicate", [KD + "/Override"], KD + "/Nope", 'Arguments = ".tga"')]),
    "dot_part": component(G1, "A", [copy(KD + "/Override/.")]),
    "dot_dot_part": component(G1, "A", [copy(KD + "/Override/../Nope")]),
    "member_miss_and_missing": component(G1, "A", [EXTRACT, copy(KD + "/Nope", MD + "/a/zz.txt")]),
    "missing_then_bad_patcher": component(G1, "A", [NOPE, step("Patcher", [MD + "/m.txt"], KD + "/Override")]),
}

C_ONCE = "C# creates the missing folder while validating (AttemptFixes), so a later step into it passes"
# name -> (checks that differ, the Rust ComponentValidation messages expected, reason)
KNOWN = {
    "duplicate_destination": (("counts", "stage"),
                              ["ERROR: A: Destination cannot be found! Got '<root>/game/Nope'"] * 2, C_ONCE),
    "same_folder_two_components": (("counts", "stage"),
                                   ["ERROR: A: Destination cannot be found! Got '<root>/game/Nope'",
                                    "ERROR: B: Destination cannot be found! Got '<root>/game/Nope'"], C_ONCE),
    "deldup": (("counts",), ["ERROR: A: Destination cannot be found! Got '<root>/game/Nope'"],
               C_ONCE + "; Rust's dry run also fails the DelDuplicate"),
    "dot_part": (("stage",), ["ERROR: A: Destination cannot be found! Got '<root>/game/Override'"],
                 "Rust shows the resolved folder without the `.` part"),
    "dot_dot_part": (("counts", "stage"),
                     ["ERROR: A: archive/destination validation: Unsafe path: Override/../Nope"],
                     "Rust refuses a `..` part (fail closed)"),
    "member_miss_and_missing": (("counts", "stage"),
                                ["ERROR: A: archive/destination validation: Failed to find "
                                 "'<<modDirectory>>/a/zz.txt' in any archives"],
                                "a member miss hides the Destination check in both; the member "
                                "message keeps its Rust wording, and C# adds a second for its retry"),
    "missing_then_bad_patcher": (("counts", "stage"),
                                 ["ERROR: A: archive/destination validation: Patcher destination must be the "
                                  "game directory"],
                                 "another destination failure ends the component's checks with one error"),
}
CHECKS = ("decision", "counts", "stage")


def fixture(root, guide):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    with open(os.path.join(root, "game", "chitin.key"), "w") as handle:
        handle.write("KEY\n")
    with open(os.path.join(root, "game", "r.txt"), "w") as handle:
        handle.write("r\n")
    with open(os.path.join(root, "mods", "m.txt"), "w") as handle:
        handle.write("m\n")
    with open(os.path.join(root, "mods", "l.csv"), "w") as handle:
        handle.write("Mandatory Deletions,r.txt\n")
    with zipfile.ZipFile(os.path.join(root, "mods", "a.zip"), "w") as archive:
        archive.writestr("x.txt", "x\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def view(code, report, root):
    stage = next((s for s in report.get("stages", []) if s["stage"] == "ComponentValidation"), None)
    messages = [] if stage is None else [m.replace(root, "<root>") for m in stage.get("messages") or []]
    return {
        "decision": [code, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stage": None if stage is None else [stage["passed"], stage["hasWarnings"], stage.get("summary"), messages],
    }


def run(argv_head, guide, work, label, extra_env=None):
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, guide)
    argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600)
    report = json.loads(done.stdout[done.stdout.index("{"):])
    return view(done.returncode, report, root)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="missing-destination-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, guide, work, "csharp", env) for name, guide in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, guide in CASES.items():
            actual = run([args.bin], guide, work, "rust")
            expected = oracle[name]
            for check in CHECKS:
                same = actual[check] == expected[check]
                if name in KNOWN and check in KNOWN[name][0]:
                    rust_messages = KNOWN[name][1]
                    ok = not same and actual["stage"] is not None and actual["stage"][3] == rust_messages
                    verdict = "known" if ok else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                detail = "" if verdict == "match" else f" | rust={actual[check]} | reference={expected[check]}"
                print(f"{verdict:5} {name}/{check}{detail}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
