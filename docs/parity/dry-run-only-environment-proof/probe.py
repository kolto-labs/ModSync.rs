#!/usr/bin/env python3
"""Dry-run-only environment proof: drives the production `modsync`.

`validate --full --dry-run-only` skips the ComponentValidation stage in both
implementations (InstallationValidationPipeline.cs:127, :251), but C#'s Environment
stage still runs `ComponentValidation` (with AttemptFixes) on every selected
component (InstallationService.cs:967-1012) and fails on any error with `Some
components failed to validate. ...` (:1051-1056), ending the report
(InstallationValidationPipeline.cs:66-82). Rust's Environment stage checked only
the recovery state, so the report passed (exit 0) a guide whose archive was
missing, where C# fails it.

Each case is a disposable mod/game fixture with one TOML guide. The mod folder holds
`a/x.txt`, `a/y.tga`, the game `chitin.key` and `Override/g.txt`; some cases add
`m1.zip` (`f1.txt`) or a patcher folder `p/`. Both sides run, on a fresh copy each:

  full_only  `validate -i G -s mods -g game --output json --full --dry-run-only`;
  text       `validate -i G -s mods -g game --full --dry-run-only` (text output);
  only       `validate ... --output json --dry-run-only` (no `--full`; a control).

Checks:
  full_only/decision, only/decision  exit code is zero or not, and `success`;
  full_only/counts, only/counts      `errorCount`, `warningCount`, `passedCount`;
  full_only/stages                   each stage's name and `passed`;
  full_only/environment              the Environment stage's first message;
  text/decision                      exit code is zero or not;
  details             (Rust only) the lines Rust lists after that message: each
                      failing component's errors (C# writes them only to its log).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy holding a HoloPatcher stub (C#'s Environment stage runs
`holopatcher --install` and expects exit code 2) under `unshare -Urnm` (no network, a
tmpfs over `/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also
rewrites csharp-oracle.json. Without it the recorded results are used. See
../dry-run-only-environment-contract.md.
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
GA = "55555555-6666-7777-8888-999999999901"
GB = "55555555-6666-7777-8888-999999999902"
GO = "55555555-6666-7777-8888-999999999911"
OV = KD + "/Override"

STUB = "#!/bin/sh\nexit 2\n"


def step(action, sources, destination=None):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    return lines


def component(guid, name, steps, selected=True, options=()):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f'Name = "{name}"',
             f"IsSelected = {'true' if selected else 'false'}", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    return "\n".join(lines + list(options))


def option(steps):
    lines = ["[[thisMod.Options]]", f'Guid = "{GO}"', 'Name = "O"', "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return lines


B_OK = component(GB, "B", [step("Copy", [MD + "/a/y.tga"], OV)])
MISSING = [step("Extract", [MD + "/m1.zip"]), step("Copy", [MD + "/m1/f1.txt"], OV)]
ZIP = {"mods/m1.zip": {"f1.txt": "z1\n"}}
CHANGES = ("[Settings]\nLookupGameFolder=0\n\n[InstallList]\ninstall_folder0=Override\n\n"
           "[install_folder0]\nFile0=n.tga\n")
PATCHER = {"mods/p/TSLPatcher.exe": "MZ\n", "mods/p/tslpatchdata/changes.ini": CHANGES,
           "mods/p/tslpatchdata/n.tga": "n tga\n"}

# name -> (extra files, guide, extra flags, Rust's detail lines after C#'s message)
CASES = {
    # Fixed: C# fails its Environment stage; Rust passed.
    "missing_archive": ({}, component(GA, "A", MISSING), [],
                        ["ERROR: A: Missing Required Archives: [<<modDirectory>>/m1.zip]"]),
    "missing_archive_second": ({}, B_OK + "\n" + component(GA, "A", MISSING), [],
                               ["ERROR: A: Missing Required Archives: [<<modDirectory>>/m1.zip]"]),
    "missing_archive_unselected": ({}, B_OK + "\n" + component(GA, "A", MISSING, selected=False), [],
                                   ["ERROR: A: Missing Required Archives: [<<modDirectory>>/m1.zip]"]),
    "loose_not_in_archive": (ZIP, component(GA, "A", [step("Extract", [MD + "/m1.zip"]),
                                                      step("Copy", [MD + "/m1/nope.txt"], OV)]), [],
                             ["ERROR: A: archive/destination validation: Failed to find "
                              "'<<modDirectory>>/m1/nope.txt' in any archives"]),
    # Fixed stage: both fail; Rust failed later (its dry run), now in Environment.
    "destination_file": ({}, component(GA, "A", [step("Copy", [MD + "/a/x.txt"], OV + "/g.txt/sub")]), [],
                         ["ERROR: A: archive/destination validation: Not a directory (os error 20)"]),
    "destination_empty": ({}, component(GA, "A", [step("Copy", [MD + "/a/x.txt"], "")]), [],
                          ["ERROR: A: Destination cannot be found! Got ''"]),
    # Controls: unchanged.
    "ok": ({}, component(GA, "A", [step("Copy", [MD + "/a/x.txt"], OV)]), [], []),
    "file_selection_unselected": ({}, B_OK + "\n" + component(GA, "A", MISSING, selected=False),
                                  ["--use-file-selection"], []),
    "destination_missing_folder": ({}, component(GA, "A", [step("Copy", [MD + "/a/x.txt"], KD + "/Nope")]),
                                   [], []),
    "patcher_no_destination": (PATCHER, component(GA, "A", [step("Patcher", [MD + "/p/TSLPatcher.exe"])]),
                               [], []),
    "extract_destination": (ZIP, component(GA, "A", [step("Extract", [MD + "/m1.zip"], OV)]), [], []),
    "missing_copy_source": ({}, component(GA, "A", [step("Copy", [MD + "/a/nope.txt"], OV)]), [], []),
    # Known difference: a partly missing Extract fails Rust's Environment stage.
    "partial_missing_extract": (ZIP, component(GA, "A", [step("Extract", [MD + "/m1.zip", MD + "/m2.zip"]),
                                                         step("Copy", [MD + "/m1/f1.txt"], OV)]), [],
                                ["ERROR: A: Missing Required Archives: "
                                 "[<<modDirectory>>/m1.zip,<<modDirectory>>/m2.zip]"]),
    # Known difference: an option's missing archive fails Rust's Environment stage.
    "missing_archive_option": ({}, component(GA, "A", [step("Choose", [GO])],
                                             options=option([step("Extract", [MD + "/m1.zip"])])), [],
                               ["ERROR: A: Missing Required Archives: [<<modDirectory>>/m1.zip]"]),
}

# name -> {check: reason}
KNOWN = {
    "missing_archive_option": {
        "full_only/stages": "Rust's Environment stage fails an option's missing archive "
                            "(fail closed); C# fails it later, in its dry run",
        "full_only/environment": "as full_only/stages",
        "full_only/counts": "as full_only/stages: C# also counts its four passed stages",
    },
    "partial_missing_extract": {
        "full_only/decision": "Rust refuses an Extract with one archive missing (fail closed); "
                              "C# skips the check when any of its archives exists",
        "full_only/counts": "as full_only/decision",
        "full_only/stages": "as full_only/decision",
        "full_only/environment": "as full_only/decision",
        "text/decision": "as full_only/decision",
        "only/counts": "unchanged: without --full both pass; Rust's dry run also warns "
                       "that it skipped the component",
    },
    "destination_file": {
        "only/decision": "unchanged: Rust's dry run refuses a Destination below a file "
                         "(fail closed); C#'s virtual dry run passes it",
        "only/counts": "as only/decision",
    },
}

FAILED = ("ERROR: Some components failed to validate. Check the output/console window for "
          "details.\nIf you are seeing this as an end user you most likely need to whitelist "
          "ModSync and HoloPatcher in your antivirus, or download the missing mods.")


def known_ok(name, check, actual):
    """The Rust side of each known difference is the documented one."""
    if check == "full_only/stages":
        return actual[check] == [["Environment", False]]
    if check == "full_only/environment":
        return actual[check] == FAILED
    if check == "full_only/counts":
        return actual[check] == [1, 0, 0]
    if check == "full_only/decision":
        return actual[check] == [False, False]
    if check == "text/decision":
        return actual[check] is False
    if check == "only/decision":
        return actual[check] == [False, False]
    if check == "only/counts":
        return actual[check] == ([0, 1, 1] if name == "partial_missing_extract" else [1, 0, 1])
    return False


BASE = {"game/chitin.key": "KEY\n", "game/Override/g.txt": "game g\n", "mods/a/x.txt": "ax\n",
        "mods/a/y.tga": "ay\n"}


def fixture(root, files, text):
    os.makedirs(os.path.join(root, "home"))
    for name, content in dict(BASE, **files).items():
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if isinstance(content, dict):
            with zipfile.ZipFile(path, "w") as archive:
                for member, data in content.items():
                    archive.writestr(member, data)
        else:
            with open(path, "w") as handle:
                handle.write(content)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(text)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def run_one(argv_head, case, work, label, mode, extra_env):
    files, text, flags, _ = case
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, files, text)
    argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game] + flags
    argv += {"full_only": ["--output", "json", "--full", "--dry-run-only"],
             "text": ["--full", "--dry-run-only"],
             "only": ["--output", "json", "--dry-run-only"]}[mode]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "text":
        return {"text/decision": done.returncode == 0}, []
    out = done.stdout
    report = json.loads(out[out.index("{"):].replace(json.dumps(root)[1:-1], "<root>"))
    view = {
        f"{mode}/decision": [done.returncode == 0, report.get("success")],
        f"{mode}/counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
    }
    details = []
    if mode == "full_only":
        stages = report.get("stages", [])
        view["full_only/stages"] = [[s.get("stage"), s.get("passed")] for s in stages]
        environment = next((s for s in stages if s.get("stage") == "Environment"), {})
        messages = environment.get("messages") or []
        view["full_only/environment"] = messages[0] if messages else None
        details = messages[1:]
    return view, details


def run_case(argv_head, case, work, label, extra_env=None):
    out, details = {}, []
    for mode in ("full_only", "text", "only"):
        view, found = run_one(argv_head, case, work, label, mode, extra_env)
        out.update(view)
        details += found
    return out, details


def stub_reference(reference, work):
    """A copy of the reference build with a HoloPatcher stub in its Resources."""
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    stub = os.path.join(copy_dir, "Resources", "holopatcher")
    with open(stub, "w") as handle:
        handle.write(STUB)
    os.chmod(stub, 0o755)
    return copy_dir


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="dry-run-only-environment-proof-",
                            dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, case, work, "csharp", env)[0] for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual, details = run_case([os.path.abspath(args.bin)], case, work, "rust")
            expected = oracle[name]
            for check in sorted(expected) + ["details"]:
                if check == "details":
                    same = details == case[3]
                    detail = f" | rust={details} | expected={case[3]}"
                else:
                    same = actual[check] == expected[check]
                    detail = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    verdict = "known" if not same and known_ok(name, check, actual) else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                print(f"{verdict:5} {name}/{check}{detail if verdict == 'FAIL' else ''}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
