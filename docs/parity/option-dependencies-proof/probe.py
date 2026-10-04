#!/usr/bin/env python3
"""Option Dependencies proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide in which component `B`
has Dependencies naming an option of component `A` (or, for the control, `A` itself).
Both sides run, on a fresh copy each, with `--use-file-selection`:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`
            (C# `WizardFull`, the wizard's Validate page and install preflight;
            Rust's desktop Validate step);
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts    `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages    each stage's name and `passed`;
  install            exit code is zero or not, and every file under `game/` and `mods/`
                     with its text afterwards (C#'s `.modsync` folder left out);
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
the recorded results are used. See ../option-dependencies-contract.md.
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
GA = "77777777-8888-9999-aaaa-bbbbbbbbbbb1"
GB = "77777777-8888-9999-aaaa-bbbbbbbbbbb2"
GC = "77777777-8888-9999-aaaa-bbbbbbbbbbb3"
OA = "77777777-8888-9999-aaaa-bbbbbbbbbbc1"
OA2 = "77777777-8888-9999-aaaa-bbbbbbbbbbc2"
OB = "77777777-8888-9999-aaaa-bbbbbbbbbbd1"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def option(guid, name, selected, steps=()):
    lines = ["[[thisMod.Options]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}",
             f"IsSelected = {'true' if selected else 'false'}", ""]
    for s in steps:
        lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return lines


def component(guid, name, steps, options=(), selected=True, extra=""):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}",
             f"IsSelected = {'true' if selected else 'false'}"]
    if extra:
        lines.append(extra)
    lines.append("")
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    for o in options:
        lines += o
    return "\n".join(lines)


def deps(*guids):
    return f"Dependencies = {json.dumps(list(guids))}"


def copy(source, destination=KD + "/Override", extra='Overwrite = true'):
    return step("Copy", [MD + "/" + source], destination, extra)


A_STEPS = [copy("a/x.txt")]
B_STEPS = [copy("b/x.txt"), copy("b.txt", KD)]


def a(option_selected=True, selected=True, steps=None, options=None):
    return component(GA, "A", A_STEPS if steps is None else steps,
                     options if options is not None else [option(OA, "A option", option_selected)],
                     selected=selected)


def b(extra, steps=None, guid=GB, name="B", options=()):
    return component(guid, name, B_STEPS if steps is None else steps, options, extra=extra)


CASES = {
    # Control: a Dependency on a selected component, accepted by both before and after.
    "control_component": a() + "\n" + b(deps(GA)),
    # A Dependency on a selected option of a selected component.
    "option_selected": a() + "\n" + b(deps(OA)),
    "option_listed_first": b(deps(OA)) + "\n" + a(),
    "option_braced_upper": a() + "\n" + b(deps("{" + OA.upper() + "}")),
    "option_and_component": a() + "\n" + b(deps(GA, OA)),
    "option_second_of_two": a(options=[option(OA, "A option", False, [copy("a/x.txt")]),
                                       option(OA2, "A second", True, [copy("a/y.txt")])])
                            + "\n" + b(deps(OA2)),
    "option_chosen_step": a(steps=[step("Choose", [OA])],
                            options=[option(OA, "A option", True, [copy("a/x.txt")])])
                          + "\n" + b(deps(OA)),
    "option_chain": a() + "\n" + b(deps(OA), options=[option(OB, "B option", True)])
                    + "\n" + b(deps(OB), steps=[copy("c.txt", KD)], guid=GC, name="C"),
    # Unchanged refusals (deliberate): the option is not selected, or its component is not.
    "option_unselected": a(option_selected=False) + "\n" + b(deps(OA)),
    "option_component_unselected": a(selected=False) + "\n" + b(deps(OA)),
}

UNSELECTED = ("Rust refuses a selected component whose Dependencies target is not selected "
              "(deliberate, install-order contract); C# installs it")
PARENT = ("Rust counts an option as selected only when its component is selected too; C# "
          "`FindComponentFromGuid` reads the option's own IsSelected, so C# installs B")
REFUSED = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision", "install", "plain")
# name -> {check: reason}. A known check passes only if Rust differs from the reference
# and shows the documented Rust refusal (see known_ok).
KNOWN = {
    "option_unselected": {check: UNSELECTED for check in REFUSED},
    "option_component_unselected": {check: PARENT for check in REFUSED},
}


def known_ok(actual):
    """The Rust side of a known difference is a refusal before any write."""
    return (not actual["plain"] and not actual["install"][0]
            and actual["install"][1] == BASELINE and not actual["full_dry/decision"][0])


FILES = {"game/chitin.key": "KEY\n", "mods/a/x.txt": "from A\n", "mods/a/y.txt": "y\n", "mods/b/x.txt": "from B\n",
         "mods/b.txt": "b\n", "mods/c.txt": "c\n"}
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
    return {
        "decision": [code == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
        "stages": [[stage["stage"], stage["passed"]] for stage in report.get("stages") or []],
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
    work = tempfile.mkdtemp(prefix="option-dependencies-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
                    verdict = "known" if not same and known_ok(actual) else "FAIL"
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
