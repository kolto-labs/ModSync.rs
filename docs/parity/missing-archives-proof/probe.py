#!/usr/bin/env python3
"""Missing archives proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose components hold
Extract steps for archives that exist (`a.zip`) or not (`nope.zip`, `nope2.zip`,
`*.7z`), in the component's own steps or in an option. The Rust binary runs
`validate --output json --dry-run` (the validation pipeline that the desktop
Validate step also shows) on a fresh copy, and the reference runs the same command
line through the pinned C# CLI (`ModSync.Core.dll validate`, d31ccbd3).

C# `ComponentValidation.VerifyExtractPaths` (ComponentValidation.cs:70-85) collects
the component's archives; when none exists, it reports each top-level Extract as
`Missing Required Archives: [<Sources joined by ",">]` and skips the component's
other checks. The pipeline (InstallationValidationPipeline.cs:373-423) prints each
error as `ERROR: <component>: <message>` and counts every error.

Three checks per case:
  decision  exit code and `success`;
  counts    `errorCount`, `warningCount`, `passedCount`;
  stage     the ComponentValidation stage: passed, hasWarnings, summary, messages.

Usage:
  probe.py [--bin target/debug/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI
is run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major;
--record also rewrites csharp-oracle.json. Without it the recorded results are
used. See ../missing-archives-contract.md.
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
KD, MD = "<<kotorDirectory>>", "<<modDirectory>>"
G1 = "33333333-4444-5555-6666-777777777771"
G2 = "33333333-4444-5555-6666-777777777772"
OPTION = "33333333-4444-5555-6666-7777777777a1"


def step(action, sources, destination=None, extra=""):
    lines = ["[[{table}]]", f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def component(guid, name, steps, options=(), selected=True):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}",
             f"IsSelected = {'true' if selected else 'false'}", ""]
    for s in steps:
        lines += [line.format(table="thisMod.Instructions") for line in s] + [""]
    for option_steps in options:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "Option"', "IsSelected = true", ""]
        for s in option_steps:
            lines += [line.format(table="thisMod.Options.Instructions") for line in s] + [""]
    return "\n".join(lines)


OK = step("Extract", [MD + "/a.zip"])
MISS = step("Extract", [MD + "/nope.zip"])
MISS2 = step("Extract", [MD + "/nope2.zip"])
COPY = step("Copy", [MD + "/m.txt"], KD + "/Override")
CHOOSE = step("Choose", [OPTION])
IF_B = f'Dependencies = ["{G2}"]'
B_OFF = component(G2, "B", [COPY], selected=False)

# name -> (guide, extra flags)
CASES = {
    "control_copy": (component(G1, "A", [COPY]), []),
    "control_extract": (component(G1, "A", [OK, step("Copy", [MD + "/a/x.txt"], KD + "/Override")]), []),
    "control_empty_source_and_present": (component(G1, "A", [step("Extract", []), OK]), []),
    "missing": (component(G1, "A", [MISS]), []),
    "two_missing": (component(G1, "A", [MISS, MISS2]), []),
    "two_sources_missing": (component(G1, "A", [step("Extract", [MD + "/nope.zip", MD + "/nope2.zip"])]), []),
    "wildcard_missing": (component(G1, "A", [step("Extract", [MD + "/*.7z"])]), []),
    "backslash_missing": (component(G1, "A", [step("Extract", ["<<modDirectory>>\\nope.zip"])]), []),
    "duplicate_missing": (component(G1, "A", [MISS, MISS]), []),
    "missing_then_copy": (component(G1, "A", [MISS, step("Copy", [MD + "/nope/x.txt"], KD + "/Override")]), []),
    "missing_bad_destination": (component(G1, "A", [MISS, step("Copy", [MD + "/m.txt"], KD + "/NoDir")]), []),
    "base_and_option_missing": (component(G1, "A", [MISS, CHOOSE], options=[[MISS2]]), []),
    "second_component_missing": (component(G1, "A", [OK]) + "\n" + component(G2, "B", [MISS]), []),
    "two_components_missing": (component(G1, "A", [MISS]) + "\n" + component(G2, "B", [MISS2, MISS]), []),
    "unselected_missing": (component(G1, "A", [OK]) + "\n" + component(G2, "B", [MISS], selected=False),
                           ["--use-file-selection"]),
    # Known differences (see KNOWN).
    "partly_missing": (component(G1, "A", [OK, MISS]), []),
    "partly_missing_sources": (component(G1, "A", [step("Extract", [MD + "/a.zip", MD + "/nope.zip"])]), []),
    "option_only_missing": (component(G1, "A", [CHOOSE], options=[[MISS]]), []),
    "option_present_base_missing": (component(G1, "A", [MISS, CHOOSE], options=[[OK]]), []),
    "empty_source_only": (component(G1, "A", [step("Extract", [])]), []),
    "conditional_present": (component(G1, "A", [step("Extract", [MD + "/a.zip"], extra=IF_B)]) + "\n" + B_OFF,
                            ["--use-file-selection"]),
    "conditional_missing": (component(G1, "A", [step("Extract", [MD + "/nope.zip"], extra=IF_B)]) + "\n" + B_OFF,
                            ["--use-file-selection"]),
    "missing_and_conditional_missing": (
        component(G1, "A", [MISS2, step("Extract", [MD + "/nope.zip"], extra=IF_B)]) + "\n" + B_OFF,
        ["--use-file-selection"]),
}

FAIL_CLOSED = "Rust refuses an Extract whose archive is missing even when C# reports nothing in this stage"
# name -> (checks that differ, the Rust ComponentValidation messages expected, reason)
KNOWN = {
    "partly_missing": (("decision", "counts", "stage"),
                       ["ERROR: A: Missing Required Archives: [<<modDirectory>>/nope.zip]"], FAIL_CLOSED),
    "partly_missing_sources": (("decision", "counts", "stage"),
                               ["ERROR: A: Missing Required Archives: [<<modDirectory>>/a.zip,<<modDirectory>>/nope.zip]"],
                               FAIL_CLOSED),
    "option_only_missing": (("counts", "stage"),
                            ["ERROR: A: Missing Required Archives: [<<modDirectory>>/nope.zip]"], FAIL_CLOSED),
    "option_present_base_missing": (("decision", "counts", "stage"),
                                    ["ERROR: A: Missing Required Archives: [<<modDirectory>>/nope.zip]"], FAIL_CLOSED),
    "conditional_present": (("decision", "counts", "stage"), ["OK: A"],
                            "C# reports a top-level Extract that does not run; Rust checks only steps that run"),
    "conditional_missing": (("decision", "counts", "stage"), ["OK: A"],
                            "C# reports a top-level Extract that does not run; Rust checks only steps that run"),
    "missing_and_conditional_missing": (("counts", "stage"),
                                        ["ERROR: A: Missing Required Archives: [<<modDirectory>>/nope2.zip]"],
                                        "C# also reports the Extract that does not run"),
}
CHECKS = ("decision", "counts", "stage")


def fixture(root, guide):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    with open(os.path.join(root, "game", "chitin.key"), "w") as handle:
        handle.write("KEY\n")
    with open(os.path.join(root, "mods", "m.txt"), "w") as handle:
        handle.write("m\n")
    with zipfile.ZipFile(os.path.join(root, "mods", "a.zip"), "w") as archive:
        archive.writestr("x.txt", "x\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def view(code, report):
    stage = next((s for s in report.get("stages", []) if s["stage"] == "ComponentValidation"), None)
    return {
        "decision": [code, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stage": None if stage is None else [stage["passed"], stage["hasWarnings"], stage.get("summary"),
                                             stage.get("messages") or []],
    }


def run(argv_head, guide, flags, work, label, extra_env=None):
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, guide)
    argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600)
    report = json.loads(done.stdout[done.stdout.index("{"):])
    return view(done.returncode, report)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="missing-archives-proof-")
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, guide, flags, work, "csharp", env) for name, (guide, flags) in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, (guide, flags) in CASES.items():
            actual = run([args.bin], guide, flags, work, "rust")
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
