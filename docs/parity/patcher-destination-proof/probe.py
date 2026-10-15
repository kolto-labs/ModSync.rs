#!/usr/bin/env python3
"""Patcher destination proof: drives the production `modsync`.

C# runs every Patcher step on the game folder: `ExecuteTSLPatcherAsync` passes
`MainConfig.DestinationPath` as `--game-dir`, and the dry run's
`TslPatcherVfsSimulator` uses it too (Instruction.cs:1308-1325, 1349-1356). The
step's Destination is only validated: `ParseDestinationWithAction` warns about an
empty one and reports any other than `<<gameDirectory>>`/`<<kotorDirectory>>`, then
sets it to `<<gameDirectory>>` (AttemptFixes; ComponentValidation.cs:265-292). Rust
ran the patcher on the Destination (`<<kotorDirectory>>/Override` gave
`--game-dir <game>/Override`) and its reports failed an empty or other Destination.

Each case is a disposable mod/game fixture with one TOML guide. The mod folder holds
`p/TSLPatcher.exe`, `p/tslpatchdata/{changes.ini, n.tga}` (an InstallList putting
`n.tga` into `Override`) and an empty `backup/` folder; the game holds `chitin.key`
and `Override/g.tga`. Both sides use the same HoloPatcher stub, a shell script: with
`--game-dir G --tslpatchdata T` it writes `G/Override/n.tga` and `T/installlog.txt`
and exits 0; without them (C#'s environment test, `--install`) it exits 2. Both sides
run, on a fresh copy each:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`
            (C# `WizardFull`, the wizard's Validate page and install preflight;
            Rust's desktop Validate step);
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first, and finds the stub in its
            `Resources` folder; Rust also gets `--allow-processes --patcher <stub>`;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts, basic/counts      `errorCount`, `warningCount`, `passedCount`;
  full_dry/stage, basic/stage        the ComponentValidation stage's `passed` and
                                     messages (the fixture folder shown as `<root>`);
  install             exit code is zero or not, and every file under `game/` and
                      `mods/` with its text afterwards (C#'s `.modsync` folder and
                      the patcher folder `mods/p/` left out: C# writes its install log
                      and INI edits there, Rust in a staged copy);
  plain               Rust's plain `validate` decides as C# `install`.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy holding the stub under `unshare -Urnm` (no network, a tmpfs over
`/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also rewrites
csharp-oracle.json. Without it the recorded results are used. See
../patcher-destination-contract.md.
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
GA = "99999999-aaaa-bbbb-cccc-dddddddddd01"
GO = "99999999-aaaa-bbbb-cccc-dddddddddd11"
OVERRIDE = KD + "/Override"
PATCHER = MD + "/p/TSLPatcher.exe"

STUB = """#!/bin/sh
game=""; data=""
while [ $# -gt 0 ]; do
  case "$1" in
    --game-dir) game="$2"; shift 2;;
    --tslpatchdata) data="$2"; shift 2;;
    *) shift;;
  esac
