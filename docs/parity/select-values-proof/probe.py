#!/usr/bin/env python3
"""Select values proof: drives the production `modsync`.

The C# CLI declares `--select` on `validate` and `install` as a sequence option
(`IEnumerable<string> Select`, CLI/ModBuildConverter.cs:582-583, :619-620), so
CommandLineParser reads every value after the flag up to the next option:
`--select category:Bugfix category:Immersion` selects both categories. Rust took one
value per flag, so the same command line stopped with a usage error (exit 2) and
validated or installed nothing.

Each case is a disposable mod/game fixture with one TOML guide of six components,
`a` to `f`; each copies `mods/<name>.txt` into `Override`, but `mods/f.txt` is
missing, so selecting `f` fails validation. Categories and tiers:

  a  Graphics Improvement          1 - Essential
  b  Bugfix, Graphics Improvement  2 - Recommended
  c  Bugfix                        3 - Suggested
  d  Immersion                     4 - Optional
  e  Immersion                     Custom Tier
  f  Broken                        4 - Optional

Every run gets the case's selection arguments (after the roots unless the case puts
them first) and a fresh copy of the fixture:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`;
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry  exit code is zero or not, and `success` (null when stdout holds no JSON);
  install   exit code is zero or not, and every file under `game/` and `mods/` with
            its text afterwards (C#'s `.modsync` folder left out);
  plain     Rust's plain `validate` decides as C# `install`.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources`
folder; the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../select-values-contract.md.
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
COMPONENTS = (
    ("a", '["Graphics Improvement"]', "1 - Essential"),
    ("b", '["Bugfix", "Graphics Improvement"]', "2 - Recommended"),
    ("c", '["Bugfix"]', "3 - Suggested"),
    ("d", '["Immersion"]', "4 - Optional"),
    ("e", '["Immersion"]', "Custom Tier"),
    ("f", '["Broken"]', "4 - Optional"),
)


def guide():
    parts = []
    for index, (name, category, tier) in enumerate(COMPONENTS, start=1):
        parts.append(f'''[[thisMod]]
Guid = "33333333-4444-5555-6666-7777777777{index:02d}"
Name = "{name}"
Category = {category}
Tier = "{tier}"
IsSelected = false

[[thisMod.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/{name}.txt"]
Destination = "<<kotorDirectory>>/Override"
''')
    return "\n".join(parts)


# name -> (selection arguments, put them before the roots)
CASES = {
    # Match before and after this slice.
    "one_value": (["--select", "category:Bugfix"], False),
    "one_value_broken": (["--select", "category:Broken"], False),
    "no_value": (["--select", "--use-file-selection"], False),
    "no_match": (["--select", "category:Nope", "tier:Nope"], False),
    # Several values after one flag.
    "two_categories": (["--select", "category:Bugfix", "category:Immersion"], False),
    "category_and_tier": (["--select", "category:Graphics Improvement", "tier:1 - Essential"], False),
    "two_tiers": (["--select", "tier:custom tier", "tier:Recommended"], False),
    "values_then_flag": (["--select", "category:Bugfix", "category:Immersion", "--use-file-selection"], False),
    "values_before_roots": (["--select", "category:Bugfix", "category:Immersion"], True),
    "unknown_type_in_list": (["--select", "category:Bugfix", "name:x"], False),
    "broken_in_list": (["--select", "category:Bugfix", "category:Broken"], False),
    # Known differences (see KNOWN).
    "repeated_flag": (["--select", "category:Bugfix", "--select", "category:Immersion"], False),
    "equals_then_value": (["--select=category:Bugfix", "category:Immersion"], False),
    "name_in_list": (["--select", "category:Bugfix", "a"], False),
}
CHECKS = ("full_dry", "install", "plain")
# name -> (checks that differ, how Rust must behave instead)
KNOWN = {
    # C# refuses a repeated --select (CommandLineParser: defined multiple times);
    # Rust keeps accepting it, as before: same result as C# `two_categories`.
    "repeated_flag": (CHECKS, "as reference two_categories"),
    # C# reads the next value after --select=...; clap does not, so Rust stops with
    # a usage error before any write.
    "equals_then_value": (CHECKS, "refused, nothing written"),
    # The C# install validates `f`'s missing file only when it reaches it, so it
    # installs b and c first; Rust refuses before any write (documented).
    "broken_in_list": (("install",), "refused, nothing written"),
    # C# ignores a value without a colon (a warning); Rust selects the component
    # of that name (its documented name/GUID selectors), so `a` installs too.
    "name_in_list": (("install",), "a, b and c installed"),
}


def fixture(root):
    for folder in ("game/Override", "mods", "home"):
        os.makedirs(os.path.join(root, folder))
    files = {"game/chitin.key": "KEY\n"}
    files.update({f"mods/{name}.txt": f"{name}\n" for name, _, _ in COMPONENTS if name != "f"})
    for name, text in files.items():
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide())
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


def run_one(argv_head, case, work, label, mode, csharp, extra_env):
    selection, first = CASES[case]
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root)
    roots = ["-i", path, "-s", mods, "-g", game]
    common = selection + roots if first else roots + selection
    if mode == "install":
        argv = argv_head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    elif mode == "plain":
        argv = argv_head + ["validate"] + common
    else:
        argv = argv_head + ["validate"] + common + ["--output", "json", "--full", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    text = done.stdout + done.stderr
    if mode == "install":
        return [done.returncode == 0, tree(root)], text
    if mode == "plain":
        return done.returncode == 0, text
    try:
        success = json.loads(done.stdout[done.stdout.index("{"):]).get("success")
    except ValueError:
        success = None
    return [done.returncode == 0, success], text


def run_case(argv_head, case, work, label, csharp, extra_env=None):
    out, texts = {}, {}
    for mode in ("full_dry", "install") + (() if csharp else ("plain",)):
        out[mode], texts[mode] = run_one(argv_head, case, work, label, mode, csharp, extra_env)
    return out, texts


def known_ok(name, check, actual, oracle):
    if name == "repeated_flag":
        reference = oracle["two_categories"]
        expected = reference["install"][0] if check == "plain" else reference[check]
        return actual[check] == expected
    if name in ("equals_then_value", "broken_in_list"):
        if check == "install":
            return actual["install"] == [False, tree_unchanged()]
        return actual[check] in (False, [False, None])
    if name == "name_in_list":
        installed = [path for path, _ in actual["install"][1] if path.startswith("game/Override/")]
        return actual["install"][0] and installed == ["game/Override/a.txt", "game/Override/b.txt",
                                                      "game/Override/c.txt"]
    return False


def tree_unchanged():
    """The fixture tree as built, before any run."""
    with tempfile.TemporaryDirectory() as root:
        fixture(root)
        return tree(root)


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
    work = tempfile.mkdtemp(prefix="select-values-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {}
            for name in CASES:
                oracle[name], texts = run_case(head, name, work, "csharp", True, env)
                if args.verbose:
                    print(f"  csharp {name}/install: {last_line(texts['install'])!r}")
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name in CASES:
            actual, texts = run_case([os.path.abspath(args.bin)], name, work, "rust", False)
            expected = oracle[name]
            if args.verbose:
                for mode in ("plain", "install"):
                    print(f"  rust {name}/{mode}: {last_line(texts[mode])!r}")
            for check in CHECKS:
                if check == "plain":
                    same = actual["plain"] == expected["install"][0]
                    detail = f" | rust={actual['plain']} | reference install={expected['install'][0]}"
                else:
                    same = actual[check] == expected[check]
                    detail = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, ((), ""))[0]:
                    verdict = "known" if not same and known_ok(name, check, actual, oracle) else "FAIL"
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
