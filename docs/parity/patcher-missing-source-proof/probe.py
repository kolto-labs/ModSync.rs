#!/usr/bin/env python3
"""Patcher missing source proof: drives the production `modsync validate --dry-run` and `install`.

C# runs a Patcher step through `ModComponent.ExecuteSingleInstructionAsync`
(ModComponent.cs:1630-1641): when `TryResolveInstructionSourcesAsync` finds no source it
logs `Missing patcher folder/archive for '<name>'` and returns
`ActionExitCode.FileNotFoundPre`, the same code as a missing Copy, Move or Extract
source. The component's loop maps it to `InstallExitCode.MissingSourceFiles`
(:1862-1866), and the dry run (`DryRunValidator`,
Services/Validation/DryRunValidator.cs:515-527) reports that as the `DryRunSkipped`
warning `Skipped '<component>' during dry-run: source archives or files are not
available.` A chosen option's Patcher miss is still `OptionalInstallFailed` (an
error), and install fails the component.

Each case is a disposable mod/game fixture plus a TOML guide. `mods/` holds `a.txt` to
`d.txt`; `game/` holds `chitin.key` and `Override/b.txt`. A present patcher is
`mods/patch/TSLPatcher.exe` with `tslpatchdata/changes.ini` installing `tex.tga` into
Override. The Rust binary and the pinned C# CLI (`ModSync.Core.dll`, d31ccbd3) run the
same command line on a fresh copy each, with `--use-file-selection` (so the guide's
`IsSelected` flags hold):

  dryrun   `validate -i G -s mods -g game --output json --dry-run --use-file-selection`
  install  `install -i G -s mods -g game --use-file-selection`; C# also gets `-y --skip-validation
           --no-checkpoint` (it asks for confirmation, and its environment check needs
           a HoloPatcher), Rust `--no-checkpoint`.

Checks per dryrun case:
  decision  exit code is zero or not, and the report's `success`;
  counts    the report's `errorCount` and `warningCount`;
  dryrun    the DryRun stage messages as kinds, in order: `skipped` for
            `[DryRunSkipped]`, else `error` or `warning` by severity;
  issues    each `dryRun.issues` entry's severity, componentName and componentGuid,
            plus category and instructionIndex for warnings (an error's wording,
            category and step differ by design).
Checks per install case:
  decision  exit code is zero or not.

Known differences (counted separately): Rust refuses a Patcher Source that is a
wildcard or an empty list (C# skips the component), and binds a Patcher Source that
names a folder (C# finds no file there and skips the component). See
../patcher-missing-source-contract.md.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used.
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
KD = "<<kotorDirectory>>"
OV = f"{KD}/Override"
EXE = f"{MD}/patch/TSLPatcher.exe"
PATCH = {
    "mods/patch/TSLPatcher.exe": "EXE\n",
    "mods/patch/tslpatchdata/changes.ini": "[InstallList]\ninstall_folder0=Override\n\n[install_folder0]\nFile0=tex.tga\n",
    "mods/patch/tslpatchdata/tex.tga": "payload tex.tga\n",
}


def step(action, sources, destination=None, overwrite=None, depends=None, arguments=None):
    return {"action": action, "sources": sources, "destination": destination, "overwrite": overwrite,
            "depends": depends, "arguments": arguments}


def patcher(source=EXE, **extra):
    return step("Patcher", [source] if source is not None else [], KD, **extra)


def copy(name, **extra):
    return step("Copy", [f"{MD}/{name}"], OV, **extra)


def choose(*options):
    return step("Choose", [OPT[o] for o in options])


def component(key, steps, selected=True, options=()):
    return {"key": key, "steps": steps, "selected": selected, "options": list(options)}


def option(key, steps):
    return {"key": key, "steps": steps}


def toml_steps(path, steps):
    lines = []
    for s in steps:
        lines += ["", f"[[{path}.Instructions]]", f"Action = {json.dumps(s['action'])}",
                  "Source = [" + ", ".join(json.dumps(x) for x in s["sources"]) + "]"]
        if s["destination"]:
            lines.append(f"Destination = {json.dumps(s['destination'])}")
        if s["overwrite"] is not None:
            lines.append(f"Overwrite = {'true' if s['overwrite'] else 'false'}")
        if s["arguments"]:
            lines.append(f"Arguments = {json.dumps(s['arguments'])}")
        if s["depends"]:
            lines.append(f"Dependencies = [{json.dumps(G[s['depends']])}]")
    return lines


def guide(components):
    lines = []
    for c in components:
        lines += ["[[thisMod]]", f'Guid = "{G[c["key"]]}"', f'Name = "{c["key"].upper()}"',
                  f"IsSelected = {'true' if c['selected'] else 'false'}"]
        lines += toml_steps("thisMod", c["steps"])
        for o in c["options"]:
            lines += ["", "[[thisMod.Options]]", f'Guid = "{OPT[o["key"]]}"', f'Name = "Option {o["key"]}"',
                      "IsSelected = true"]
            lines += toml_steps("thisMod.Options", o["steps"])
        lines.append("")
    return "\n".join(lines) + "\n"


CASES = {
    # An own-step Patcher whose Source is missing: C# skips the component.
    "missing_folder": ("dryrun", {}, [component("b", [patcher()])]),
    "missing_tslpatchdata": ("dryrun", {}, [component("b", [patcher(f"{MD}/patch/tslpatchdata", arguments="0")])]),
    "missing_exe_in_folder": ("dryrun", {"mods/patch/readme.txt": "r\n"}, [component("b", [patcher()])]),
    "missing_after_copy": ("dryrun", {}, [component("b", [copy("a.txt"), patcher()])]),
    "missing_then_later_steps": ("dryrun", {}, [component("b", [patcher(), copy("c.txt")])]),
    "missing_other_component_ok": ("dryrun", {}, [component("b", [patcher()]), component("c", [copy("c.txt")])]),
    "two_components_missing": ("dryrun", {}, [component("b", [patcher()]),
                                             component("c", [copy("c.txt"), patcher(f"{MD}/other/TSLPatcher.exe")])]),
    # Rust warns when Overwrite = false keeps an existing file; the skip drops it, as
    # C# reports only the skip.
    "missing_after_kept_file": ("dryrun", {}, [component("b", [copy("b.txt", overwrite=False), patcher()])]),
    # Controls.
    "control_present": ("dryrun", PATCH, [component("b", [patcher()])]),
    "control_present_then_rename": ("dryrun", PATCH, [component("b", [
        patcher(), step("Rename", [f"{OV}/tex.tga"], "renamed.tga")])]),
    "control_condition_unmet": ("dryrun", {}, [component("a", [copy("a.txt")], selected=False),
                                              component("b", [copy("b.txt"), patcher(depends="a")])]),
    "control_copy_missing": ("dryrun", {}, [component("b", [copy("nope.txt")])]),
    "control_option_missing": ("dryrun", {}, [component("b", [copy("a.txt"), choose("1")],
                                                       options=[option("1", [patcher()])])]),
    "control_component_unselected": ("dryrun", {}, [component("a", [copy("a.txt")]),
                                                   component("b", [patcher()], selected=False)]),
    # Known differences: Rust refuses these Sources; C# finds no file and skips.
    "known_wildcard_missing": ("dryrun", {}, [component("b", [patcher(f"{MD}/pat*/TSLPatcher.exe")])]),
    "known_empty_source": ("dryrun", {}, [component("b", [patcher(None)])]),
    "known_folder_source": ("dryrun", PATCH, [component("b", [patcher(f"{MD}/patch")])]),
    # Install refuses in both.
    "install_missing": ("install", {}, [component("b", [copy("a.txt"), patcher()])]),
}
CHECKS = {"dryrun": ("decision", "counts", "dryrun", "issues"), "install": ("decision",)}
KNOWN = {name: set(CHECKS["dryrun"]) for name in CASES if name.startswith("known_")}


def fixture(root, files, components):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    base = {"game/chitin.key": "KEY\n", "game/Override/b.txt": "old b\n"}
    base.update({f"mods/{k}.txt": f"{k}\n" for k in "abcd"})
    base.update(files)
    for rel, content in base.items():
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(content)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(components))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def kind(message):
    if "[DryRunSkipped]" in message:
        return "skipped"
    return "error" if message.startswith("ERROR") else "warning"


def run(argv_head, case, work, label, csharp, extra_env=None):
    mode, files, components = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, files, components)
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
    issues = []
    for i in (report.get("dryRun") or {}).get("issues") or []:
        fields = [i.get("severity"), i.get("componentName"), i.get("componentGuid")]
        # An error's category and step differ by design (Rust `EngineSimulation` with
        # the step, C# `DryRun` without; option-failure contract, Known differences).
        if i.get("severity") != "Error":
            fields += [i.get("category"), i.get("instructionIndex")]
        issues.append(fields)
    return {
        "decision": [done.returncode == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount")],
        "dryrun": dryrun,
        "issues": issues,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="patcher-missing-source-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
