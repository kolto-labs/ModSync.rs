#!/usr/bin/env python3
"""Option failure proof: drives the production `modsync validate --dry-run` and `install`.

C# runs a Choose step by executing each chosen option's instructions
(`ModComponent.ExecuteSingleInstructionAsync`, ModComponent.cs:1648-1667). Any option
result other than Success, a missing source included, becomes
`ActionExitCode.OptionalInstallFailed`, which fails the component with
`InstallExitCode.UserCancelledInstall` (:1852-1860). The dry run
(`DryRunValidator`, Services/Validation/DryRunValidator.cs:507-547) turns only
`MissingSourceFiles` into the `DryRunSkipped` warning; `UserCancelledInstall` is the
error `Dry-run failed for '<component>' with exit code UserCancelledInstall.` So a
chosen option whose source is missing fails validation, while the same miss in the
component's own steps is skipped with a warning.

Each case is a disposable mod/game fixture plus a TOML guide. `mods/` holds `a.txt` to
`h.txt`; `game/` holds `chitin.key` and an empty `Override/`. The Rust binary and the
pinned C# CLI (`ModSync.Core.dll`, d31ccbd3) run the same command line on a fresh copy
each, with `--use-file-selection`:

  dryrun   `validate -i G -s mods -g game --output json --dry-run --use-file-selection`
  install  `install -i G -s mods -g game --use-file-selection`; C# also gets
           `-y --skip-validation --no-checkpoint` (it asks for confirmation, and its
           environment check needs a HoloPatcher), Rust `--no-checkpoint`.

Checks per dryrun case:
  decision  exit code is zero or not, and the report's `success`;
  counts    the report's `errorCount` and `warningCount`;
  dryrun    the DryRun stage messages as kinds, in order: `skipped` for
            `[DryRunSkipped]`, else `error` or `warning` by severity (the wording
            differs: Rust names the failing step, C# prints the exit code).
Checks per install case:
  decision  exit code is zero or not.

Known differences (counted separately): an Extract with a missing archive in an option
also fails Rust's archive stage, where C# passes it (missing-archives contract), so
`counts` differs for those cases.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used. See
../option-failure-contract.md.
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
G = {k: f"{k * 8}-{k * 4}-{k * 4}-{k * 4}-{k * 12}" for k in "abcdef"}
OPT = {k: f"{k * 8}-0000-0000-0000-{k * 12}" for k in "123456789"}
MD = "<<modDirectory>>"
OV = "<<kotorDirectory>>/Override"
FILES = "abcdefgh"


def step(action, sources=None, destination=OV, overwrite=None, depends=None):
    return {"action": action, "sources": sources, "destination": destination, "overwrite": overwrite,
            "depends": depends}


def copy(name, **extra):
    return step("Copy", [f"{MD}/{name}"], **extra)


def choose(*options, **extra):
    return step("Choose", [OPT[o] for o in options], None, **extra)


def option(key, steps, selected=True, options=()):
    return {"key": key, "steps": steps, "selected": selected, "options": list(options)}


def component(key, steps, selected=True, options=()):
    return {"key": key, "steps": steps, "selected": selected, "options": list(options)}


def toml_steps(path, steps):
    lines = []
    for s in steps:
        lines += ["", f"[[{path}.Instructions]]", f"Action = {json.dumps(s['action'])}"]
        if s["sources"] is not None:
            lines.append("Source = [" + ", ".join(json.dumps(x) for x in s["sources"]) + "]")
        if s["destination"]:
            lines.append(f"Destination = {json.dumps(s['destination'])}")
        if s["overwrite"] is not None:
            lines.append(f"Overwrite = {'true' if s['overwrite'] else 'false'}")
        if s["depends"]:
            lines.append(f"Dependencies = [{json.dumps(G[s['depends']])}]")
    return lines


def toml_options(path, options):
    lines = []
    for o in options:
        lines += ["", f"[[{path}.Options]]", f'Guid = "{OPT[o["key"]]}"', f'Name = "Option {o["key"]}"',
                  f"IsSelected = {'true' if o['selected'] else 'false'}"]
        lines += toml_steps(f"{path}.Options", o["steps"])
        lines += toml_options(f"{path}.Options", o["options"])
    return lines


def guide(components):
    lines = []
    for c in components:
        lines += ["[[thisMod]]", f'Guid = "{G[c["key"]]}"', f'Name = "{c["key"].upper()}"',
                  f"IsSelected = {'true' if c['selected'] else 'false'}"]
        lines += toml_steps("thisMod", c["steps"])
        lines += toml_options("thisMod", c["options"])
        lines.append("")
    return "\n".join(lines) + "\n"


def b(steps, options, **extra):
    """Component B: copies b.txt, then the case's steps."""
    return component("b", [copy("b.txt")] + steps, options=options, **extra)


