#!/usr/bin/env python3
"""No-managed proof: drives the production `modsync`.

The C# CLI's `install` takes `--no-managed` (CLI/ModBuildConverter.cs:673-674): force
a classic install for this run even when settings.json enables managed deployment.
Rust had no such option, so a C# command line with it stopped with a usage error
(exit 2) and installed nothing.

Each case is a disposable mod/game fixture with one TOML guide; each component copies
`mods/<name>.txt` into `Override`:

  good    `a` and `b`, both present;
  broken  `a` and `f`, but `mods/f.txt` is missing.

`settings` cases also write `<XDG_CONFIG_HOME>/ModSync/settings.json` with
`managedDeploymentEnabled: true` and `activeProfileName: "p"`, the C# GUI's
managed-deployment toggle.

Runs (C# also gets `-y` on install, never `--skip-validation`):

  install   `install -i G -s mods -g game --no-checkpoint` plus the case arguments
            (`--no-checkpoint` is left out with `--managed`, which Rust's parser
            refuses beside it):
            exit code zero or not, and every file under `game/` and `mods/` with its
            text afterwards (C#'s `.modsync` folder left out);
  validate  `validate -i G -s mods -g game` plus the case arguments: exit code zero
            or not;
  rootless  `validate -i G` plus the case arguments: exit code zero or not.

`both_flags` also checks that the output names `Cannot combine --managed and
--no-managed.`

The C# install validates first, and its Environment stage needs a HoloPatcher in its
`Resources` folder; the probe copies the reference build into its temporary
directory and adds `Resources/holopatcher`, a shell script that exits 2.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../no-managed-contract.md.
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
GUIDES = {"good": ("a", "b"), "broken": ("a", "f")}
SETTINGS = {"managedDeploymentEnabled": True, "activeProfileName": "p"}
COMBINE = "Cannot combine --managed and --no-managed."

# name -> (mode, guide, case arguments, write settings.json)
CASES = {
    "control": ("install", "good", [], False),
    "no_managed": ("install", "good", ["--no-managed"], False),
    "no_managed_first": ("install", "good", ["--no-managed"], False),
    "no_managed_broken": ("install", "broken", ["--no-managed"], False),
    "settings_no_managed": ("install", "good", ["--no-managed"], True),
    "settings_only": ("install", "good", [], True),
    "both_flags": ("install", "good", ["--managed", "--no-managed"], False),
    "validate_no_managed": ("validate", "good", ["--no-managed"], False),
    "rootless_no_managed": ("rootless", "good", ["--no-managed"], False),
}
# name -> (checks that differ, how Rust must behave instead)
KNOWN = {
    # The C# install validates `f`'s missing file only when it reaches it, so it
    # installs `a` first; Rust refuses before any write (documented).
    "no_managed_broken": (("tree",), "refused, nothing written"),
    # C# follows settings.json into a managed install, which fails here because
    # profile `p` does not exist; the Rust CLI never reads that toggle and installs
    # classic, as with --no-managed (documented).
    "settings_only": (("exit_zero", "tree"), "as reference settings_no_managed"),
}


def fixture(root, guide_name, settings):
    for folder in ("game/Override", "mods", "home", "xdg"):
        os.makedirs(os.path.join(root, folder))
    names = GUIDES[guide_name]
    files = {"game/chitin.key": "KEY\n"}
    files.update({f"mods/{name}.txt": f"{name}\n" for name in names if name != "f"})
    for name, text in files.items():
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    if settings:
        os.makedirs(os.path.join(root, "xdg", "ModSync"))
        with open(os.path.join(root, "xdg", "ModSync", "settings.json"), "w") as handle:
            json.dump(SETTINGS, handle)
    parts = []
    for index, name in enumerate(names, start=1):
        parts.append(f'''[[thisMod]]
Guid = "44444444-5555-6666-7777-8888888888{index:02d}"
Name = "{name}"

[[thisMod.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/{name}.txt"]
Destination = "<<kotorDirectory>>/Override"
''')
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write("\n".join(parts))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    """Every file under game/ and mods/ with its text."""
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


def tree_unchanged(guide_name):
    """The fixture tree as built, before any run."""
    with tempfile.TemporaryDirectory() as root:
        fixture(root, guide_name, False)
        return tree(root)


def run_case(argv_head, case, work, label, csharp, extra_env=None):
    mode, guide_name, extra, settings = CASES[case]
    root = tempfile.mkdtemp(prefix=f"{label}-{case}-", dir=work)
    path, mods, game = fixture(root, guide_name, settings)
    roots = ["-i", path] if mode == "rootless" else ["-i", path, "-s", mods, "-g", game]
    common = extra + roots if case == "no_managed_first" else roots + extra
    if mode == "install":
        # Rust's --managed conflicts with --no-checkpoint at parse time; leave it out
        # where --managed is passed so the case reaches the combination check.
        checkpoint = [] if "--managed" in extra else ["--no-checkpoint"]
        argv = argv_head + ["install"] + common + checkpoint + (["-y"] if csharp else [])
    else:
        argv = argv_head + ["validate"] + common
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    text = done.stdout + done.stderr
    result = {"exit_zero": done.returncode == 0}
    if mode == "install":
        result["tree"] = tree(root)
    if case == "both_flags":
        result["message"] = COMBINE in text
    return result, text


def known_ok(name, check, actual, oracle):
    if name == "no_managed_broken":
        return actual["tree"] == tree_unchanged("broken")
    if name == "settings_only":
        return actual[check] == oracle["settings_no_managed"][check]
    return False


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


def last_line(text):
    lines = [line for line in (text or "").strip().splitlines() if line.strip()]
    return lines[-1] if lines else ""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--verbose", action="store_true", help="print the last output line per run")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="no-managed-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {}
            for name in CASES:
                oracle[name], text = run_case(head, name, work, "csharp", True, env)
                if args.verbose:
                    print(f"  csharp {name}: {last_line(text)!r}")
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name in CASES:
            actual, text = run_case([os.path.abspath(args.bin)], name, work, "rust", False)
            if args.verbose:
                print(f"  rust {name}: {last_line(text)!r}")
            expected = oracle[name]
            for check in sorted(expected):
                same = actual.get(check) == expected[check]
                if check in KNOWN.get(name, ((), ""))[0]:
                    verdict = "known" if not same and known_ok(name, check, actual, oracle) else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                detail = f" | rust={actual.get(check)} | reference={expected[check]}"
                print(f"{verdict:5} {name}/{check}{detail if verdict == 'FAIL' else ''}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
