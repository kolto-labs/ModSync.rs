#!/usr/bin/env python3
"""Wildcard-miss proof: drives the production `modsync`.

C# Copy and Move bind their Sources with `Instruction.SetRealPaths`, which gathers the
files of every Source (`PathHelper.EnumerateFilesWithWildcards`) and fails only when
the gathered list is empty (Instruction.cs:300-338). A wildcard Source that matches
nothing is therefore dropped when another Source of the step names a file:
`Move [m/*.tga, m/*.tpc]` with only TGAs moves the TGAs. Rename gathers the same way
(`SetRealPaths(skipExistenceCheck: true)`). Rust required every Source to match, so
plain `validate` and `install` refused such a step before any write; the dry-run
reports skipped a Copy or Move component with a `DryRunSkipped` warning and failed a
Rename.

Each case is a disposable mod/game fixture with one TOML guide. The mod folder holds
`m/a.tga`, `m/b.txi`, `c.txt`, an empty `backup/` folder and `p.zip` (holding
`x.tga`); the game holds `Override/g.tga`. Both sides run, on a fresh copy each:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`
            (C# `WizardFull`, the wizard's Validate page and install preflight;
            Rust's desktop Validate step);
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts, basic/counts      `errorCount`, `warningCount`, `passedCount`;
  install             exit code is zero or not, and every file under `game/` and
                      `mods/` with its text afterwards (C#'s `.modsync` folder left
                      out; `p.zip` is listed by name);
  plain               Rust's plain `validate` decides as C# `install`.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder;
the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../wildcard-miss-contract.md.
"""
import argparse
import io
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
GA = "88888888-9999-aaaa-bbbb-cccccccccc01"
GB = "88888888-9999-aaaa-bbbb-cccccccccc02"
GO = "88888888-9999-aaaa-bbbb-cccccccccd01"
OVERRIDE = KD + "/Override"


def step(action, sources, destination=None):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    return lines


def component(guid, name, steps, options=()):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    return "\n".join(lines + list(options))


def option(steps):
    lines = ["[[thisMod.Options]]", f'Guid = "{GO}"', 'Name = "O"', "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return lines


def a(*steps, options=()):
    return component(GA, "A", list(steps), options)


def b(*steps):
    return "\n" + component(GB, "B", list(steps))


TGA_TPC = [MD + "/m/*.tga", MD + "/m/*.tpc"]
EXTRACT = step("Extract", [MD + "/p.zip"])
CASES = {
    # Fixed: a wildcard Source that matches nothing beside one that does.
    "move_one_miss": a(step("Move", TGA_TPC, OVERRIDE)),
    "copy_two_of_three": a(step("Copy", [MD + "/m/*.tga", MD + "/m/*.tpc", MD + "/m/*.txi"], OVERRIDE)),
    "copy_miss_first": a(step("Copy", [MD + "/m/nope*.tga", MD + "/m/a.tga"], OVERRIDE)),
    "copy_missing_folder": a(step("Copy", [MD + "/m/*.tga", MD + "/q/*.tga"], OVERRIDE)),
    "copy_game_sources": a(step("Copy", [OVERRIDE + "/*.tga", OVERRIDE + "/*.tpc"], MD + "/backup")),
    "option_move": a(step("Choose", [GO]), options=option([step("Move", TGA_TPC, OVERRIDE)])),
    "next_component": a(step("Move", TGA_TPC, OVERRIDE)) + b(step("Copy", [MD + "/c.txt"], OVERRIDE)),
    "extracted_other_component": a(EXTRACT) + b(step("Move", [MD + "/p/*.tga", MD + "/p/*.tpc"], OVERRIDE)),
    "rename_wildcard_miss": a(step("Rename", [MD + "/m/a.tga", MD + "/m/nope*.txt"], "z.tga")),
    # Unchanged controls: nothing matched; a missing literal Rename Source; a loose
    # wildcard next to the component's own Extract (C# VerifyExtractPaths).
    "all_miss": a(step("Copy", [MD + "/m/*.tpc", MD + "/m/*.dds"], OVERRIDE)),
    "rename_literal_miss": a(step("Rename", [MD + "/m/a.tga", MD + "/m/nope.txt"], "z.tga")),
    "extracted_same_component": a(EXTRACT, step("Move", [MD + "/p/*.tga", MD + "/p/*.tpc"], OVERRIDE)),
    # Unchanged known differences: a missing literal Copy Source, an Extract Source.
    "literal_miss": a(step("Copy", [MD + "/m/a.tga", MD + "/m/nope.tga"], OVERRIDE)),
    "extract_wildcard_miss": a(step("Extract", [MD + "/p.zip", MD + "/*.7z"]),
                               step("Move", [MD + "/p/*.tga"], OVERRIDE)),
}

STRICT = "Rust refuses before any write; nothing installed"
# name -> {check: reason}
KNOWN = {
    "literal_miss": {
        "install": "a literal Source that names no file stays refused (fail closed)",
        "plain": STRICT,
        "full_dry/counts": "Rust's dry run skips the component with a DryRunSkipped warning",
        "basic/counts": "as full_dry/counts",
    },
    "extract_wildcard_miss": {
        "install": "every Extract Source must match (deliberate, missing-archives contract)",
        "plain": STRICT,
        "full_dry/decision": "Rust's archive stage reports the step (Missing Required Archives)",
        "full_dry/counts": "as full_dry/decision",
        "basic/decision": "as full_dry/decision",
        "basic/counts": "as full_dry/decision",
    },
    "rename_literal_miss": {
        "install": "both fail; C# renames a.tga first and keeps it, " + STRICT,
    },
    "extracted_same_component": {
        "full_dry/counts": "both refuse; counts differ as in the loose-source contract",
        "basic/counts": "as full_dry/counts",
    },
}


def zip_bytes():
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w") as archive:
        archive.writestr("x.tga", "x tga\n")
    return data.getvalue()


FILES = {"game/chitin.key": "KEY\n", "game/Override/g.tga": "game tga\n", "mods/m/a.tga": "a tga\n",
         "mods/m/b.txi": "b txi\n", "mods/c.txt": "c\n"}


def fixture(root, guide):
    for folder in ("game/Override", "mods/backup", "home"):
        os.makedirs(os.path.join(root, folder))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    with open(os.path.join(root, "mods", "p.zip"), "wb") as handle:
        handle.write(zip_bytes())
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
                rel = os.path.relpath(full, root).replace(os.sep, "/")
                if name.endswith(".zip"):
                    out.append([rel, "<zip>"])
                    continue
                with open(full) as handle:
                    out.append([rel, handle.read()])
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
    if check.endswith("/counts"):
        errors, warnings, _ = actual[check]
        if name in ("extract_wildcard_miss", "extracted_same_component"):
            return errors == 1
        return errors == 0 and warnings == 1
    if check.endswith("/decision"):
        return actual[check] == [False, False]
    return False


def report_view(code, stdout):
    report = json.loads(stdout[stdout.index("{"):])
    return {
        "decision": [code == 0, report.get("success")],
        "counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
    }


def run_one(argv_head, guide, work, label, mode, csharp, extra_env):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, guide)
    common = ["-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "basic": ["--dry-run"]}[mode]
        argv = argv_head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
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
    work = tempfile.mkdtemp(prefix="wildcard-miss-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
            actual, texts = run_case([os.path.abspath(args.bin)], guide, work, "rust", False)
            expected = oracle[name]
            if args.verbose:
                for mode, text in texts.items():
                    print(f"  {name}/{mode}: {text.strip().splitlines()[-1:] if text.strip() else ''}")
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
