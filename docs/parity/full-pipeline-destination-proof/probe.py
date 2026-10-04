#!/usr/bin/env python3
"""Full pipeline destination proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose components Copy,
Move, CleanList or DelDuplicate into folders that exist (`Override`) or not (`Nope`).
Both sides run, on a fresh copy each, with `--use-file-selection`:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`
            (C# `ValidationPipelineOptions.WizardFull`, the wizard's Validate page and
            the install preflight; Rust's desktop Validate step);
  full      `validate ... --output json --full` (no dry run);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` (it asks
            for confirmation) but not `--skip-validation`, so it validates first;
  basic     `validate ... --output json --dry-run` (no `--full`), for some cases.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder
(`InstallationService.ValidateInstallationEnvironmentAsync`,
Services/InstallationService.cs:794-1072). The probe copies the reference build into
its temporary directory and adds `Resources/holopatcher`, a shell script that exits
2, which is what the stage expects from `holopatcher --install` without paths. That
stage then runs `ComponentValidation` for every selected component with AttemptFixes
on (:967-1010), which creates each missing Destination folder it can
(ComponentValidation.cs:331-365); the later stages find the folders.

Checks:
  full_dry/decision  exit code is zero or not, and `success`;
  full_dry/counts    `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages    each stage's name and `passed`, and the ComponentValidation and
                     DryRun stages' messages (the fixture root as `<root>`);
  full/decision, full/counts  as above, without a dry run;
  install            exit code is zero or not, and the files under `game/` and `mods/`
                     afterwards (C#'s `.modsync` folder left out);
  modes              (Rust only) plain `validate` decides as the full_dry report;
  basic/decision     for the cases that have it.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../full-pipeline-destination-contract.md.
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
G1 = "55555555-6666-7777-8888-999999999991"
G2 = "55555555-6666-7777-8888-999999999992"
OPTION = "55555555-6666-7777-8888-9999999999a1"


def step(action, sources, destination=None, extra=""):
    lines = ["[[{table}]]", f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def component(guid, name, steps, options=(), option_selected=True):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true", ""]
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
CHOOSE = step("Choose", [OPTION])
EXTRACT = step("Extract", [MD + "/a.zip"])

# name -> (guide, fixture without game/Override, also run the basic mode)
CASES = {
    "control_override": (component(G1, "A", [OK]), False, True),
    "missing": (component(G1, "A", [NOPE]), False, True),
    "override_absent": (component(G1, "A", [OK]), True, True),
    "nested": (component(G1, "A", [copy(KD + "\\Nope\\Deeper")]), False, False),
    "case_of_parent": (component(G1, "A", [copy(KD + "/override/Nope")]), False, False),
    "game_directory": (component(G1, "A", [copy(GD + "/Nope")]), False, False),
    "mod_directory": (component(G1, "A", [copy(MD + "/Nope")]), False, False),
    "move": (component(G1, "A", [copy(KD + "/Nope", action="Move")]), False, False),
    "cleanlist": (component(G1, "A", [copy(KD + "/Nope", MD + "/l.csv", action="CleanList")]), False, False),
    "two_steps_same_folder": (component(G1, "A", [NOPE, copy(KD + "/Nope", MD + "/n.txt")]), False, True),
    "two_components_same_folder": (component(G1, "A", [NOPE]) + "\n"
                                   + component(G2, "B", [copy(KD + "/Nope", MD + "/n.txt")]), False, False),
    "extract_then_copy": (component(G1, "A", [EXTRACT, copy(KD + "/Nope", MD + "/a/x.txt")]), False, True),
    "option_missing": (component(G1, "A", [CHOOSE], options=[[NOPE]]), False, False),
    "unselected_option_missing": (component(G1, "A", [OK], options=[[NOPE]], option_selected=False), False, False),
    # Still refused in both (the decision matches; see KNOWN for the stage).
    "empty_destination": (component(G1, "A", [copy("")]), False, False),
    "dot_part": (component(G1, "A", [copy(KD + "/Nope/.")]), False, False),
    "file_in_the_way": (component(G1, "A", [copy(KD + "/r.txt/Nope")]), False, False),
    "missing_archive": (component(G1, "A", [step("Extract", [MD + "/nope.zip"]), NOPE]), False, False),
    # Known difference (see KNOWN).
    "deldup": (component(G1, "A", [step("DelDuplicate", [KD + "/Override"], KD + "/Nope",
                                        'Arguments = ".tga"')]), False, False),
}

ROOT_NOPE = "<root>/game/Nope"
C_ENV = ("C# fails these in its Environment stage (`Some components failed to validate`) "
         "and stops; Rust fails its archive stage")
# name -> {check: (expected Rust value or None for "anything but the reference", reason)}
KNOWN = {
    "empty_destination": {"full_dry/counts": C_ENV, "full_dry/stages": C_ENV, "full/counts": C_ENV},
    "dot_part": {"full_dry/stages": "Rust shows the resolved folder without the `.` part"},
    "file_in_the_way": {"full_dry/counts": C_ENV, "full_dry/stages": C_ENV, "full/counts": C_ENV},
    "missing_archive": {"full_dry/counts": C_ENV, "full_dry/stages": C_ENV, "full/counts": C_ENV},
    "deldup": {
        "full_dry/decision": "Rust still refuses a DelDuplicate into a missing folder",
        "full_dry/counts": "Rust still refuses a DelDuplicate into a missing folder",
        "full_dry/stages": "Rust still refuses a DelDuplicate into a missing folder",
        "full/decision": "Rust still refuses a DelDuplicate into a missing folder",
        "full/counts": "Rust still refuses a DelDuplicate into a missing folder",
        "install": "Rust refuses before any write; C# creates the folder and installs",
    },
}
# The Rust ComponentValidation messages a known case must show in full_dry.
KNOWN_RUST_MESSAGES = {
    "empty_destination": ["ERROR: A: Destination cannot be found! Got ''"],
    "dot_part": [f"ERROR: A: Destination cannot be found! Got '{ROOT_NOPE}'"],
    "file_in_the_way": ["ERROR: A: archive/destination validation: Not a directory (os error 20)"],
    "missing_archive": ["ERROR: A: Missing Required Archives: [<<modDirectory>>/nope.zip]"],
    "deldup": [f"ERROR: A: Destination cannot be found! Got '{ROOT_NOPE}'"],
}


def fixture(root, guide, no_override):
    os.makedirs(os.path.join(root, "game"))
    if not no_override:
        os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    files = {"game/chitin.key": "KEY\n", "game/r.txt": "r\n", "mods/m.txt": "m\n", "mods/n.txt": "n\n",
             "mods/l.csv": "Mandatory Deletions,r.txt\n"}
    for name, text in files.items():
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    with zipfile.ZipFile(os.path.join(root, "mods", "a.zip"), "w") as archive:
        archive.writestr("x.txt", "x\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    out = []
    for folder in ("game", "mods"):
        base = os.path.join(root, folder)
        for current, dirs, files in os.walk(base):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                out.append(os.path.relpath(os.path.join(current, name), root).replace(os.sep, "/"))
    return sorted(out)


def report_view(code, stdout, root):
    report = json.loads(stdout[stdout.index("{"):])
    stages = []
    for stage in report.get("stages") or []:
        messages = None
        if stage["stage"] in ("ComponentValidation", "DryRun"):
            messages = [m.replace(root, "<root>") for m in stage.get("messages") or []]
        stages.append([stage["stage"], stage["passed"], messages])
    return {
        "decision": [code == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stages": stages,
    }


def run_one(argv_head, guide, no_override, work, label, mode, csharp, extra_env):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, guide, no_override)
    common = ["-i", path, "-s", mods, "-g", game, "--use-file-selection"]
    if mode == "install":
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "full": ["--full"], "basic": ["--dry-run"]}[mode]
        argv = argv_head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "install":
        return [done.returncode == 0, tree(root)]
    if mode == "plain":
        return done.returncode == 0
    return report_view(done.returncode, done.stdout, root)


def run_case(argv_head, case, work, label, csharp, extra_env=None):
    guide, no_override, basic = case
    out = {}
    for mode in ("full_dry", "full", "install") + (("basic",) if basic else ()) + (() if csharp else ("plain",)):
        result = run_one(argv_head, guide, no_override, work, label, mode, csharp, extra_env)
        if mode in ("full_dry", "full", "basic"):
            for key, value in result.items():
                if mode == "full" and key == "stages" or mode == "basic" and key != "decision":
                    continue
                out[f"{mode}/{key}"] = value
        else:
            out[mode] = result
    return out


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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="full-pipeline-destination-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, case, work, "csharp", True, env) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run_case([args.bin], case, work, "rust", False)
            expected = oracle[name]
            checks = list(expected)
            checks.append("modes")
            for check in checks:
                if check == "modes":
                    same = actual["plain"] == actual["full_dry/decision"][0]
                    detail = "" if same else f" | plain={actual['plain']} | report={actual['full_dry/decision']}"
                else:
                    same = actual[check] == expected[check]
                    detail = "" if same else f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    stages = actual["full_dry/stages"]
                    component = next((s[2] for s in stages if s[0] == "ComponentValidation"), None)
                    wanted = KNOWN_RUST_MESSAGES.get(name)
                    ok = not same and (wanted is None or component == wanted)
                    verdict = "known" if ok else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                print(f"{verdict:5} {name}/{check}{detail if verdict != 'match' else ''}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
