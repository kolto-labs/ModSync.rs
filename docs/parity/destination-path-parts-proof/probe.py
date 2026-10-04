#!/usr/bin/env python3
"""Destination path parts proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose component Copies,
Moves, CleanLists or DelDuplicates into a Destination with a part C#
`PathValidator.IsValidPath` refuses (a `.` part, `..` inside a name, a control
character) or a control Destination it accepts. Both sides run, on a fresh copy
each, with `--use-file-selection`:

  full_dry      `validate -i G -s mods -g game --output json --full --dry-run`
                (C# `WizardFull`, the wizard's Validate page and install preflight;
                Rust's desktop Validate step);
  full          the same with `--full` only;
  basic         `validate ... --output json --dry-run` (no `--full`);
  install       `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but
                not `--skip-validation`, so it validates first;
  install_skip  (C# only) `install ... -y --skip-validation`, which still runs
                `ComponentValidation` per component before installing it;
  plain         (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, full/decision, basic/decision  exit code is zero or not, and
                     `success`;
  full_dry/counts    `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages    each stage's name and `passed`, and the ComponentValidation and
                     DryRun stages' messages (the fixture root as `<root>`);
  install            exit code is zero or not, and the files under `game/` and `mods/`
                     afterwards (C#'s `.modsync` folder left out);
  install_skip       Rust `install` (Rust has no `--skip-validation`) against C#
                     `install --skip-validation`, as `install`;
  plain              Rust's plain `validate` decides as C# `install`.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder;
the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2 (see
../full-pipeline-destination-proof/README.md).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../destination-path-parts-contract.md.
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
KD, MD = "<<kotorDirectory>>", "<<modDirectory>>"
G1 = "66666666-7777-8888-9999-aaaaaaaaaaa1"
G2 = "66666666-7777-8888-9999-aaaaaaaaaaa2"
OPTION = "66666666-7777-8888-9999-aaaaaaaaaab1"


def step(action, sources, destination=None, extra=""):
    lines = ["[[{table}]]", f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def component(guid, name, steps, options=(), option_selected=True, selected=True):
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


def copy(destination, source=MD + "/m.txt", action="Copy", extra=""):
    return step(action, [source], destination, extra)


OK = copy(KD + "/Override")
DOT = copy(KD + "/Override/.")
CHOOSE = step("Choose", [OPTION])
TGA = 'Arguments = ".tga"'

# name -> (guide, extra game folder holding r.txt, x.tga and x.tpc, or None)
CASES = {
    # Controls: accepted by both.
    "control_override": (component(G1, "A", [OK]), None),
    "control_dot_name": (component(G1, "A", [copy(KD + "/Override/.x")]), None),
    "control_dotted_name": (component(G1, "A", [copy(KD + "/Override/a.b")]), None),
    "control_unselected_component": (component(G1, "A", [OK]) + "\n"
                                     + component(G2, "B", [DOT], selected=False), None),
    # A `.` part.
    "dot_override": (component(G1, "A", [DOT]), None),
    "dot_missing": (component(G1, "A", [copy(KD + "/Nope/.")]), None),
    "dot_game_root": (component(G1, "A", [copy(KD + "/.")]), None),
    "dot_middle": (component(G1, "A", [copy(KD + "/./Override")]), None),
    "dot_backslash": (component(G1, "A", [copy(KD + "\\Override\\.")]), None),
    "dot_mod_root": (component(G1, "A", [copy(MD + "/.", KD + "/r.txt")]), None),
    "dot_move": (component(G1, "A", [copy(KD + "/Override/.", action="Move")]), None),
    "dot_selected_option": (component(G1, "A", [CHOOSE], options=[[DOT]]), None),
    "dot_unselected_option": (component(G1, "A", [OK], options=[[DOT]], option_selected=False), None),
    "dot_condition_unmet": (component(G1, "A", [OK, copy(KD + "/Override/.", extra=f'Dependencies = ["{G2}"]')])
                            + "\n" + component(G2, "B", [OK], selected=False), None),
    "dot_after_good_step": (component(G1, "A", [OK, copy(KD + "/Override/.", MD + "/n.txt")]), None),
    "dot_second_component": (component(G1, "A", [OK]) + "\n"
                             + component(G2, "B", [copy(KD + "/Override/.", MD + "/n.txt")]), None),
    # `..` inside a name.
    "dotdot_existing": (component(G1, "A", [copy(KD + "/a..b")]), "a..b"),
    "dotdot_missing": (component(G1, "A", [copy(KD + "/Over..ride")]), None),
    "dotdot_leading": (component(G1, "A", [copy(KD + "/Override/..x")]), None),
    "dotdot_cleanlist": (component(G1, "A", [copy(KD + "/a..b", MD + "/l.csv", action="CleanList")]), "a..b"),
    "dotdot_deldup": (component(G1, "A", [step("DelDuplicate", [], KD + "/a..b", TGA)]), "a..b"),
    # A control character.
    "tab_missing": (component(G1, "A", [copy(KD + "/Override/a\tb")]), None),
    "tab_existing": (component(G1, "A", [copy(KD + "/t\tb")]), "t\tb"),
    # Already refused by both before this slice.
    "dot_cleanlist": (component(G1, "A", [copy(KD + "/Override/.", MD + "/l.csv", action="CleanList")]), None),
    "dot_deldup": (component(G1, "A", [step("DelDuplicate", [], KD + "/Override/.", TGA)]), None),
}

C_ENV = ("C# runs ComponentValidation in its Environment stage, which fails on the part and ends the "
         "report there; Rust's Environment stage passes and its ComponentValidation stage fails")
SHOWN = "Rust shows the resolved folder (without a `.` part); C# fails its Environment stage first"
SKIP = ("C# `install --skip-validation` copies or moves into the folder; Rust has no "
        "`--skip-validation` and its install refuses before any write")
UNMET = ("Rust's archive stage, plain validate and install leave out a step whose instruction "
         "conditions are not met (it never runs); C# validates every step")
REFUSED = [f"dot_{n}" for n in ("override", "game_root", "middle", "backslash", "mod_root", "move",
                                "selected_option", "unselected_option", "after_good_step",
                                "second_component", "cleanlist", "deldup")] + [
    "dotdot_existing", "dotdot_missing", "dotdot_leading", "dotdot_cleanlist", "dotdot_deldup",
    "tab_missing", "tab_existing"]
# name -> {check: reason}. A known check passes only if Rust differs from the reference
# and shows the expected Rust result (see known_ok).
KNOWN = {name: {"full_dry/counts": C_ENV, "full_dry/stages": SHOWN} for name in REFUSED}
KNOWN["dot_missing"] = {"full_dry/stages": "Rust shows the resolved folder without the `.` part"}
for _name in REFUSED + ["dot_missing"]:
    if not _name.endswith(("cleanlist", "deldup")):
        KNOWN[_name]["install_skip"] = SKIP
KNOWN["dot_condition_unmet"] = {check: UNMET for check in (
    "full_dry/decision", "full_dry/counts", "full_dry/stages", "full/decision", "basic/decision",
    "install", "plain")}


def known_ok(name, check, actual):
    """The Rust side of a known difference is the documented one."""
    if name == "dot_condition_unmet":
        return actual["plain"] and actual["install"][0] and actual["full_dry/decision"][0]
    if check == "install_skip":
        return not actual["install"][0]
    if check == "full_dry/stages":
        component = next(s for s in actual["full_dry/stages"] if s[0] == "ComponentValidation")
        return not component[1] and any("Destination cannot be found! Got '<root>/" in m for m in component[2])
    return True


def fixture(root, guide, extra_folder):
    for folder in ("game/Override", "mods", "home"):
        os.makedirs(os.path.join(root, folder))
    files = {"game/chitin.key": "KEY\n", "game/r.txt": "r\n", "game/Override/x.tga": "XTGA\n",
             "game/Override/x.tpc": "XTPC\n", "mods/m.txt": "m\n", "mods/n.txt": "n\n",
             "mods/l.csv": "Mandatory Deletions,r.txt\n"}
    if extra_folder:
        for name in ("r.txt", "x.tga", "x.tpc"):
            files[f"game/{extra_folder}/{name}"] = name + "\n"
    for name, text in files.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
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


def run_one(argv_head, guide, extra_folder, work, label, mode, csharp, extra_env):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, guide, extra_folder)
    common = ["-i", path, "-s", mods, "-g", game, "--use-file-selection"]
    if mode in ("install", "install_skip"):
        argv = argv_head + ["install"] + common + ["--no-checkpoint"]
        if csharp:
            argv += ["-y"] + (["--skip-validation"] if mode == "install_skip" else [])
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "full": ["--full"], "basic": ["--dry-run"]}[mode]
        argv = argv_head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode in ("install", "install_skip"):
        return [done.returncode == 0, tree(root)], done.stdout + done.stderr
    if mode == "plain":
        return done.returncode == 0, done.stdout + done.stderr
    return report_view(done.returncode, done.stdout, root), None


def run_case(argv_head, case, work, label, csharp, extra_env=None):
    guide, extra_folder = case
    out, texts = {}, {}
    modes = ("full_dry", "full", "basic", "install") + (("install_skip",) if csharp else ("plain",))
    for mode in modes:
        result, text = run_one(argv_head, guide, extra_folder, work, label, mode, csharp, extra_env)
        if mode in ("full_dry", "full", "basic"):
            for key, value in result.items():
                if mode != "full_dry" and key != "decision":
                    continue
                out[f"{mode}/{key}"] = value
        else:
            out[mode] = result
            texts[mode] = text
    return out, texts


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
    parser.add_argument("--verbose", action="store_true", help="print Rust's plain/install output")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="destination-path-parts-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, case, work, "csharp", True, env)[0] for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual, texts = run_case([args.bin], case, work, "rust", False)
            expected = oracle[name]
            if args.verbose:
                for mode, text in texts.items():
                    print(f"  {name}/{mode}: {text.strip().splitlines()[-1:] if text.strip() else ''}")
            for check in list(expected) + ["plain"]:
                if check == "plain":
                    same = actual["plain"] == expected["install"][0]
                    detail = f" | rust={actual['plain']} | reference install={expected['install'][0]}"
                elif check == "install_skip":
                    same = actual["install"] == expected["install_skip"]
                    detail = f" | rust={actual['install']} | reference={expected['install_skip']}"
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
                print(f"{verdict:5} {name}/{check}{detail if verdict != 'match' else ''}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