DELETE_STRICT = step("Delete", [f"{OV}/nope.txt"], None, overwrite=True)
CASES = {
    # A chosen option whose step misses its source: C# fails the component.
    "option_copy_missing": ("dryrun", [b([choose("1")], [option("1", [copy("nope.txt")])])]),
    "option_copy_wildcard_missing": ("dryrun", [b([choose("1")], [option("1", [copy("nope*.txt")])])]),
    "option_move_missing": ("dryrun", [b([choose("1")], [option("1", [step("Move", [f"{MD}/nope.txt"])])])]),
    "option_delete_strict_missing": ("dryrun", [b([choose("1")], [option("1", [DELETE_STRICT])])]),
    "option_rename_missing": ("dryrun", [b([choose("1")], [option("1", [
        step("Rename", [f"{OV}/nope.txt"], "renamed.txt")])])]),
    "option_cleanlist_missing": ("dryrun", [b([choose("1")], [option("1", [
        step("CleanList", [f"{MD}/nope.csv"])])])]),
    "option_extract_missing": ("dryrun", [b([choose("1")], [option("1", [
        step("Extract", [f"{MD}/nope.zip"], None)])])]),
    "option_missing_after_present": ("dryrun", [b([choose("1")], [option("1", [copy("c.txt"), copy("nope.txt")])])]),
    "second_option_missing": ("dryrun", [b([choose("1", "2")], [option("1", [copy("c.txt")]),
                                                                option("2", [copy("nope.txt")])])]),
    "second_choose_missing": ("dryrun", [b([choose("1"), choose("2")], [option("1", [copy("c.txt")]),
                                                                       option("2", [copy("nope.txt")])])]),
    "option_missing_then_later_steps": ("dryrun", [b([choose("1"), copy("d.txt")], [option("1", [copy("nope.txt")])])]),
    "option_missing_other_component_ok": ("dryrun", [b([choose("1")], [option("1", [copy("nope.txt")])]),
                                                    component("c", [copy("c.txt")])]),
    "two_components_option_missing": ("dryrun", [b([choose("1")], [option("1", [copy("nope.txt")])]),
                                                component("c", [copy("c.txt"), choose("2")],
                                                          options=[option("2", [copy("nope2.txt")])])]),
    # The option's own step does not run (unmet instruction dependency): nothing is missing.
    "option_conditional_missing": ("dryrun", [component("a", [copy("a.txt")], selected=False),
                                             b([choose("1")], [option("1", [copy("nope.txt", depends="a")])])]),
    # Controls: the same miss in the component's own steps is skipped with a warning.
    "control_own_missing": ("dryrun", [b([copy("nope.txt")], [])]),
    "control_own_wildcard_missing": ("dryrun", [b([copy("nope*.txt")], [])]),
    "control_own_missing_before_option": ("dryrun", [b([copy("nope.txt"), choose("1")],
                                                      [option("1", [copy("nope2.txt")])])]),
    "control_option_present": ("dryrun", [b([choose("1")], [option("1", [copy("c.txt")])])]),
    "control_option_unselected": ("dryrun", [b([choose("1")], [option("1", [copy("nope.txt")], selected=False)])]),
    "control_option_not_chosen": ("dryrun", [b([], [option("1", [copy("nope.txt")])])]),
    "control_choose_condition_unmet": ("dryrun", [component("a", [copy("a.txt")], selected=False),
                                                 b([choose("1", depends="a")], [option("1", [copy("nope.txt")])])]),
    "control_component_unselected": ("dryrun", [component("a", [copy("a.txt")]),
                                               b([choose("1")], [option("1", [copy("nope.txt")])], selected=False)]),
    # Install refuses in both (Rust before any write, C# at the option).
    "install_option_copy_missing": ("install", [b([choose("1")], [option("1", [copy("nope.txt")])])]),
    "install_control_option_present": ("install", [b([choose("1")], [option("1", [copy("c.txt")])])]),
}
CHECKS = {"dryrun": ("decision", "counts", "dryrun"), "install": ("decision",)}
KNOWN = {"option_extract_missing": {"counts"}}


def fixture(root, components):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    with open(os.path.join(root, "game", "chitin.key"), "w") as handle:
        handle.write("KEY\n")
    for key in FILES:
        with open(os.path.join(root, "mods", f"{key}.txt"), "w") as handle:
            handle.write(key + "\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(components))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def kind(message):
    if "[DryRunSkipped]" in message:
        return "skipped"
    return "error" if message.startswith("ERROR") else "warning"


def run(argv_head, case, work, label, csharp, extra_env=None):
    mode, components = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, components)
    if mode == "dryrun":
        argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run",
                            "--use-file-selection"]
    else:
        side = ["-y", "--skip-validation", "--no-checkpoint"] if csharp else ["--no-checkpoint"]
        argv = argv_head + ["install", "-i", path, "-s", mods, "-g", game, "--use-file-selection"] + side
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    if mode == "install":
        return {"decision": done.returncode == 0}
    report = json.loads(done.stdout[done.stdout.index("{"):])
    dryrun = None
    for stage in report.get("stages") or []:
        if stage["stage"] == "DryRun":
            dryrun = [kind(m) for m in stage.get("messages") or []]
    return {
        "decision": [done.returncode == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount")],
        "dryrun": dryrun,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="option-failure-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, case, work, "csharp", True, env) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run([args.bin], case, work, "rust", False)
            expected = oracle[name]
            for check in CHECKS[case[0]]:
                same = actual[check] == expected[check]
                if same:
                    label = "match"
                    matched += 1
                elif check in KNOWN.get(name, ()):
                    label = "known"
                    known += 1
                else:
                    label = "FAIL"
                    failed += 1
                detail = "" if same else f" | rust={actual[check]} | reference={expected[check]}"
                print(f"{label:5} {name}/{check}{detail}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