done
[ -n "$game" ] && [ -n "$data" ] || exit 2
mkdir -p "$game/Override" && printf 'patched\\n' > "$game/Override/n.tga" || exit 4
printf 'Installation complete\\n' > "$data/installlog.txt"
exit 0
"""


def step(action, sources, destination=None):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    return lines


def guide(steps, options=()):
    lines = ["[[thisMod]]", f'Guid = "{GA}"', 'Name = "A"', ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    return "\n".join(lines + list(options))


def option(steps):
    lines = ["[[thisMod.Options]]", f'Guid = "{GO}"', 'Name = "O"', "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return lines


def patched(destination):
    """A Patcher step, then a Copy of the file it installs."""
    return guide([step("Patcher", [PATCHER], destination), step("Copy", [OVERRIDE + "/n.tga"], MD + "/backup")])


CASES = {
    # Fixed: the patcher runs on the game folder whatever its Destination.
    "no_destination": patched(None),
    "override_destination": patched(OVERRIDE),
    "mod_destination": guide([step("Patcher", [PATCHER], MD)]),
    "trailing_separator": patched(KD + "/"),
    "option_no_destination": guide([step("Choose", [GO])],
                                   option([step("Patcher", [PATCHER]),
                                           step("Copy", [OVERRIDE + "/n.tga"], MD + "/backup")])),
    # Unchanged controls; the placeholder in another case is not found in either.
    "game_destination": patched(KD),
    "gamedirectory_destination": patched("<<gameDirectory>>"),
    "placeholder_case": patched("<<KotorDirectory>>"),
    # Known difference: a Destination Rust cannot resolve is still refused.
    "unsafe_destination": patched("/tmp/elsewhere"),
}

STRICT = "Rust refuses before any write; nothing installed"
# name -> {check: reason}
KNOWN = {
    "placeholder_case": {
        "full_dry/counts": "both refuse; C# fails its Environment stage, Rust its "
                           "ComponentValidation and DryRun stages (Path must start with ...)",
        "full_dry/stage": "as full_dry/counts",
        "basic/counts": "both refuse; Rust's DryRun stage also refuses the step",
        "basic/stage": "C# names the unresolved path, Rust the unknown placeholder",
    },
    "unsafe_destination": {
        "install": "a Destination outside the roots stays refused (fail closed)",
        "plain": STRICT,
        "full_dry/decision": "Rust's dry run refuses the step (Path must start with ...)",
        "full_dry/counts": "as full_dry/decision",
        "basic/counts": "as full_dry/decision, after C#'s Destination error",
    },
}

CHANGES = ("[Settings]\nLookupGameFolder=0\n\n[InstallList]\ninstall_folder0=Override\n\n"
           "[install_folder0]\nFile0=n.tga\n")
FILES = {"game/chitin.key": "KEY\n", "game/Override/g.tga": "game tga\n", "mods/p/TSLPatcher.exe": "MZ\n",
         "mods/p/tslpatchdata/changes.ini": CHANGES, "mods/p/tslpatchdata/n.tga": "n tga\n"}


def fixture(root, text):
    for folder in ("game/Override", "mods/backup", "home"):
        os.makedirs(os.path.join(root, folder))
    for name, content in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(content)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(text)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    out = []
    for folder in ("game", "mods"):
        base = os.path.join(root, folder)
        for current, dirs, files in os.walk(base):
            rel_dir = os.path.relpath(current, root).replace(os.sep, "/")
            dirs[:] = sorted(d for d in dirs if d != ".modsync" and f"{rel_dir}/{d}" != "mods/p")
            for name in files:
                full = os.path.join(current, name)
                with open(full) as handle:
                    out.append([os.path.relpath(full, root).replace(os.sep, "/"), handle.read()])
    return sorted(out)


BASELINE = None


def baseline():
    global BASELINE
    if BASELINE is None:
        with tempfile.TemporaryDirectory() as root:
            fixture(root, "")
            BASELINE = tree(root)
    return BASELINE


def known_ok(name, check, actual):
    """The Rust side of each known difference is the documented one."""
    if check == "install":
        return actual["install"] == [False, baseline()]
    if check == "plain":
        return actual["plain"] is False
    if check == "full_dry/decision":
        return actual[check] == [False, False]
    if check.endswith("/counts"):
        return actual[check][0] == (1 if (name, check) == ("unsafe_destination", "full_dry/counts") else 2)
    if check.endswith("/stage"):
        passed, messages = actual[check]
        return passed is False and messages == [
            "ERROR: A: archive/destination validation: Path must start with <<modDirectory>> or "
            "<<kotorDirectory>>: <<KotorDirectory>>"]
    return False


def report_view(code, stdout, root):
    report = json.loads(stdout[stdout.index("{"):].replace(root, "<root>"))
    stage = next((s for s in report.get("stages", []) if s.get("stage") == "ComponentValidation"), {})
    return {
        "decision": [code == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stage": [stage.get("passed"), stage.get("messages")],
    }


def run_one(argv_head, text, work, label, mode, csharp, extra_env, stub):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, text)
    common = ["-i", path, "-s", mods, "-g", game]
    if mode == "install":
        flags = ["-y"] if csharp else ["--allow-processes", "--patcher", stub]
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + flags
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "basic": ["--dry-run"]}[mode]
        argv = argv_head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    text_out = done.stdout + done.stderr
    if mode == "install":
        return [done.returncode == 0, tree(root)], text_out
    if mode == "plain":
        return done.returncode == 0, text_out
    return report_view(done.returncode, done.stdout, json.dumps(root)[1:-1]), None


def run_case(argv_head, text, work, label, csharp, stub, extra_env=None):
    out, texts = {}, {}
    for mode in ("full_dry", "basic", "install") + (() if csharp else ("plain",)):
        result, text_out = run_one(argv_head, text, work, label, mode, csharp, extra_env, stub)
        if mode in ("full_dry", "basic"):
            for key, value in result.items():
                out[f"{mode}/{key}"] = value
        else:
            out[mode] = result
            texts[mode] = text_out
    return out, texts


def write_stub(path):
    with open(path, "w") as handle:
        handle.write(STUB)
    os.chmod(path, 0o755)
    return path


def stub_reference(reference, work):
    """A copy of the reference build with the stub as its Resources HoloPatcher."""
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    write_stub(os.path.join(copy_dir, "Resources", "holopatcher"))
    return copy_dir


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--verbose", action="store_true", help="print Rust's plain/install output")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="patcher-destination-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        stub = write_stub(os.path.join(work, "holopatcher"))
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, text, work, "csharp", True, stub, env)[0]
                      for name, text in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, text in CASES.items():
            actual, texts = run_case([os.path.abspath(args.bin)], text, work, "rust", False, stub)
            expected = oracle[name]
            if args.verbose:
                for mode, output in texts.items():
                    print(f"  {name}/{mode}: {output.strip().splitlines()[-1:] if output.strip() else ''}")
            for check in sorted(expected) + ["plain"]:
                if check == "plain":
                    same = actual["plain"] == expected["install"][0]
                    detail = f" | rust={actual['plain']} | reference install={expected['install'][0]}"
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
