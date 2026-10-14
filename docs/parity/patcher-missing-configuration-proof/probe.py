#!/usr/bin/env python3
"""Patcher missing configuration proof: drives the production `modsync validate --dry-run`.

In the validation dry run C# simulates a present Patcher step with
`TslPatcherVfsSimulator.SimulateInstallAsync` (Instruction.cs:1308-1326,
TSLPatcher/TslPatcherVfsSimulator.cs:23-51). It looks for `changes.ini` in the patcher
folder and in its `tslpatchdata` folder (:112-131); when neither exists it logs the
verbose line `[Simulation] No changes.ini found under '<dir>' - skipping TSLPatcher VFS
simulation.` and returns, so the step succeeds and the report gains nothing. Rust
added the warning `[PatcherSimulation] No changes.ini found; no InstallList effects
inferred` for the same step.

Each case is a disposable mod/game fixture plus a TOML guide. `mods/` holds `a.txt` to
`d.txt`; `game/` holds `chitin.key` and `Override/b.txt`. The patcher executable
`mods/patch/TSLPatcher.exe` is present in every case; what the case varies is its
configuration. The Rust binary and the pinned C# CLI (`ModSync.Core.dll`, d31ccbd3)
run the same command line on a fresh copy each:

  `validate -i G -s mods -g game --output json --dry-run --use-file-selection`

Checks per case:
  decision  exit code is zero or not, and the report's `success`;
  counts    the report's `errorCount` and `warningCount`;
  dryrun    the DryRun stage messages as kinds, in order: `skipped` for
            `[DryRunSkipped]`, else `error` or `warning` by severity;
  issues    each `dryRun.issues` entry's severity, category, componentName,
            componentGuid and instructionIndex.

Known differences (counted separately): an InstallList file that is missing from
`tslpatchdata` keeps Rust's deliberate `unknown payload` warning (C# writes an empty
placeholder silently). See ../patcher-missing-configuration-contract.md.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network; a tmpfs over `/Override` when that folder
exists, so a rootless C# run cannot write the host's) with DOTNET_ROLL_FORWARD=Major;
--record also rewrites csharp-oracle.json. Without it the recorded results are used.
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
MD = "<<modDirectory>>"
KD = "<<kotorDirectory>>"
OV = f"{KD}/Override"
EXE = f"{MD}/patch/TSLPatcher.exe"
INSTALL_TEX = "[InstallList]\ninstall_folder0=Override\n\n[install_folder0]\nFile0=tex.tga\n"
BARE = {"mods/patch/TSLPatcher.exe": "EXE\n"}
PAYLOAD = {"mods/patch/tslpatchdata/tex.tga": "payload tex.tga\n"}
PATCH = {**BARE, **PAYLOAD, "mods/patch/tslpatchdata/changes.ini": INSTALL_TEX}


def files(*parts, **extra):
    out = {}
    for part in parts:
        out.update(part)
    out.update({k.replace("__", "/"): v for k, v in extra.items()})
    return out


def step(action, sources, destination=None, overwrite=None, arguments=None):
    return {"action": action, "sources": sources, "destination": destination, "overwrite": overwrite,
            "arguments": arguments}


def patcher(source=EXE, arguments=None):
    return step("Patcher", [source], KD, arguments=arguments)


def copy(name, **extra):
    return step("Copy", [f"{MD}/{name}"], OV, **extra)


def component(key, steps):
    return {"key": key, "steps": steps}


def guide(components):
    lines = []
    for c in components:
        lines += ["[[thisMod]]", f'Guid = "{G[c["key"]]}"', f'Name = "{c["key"].upper()}"', "IsSelected = true"]
        for s in c["steps"]:
            lines += ["", "[[thisMod.Instructions]]", f"Action = {json.dumps(s['action'])}",
                      "Source = [" + ", ".join(json.dumps(x) for x in s["sources"]) + "]"]
            if s["destination"]:
                lines.append(f"Destination = {json.dumps(s['destination'])}")
            if s["overwrite"] is not None:
                lines.append(f"Overwrite = {'true' if s['overwrite'] else 'false'}")
            if s["arguments"]:
                lines.append(f"Arguments = {json.dumps(s['arguments'])}")
        lines.append("")
    return "\n".join(lines) + "\n"


CASES = {
    # The executable is present but neither <patch>/changes.ini nor
    # <patch>/tslpatchdata/changes.ini exists: C# skips the simulation silently.
    "no_tslpatchdata": (BARE, [component("b", [patcher()])]),
    "tslpatchdata_without_ini": (files(BARE, PAYLOAD), [component("b", [patcher()])]),
    "empty_tslpatchdata": (files(BARE, mods__patch__tslpatchdata__keep="k\n"), [component("b", [patcher()])]),
    "ini_in_subfolder": (files(BARE, PAYLOAD, {"mods/patch/tslpatchdata/sub/changes.ini": INSTALL_TEX}),
                         [component("b", [patcher()])]),
    "ini_other_name": (files(BARE, PAYLOAD, {"mods/patch/tslpatchdata/changes.txt": INSTALL_TEX}),
                       [component("b", [patcher()])]),
    "index_zero": (BARE, [component("b", [patcher(arguments="0")])]),
    "then_copy": (BARE, [component("b", [patcher(), copy("c.txt")])]),
    "after_copy": (BARE, [component("b", [copy("a.txt"), patcher()])]),
    "two_components": (files(BARE, {"mods/other/TSLPatcher.exe": "EXE\n"}),
                       [component("b", [patcher()]), component("c", [patcher(f"{MD}/other/TSLPatcher.exe")])]),
    "beside_present_patcher": (files(PATCH, {"mods/other/TSLPatcher.exe": "EXE\n"}),
                               [component("b", [patcher()]), component("c", [patcher(f"{MD}/other/TSLPatcher.exe")])]),
    # Controls. Nothing was simulated, so a later Rename of the file the INI would
    # install misses its source in both and the component is skipped (the skip drops
    # Rust's warning already).
    "control_then_rename_expected": (files(BARE, PAYLOAD), [component("b", [
        patcher(), step("Rename", [f"{OV}/tex.tga"], "renamed.tga")])]),
    "control_present": (PATCH, [component("b", [patcher()])]),
    "control_present_then_rename": (PATCH, [component("b", [
        patcher(), step("Rename", [f"{OV}/tex.tga"], "renamed.tga")])]),
    "control_ini_at_base": (files(BARE, {"mods/patch/changes.ini": INSTALL_TEX, "mods/patch/tex.tga": "t\n"}),
                            [component("b", [patcher()])]),
    "control_ini_empty": (files(BARE, {"mods/patch/tslpatchdata/changes.ini": ""}), [component("b", [patcher()])]),
    "control_ini_no_installlist": (files(BARE, {"mods/patch/tslpatchdata/changes.ini": "[Settings]\nx=1\n"}),
                                   [component("b", [patcher()])]),
    "control_missing_exe": ({}, [component("b", [patcher()])]),
    # Known difference: Rust marks an InstallList file missing from tslpatchdata as an
    # unknown payload (deliberate); C# writes an empty placeholder.
    "known_payload_missing": (files(BARE, {"mods/patch/tslpatchdata/changes.ini": INSTALL_TEX}),
                              [component("b", [patcher()])]),
}
CHECKS = ("decision", "counts", "dryrun", "issues")
KNOWN = {"known_payload_missing": {"counts", "dryrun", "issues"}}


def fixture(root, extra, components):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "mods"))
    os.makedirs(os.path.join(root, "home"))
    base = {"game/chitin.key": "KEY\n", "game/Override/b.txt": "old b\n"}
    base.update({f"mods/{k}.txt": f"{k}\n" for k in "abcd"})
    base.update(extra)
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


def run(argv_head, case, work, label, extra_env=None):
    extra, components = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, extra, components)
    argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--output", "json", "--dry-run",
                        "--use-file-selection"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    report = json.loads(done.stdout[done.stdout.index("{"):])
    dryrun = None
    for stage in report.get("stages") or []:
        if stage["stage"] == "DryRun":
            dryrun = [kind(m) for m in stage.get("messages") or []]
    issues = [[i.get("severity"), i.get("category"), i.get("componentName"), i.get("componentGuid"),
               i.get("instructionIndex")] for i in (report.get("dryRun") or {}).get("issues") or []]
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
    work = tempfile.mkdtemp(prefix="patcher-missing-configuration-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            shell = '[ -d /Override ] && mount -t tmpfs tmpfs /Override; exec dotnet "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", shell, "sh", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run(head, case, work, "csharp", env) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run([args.bin], case, work, "rust")
            expected = oracle[name]
            for check in CHECKS:
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
