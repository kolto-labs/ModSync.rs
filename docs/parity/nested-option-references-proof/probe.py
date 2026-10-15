#!/usr/bin/env python3
"""Nested option references proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide in which component `A`
has a selected option `O`, and `O` has an option `N` of its own (a nested option,
`[[thisMod.Options.Options]]`). A Restriction of component `B`, or a Dependency or
Restriction of a step, names `N` (or, for the controls, `O`). The C# loader keeps no
option below an option, so for C# such a GUID names nothing. Both sides run, on a
fresh copy each, with `--use-file-selection`:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`
            (C# `WizardFull`, the wizard's Validate page and install preflight;
            Rust's desktop Validate step);
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts     `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages     each stage's name and `passed`;
  full_dry/conflicts  the `Conflicts` stage's messages;
  install             exit code is zero or not, and every file under `game/` and
                      `mods/` with its text afterwards (C#'s `.modsync` folder left out);
  plain               Rust's plain `validate` decides as C# `install`.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder;
the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2 (see
../full-pipeline-destination-proof/README.md).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../nested-option-references-contract.md.
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
GA = "77777777-8888-9999-aaaa-ccccccccccc1"
GB = "77777777-8888-9999-aaaa-ccccccccccc2"
OA = "77777777-8888-9999-aaaa-cccccccccdc1"
ON = "77777777-8888-9999-aaaa-cccccccccec1"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def copy(source, destination=KD + "/Override", condition=""):
    return step("Copy", [MD + "/" + source], destination,
                "Overwrite = true" + (f"\n{condition}" if condition else ""))


def guids(key, *values):
    return f"{key} = {json.dumps(list(values))}"


def option_o(steps=(), nested_selected=True, nested_steps=()):
    """`O` (selected) with its nested option `N`."""
    lines = ["[[thisMod.Options]]", f'Guid = "{OA}"', 'Name = "A option"', "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    lines += ["[[thisMod.Options.Options]]", f'Guid = "{ON}"', 'Name = "Nested"',
              f"IsSelected = {'true' if nested_selected else 'false'}", ""]
    for s in nested_steps:
        lines += ["[[thisMod.Options.Options.Instructions]]"] + s + [""]
    return lines


def component(guid, name, steps, options=(), extra=""):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true"]
    if extra:
        lines.append(extra)
    lines.append("")
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    return "\n".join(lines + list(options))


def a(own=None, **option):
    """`A`: its own steps (default: copy `a/x.txt`) and `O`, built by `option_o(**option)`."""
    return component(GA, "A", [copy("a/x.txt")] if own is None else own, option_o(**option))


def b(extra="", steps=()):
    return component(GB, "B", [copy("b/x.txt"), copy("b.txt", KD)] + list(steps), extra=extra)


CHOOSE_O = [step("Choose", [OA])]
CASES = {
    # A Restriction of B naming the nested option N.
    "restriction_nested": a() + "\n" + b(guids("Restrictions", ON)),
    "restriction_nested_unselected": a(nested_selected=False) + "\n" + b(guids("Restrictions", ON)),
    # A step of B whose Dependencies or Restrictions name N, and the immediate-option controls.
    "step_dependency_nested": a() + "\n" + b(steps=[copy("c.txt", KD, guids("Dependencies", ON))]),
    "step_restriction_nested": a() + "\n" + b(steps=[copy("c.txt", KD, guids("Restrictions", ON))]),
    "step_dependency_option": a() + "\n" + b(steps=[copy("c.txt", KD, guids("Dependencies", OA))]),
    "step_restriction_option": a() + "\n" + b(steps=[copy("c.txt", KD, guids("Restrictions", OA))]),
    # A step of the chosen option O itself whose condition names its own nested option N.
    "option_step_dependency_nested": a(CHOOSE_O, steps=[copy("a/y.txt", condition=guids("Dependencies", ON))])
                                     + "\n" + b(),
    "option_step_restriction_nested": a(CHOOSE_O, steps=[copy("a/y.txt", condition=guids("Restrictions", ON))])
                                      + "\n" + b(),
    # Unchanged (known): a Restriction naming the chosen immediate option O.
    "restriction_option": a(CHOOSE_O, steps=[copy("a/y.txt")]) + "\n" + b(guids("Restrictions", OA)),
    # Unchanged (known): O's Choose step names N, which copies a/y.txt.
    "nested_chosen": a(CHOOSE_O, steps=[step("Choose", [ON])], nested_steps=[copy("a/y.txt")]) + "\n" + b(),
}

ALL = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "full_dry/conflicts",
       "basic/decision", "install", "plain")
RESTRICTION = ("Rust refuses a Restrictions conflict (deliberate, install-order contract); the C# "
               "CLI loader deselects O (AttemptFixes, AutoFixComponentIssues) and installs A and B")
CHOSEN = ("Rust runs an option chosen by a Choose step of an option (a Rust extension); the C# "
          "loader keeps no option below an option, so the Choose chooses nothing")
# name -> {check: reason}
KNOWN = {
    "restriction_option": {check: RESTRICTION for check in ALL},
    "nested_chosen": {"install": CHOSEN},
}


def known_ok(name, actual, expected):
    """The Rust side of each known difference is the documented one."""
    if name == "restriction_option":
        return (not actual["plain"] and not actual["install"][0]
                and actual["install"][1] == BASELINE and not actual["full_dry/decision"][0])
    return (actual["install"][0]
            and actual["install"][1] == sorted(expected["install"][1] + [["game/Override/y.txt", "y\n"]]))


FILES = {"game/chitin.key": "KEY\n", "mods/a/x.txt": "from A\n", "mods/a/y.txt": "y\n",
         "mods/b/x.txt": "from B\n", "mods/b.txt": "b\n", "mods/c.txt": "c\n"}
BASELINE = sorted([name, text] for name, text in FILES.items())


def fixture(root, guide):
    for folder in ("game/Override", "home"):
        os.makedirs(os.path.join(root, folder))
    for name, text in FILES.items():
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
                full = os.path.join(current, name)
                with open(full) as handle:
                    out.append([os.path.relpath(full, root).replace(os.sep, "/"), handle.read()])
    return sorted(out)


def report_view(code, stdout):
    report = json.loads(stdout[stdout.index("{"):])
    stages = report.get("stages") or []
    return {
        "decision": [code == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stages": [[stage["stage"], stage["passed"]] for stage in stages],
        "conflicts": [stage.get("messages") or [] for stage in stages if stage["stage"] == "Conflicts"],
    }


def run_one(argv_head, guide, work, label, mode, csharp, extra_env):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, guide)
    common = ["-i", path, "-s", mods, "-g", game, "--use-file-selection"]
    if mode == "install":
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "basic": ["--dry-run"]}[mode]
        argv = argv_head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    text = done.stdout + done.stderr
    if mode == "install":
        return [done.returncode == 0, tree(root)], text
    if mode == "plain":
        return done.returncode == 0, text
    return report_view(done.returncode, done.stdout), None


def run_case(argv_head, guide, work, label, csharp, extra_env=None):
    out, texts = {}, {}
    for mode in ("full_dry", "basic", "install") + (() if csharp else ("plain",)):
        result, text = run_one(argv_head, guide, work, label, mode, csharp, extra_env)
        if mode in ("full_dry", "basic"):
            for key, value in result.items():
                if mode == "full_dry" or key == "decision":
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
    work = tempfile.mkdtemp(prefix="nested-option-references-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, guide, work, "csharp", True, env)[0] for name, guide in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, guide in CASES.items():
            actual, texts = run_case([args.bin], guide, work, "rust", False)
            expected = oracle[name]
            if args.verbose:
                for mode, text in texts.items():
                    print(f"  {name}/{mode}: {text.strip().splitlines()[-1:] if text.strip() else ''}")
            for check in list(expected) + ["plain"]:
                if check == "plain":
                    same = actual["plain"] == expected["install"][0]
                    detail = f" | rust={actual['plain']} | reference install={expected['install'][0]}"
                else:
                    same = actual[check] == expected[check]
                    detail = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    verdict = "known" if not same and known_ok(name, actual, expected) else "FAIL"
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
