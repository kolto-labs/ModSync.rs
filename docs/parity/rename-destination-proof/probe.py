#!/usr/bin/env python3
"""Rename Destination proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide in which component `A`
has a Rename step and component `B` copies `b/x.txt` into `Override` and `b.txt` into
the game folder. C# `Instruction.RenameFile` combines the Destination with the
source's folder as a file name, so its install fails a placeholder path (no folder
`<<kotorDirectory>>` there) while its dry run passes it; Rust resolved such a
Destination as a path and moved the file there. Both sides run, on a fresh copy each,
with `--use-file-selection`:

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
the recorded results are used. See ../rename-destination-contract.md.
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
KD, MD, GD = "<<kotorDirectory>>", "<<modDirectory>>", "<<gameDirectory>>"
GA = "77777777-8888-9999-aaaa-ddddddddddd1"
GB = "77777777-8888-9999-aaaa-ddddddddddd2"
OPTION = "77777777-8888-9999-aaaa-ddddddddddd3"
UNSELECTED = "77777777-8888-9999-aaaa-ddddddddddd9"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def copy(source, destination=KD + "/Override"):
    return step("Copy", [MD + "/" + source], destination, "Overwrite = true")


def rename(source, destination, extra=""):
    return step("Rename", [source], destination, extra)


def component(guid, name, steps, option_steps=None):
    """A selected component; `option_steps` gives it an unselected option `O`."""
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    if option_steps is not None:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "O"', "IsSelected = false", ""]
        for s in option_steps:
            lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return "\n".join(lines)


def guide(*a_steps, option_steps=None):
    """`A` with the given steps, then `B`."""
    return (component(GA, "A", a_steps, option_steps) + "\n"
            + component(GB, "B", [copy("b/x.txt"), copy("b.txt", KD)]))


X = MD + "/a/x.txt"
CASES = {
    # A placeholder path as the Rename Destination: C# install fails, Rust installed.
    "placeholder_game": guide(rename(X, KD + "/Override/z.txt")),
    "placeholder_mod": guide(rename(X, MD + "/a/z.txt")),
    "placeholder_legacy": guide(rename(X, GD + "/Override/z.txt")),
    # The same after a Copy: C# has written Override/x.txt when its Rename fails.
    "placeholder_after_copy": guide(copy("a/x.txt"),
                                    rename(KD + "/Override/x.txt", KD + "/Override/z.txt", "Overwrite = true")),
    # Controls: a step excluded by its Dependencies, a step of an unselected option
    # (`A` chooses it, so it is checked but not run), and a plain file name.
    "placeholder_excluded": guide(rename(X, KD + "/Override/z.txt", f'Dependencies = ["{UNSELECTED}"]')),
    "placeholder_unselected_option": guide(step("Choose", [OPTION]),
                                           option_steps=[rename(X, KD + "/Override/z.txt")]),
    "file_name": guide(rename(X, "z.txt")),
    # Unchanged (known): a relative path with a folder.
    "subfolder": guide(rename(X, "sub/z.txt")),
    "parent": guide(rename(X, "../z.txt")),
}

ALL = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision", "install", "plain")
REPORTS = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision")
PLACEHOLDER = ("Rust refuses a placeholder-path Rename Destination in every report too (fail closed); "
               "the C# dry run renames to a virtual path below the source folder and passes, and only "
               "its install fails")
PARTIAL = ("C# fails the install after writing A's Copy (no rollback); Rust refuses before any write")
FOLDER = ("Rust refuses a Rename Destination with a folder part (deliberate); C# renames into "
          "the source's subfolder, or out of it with `..`")
# name -> {check: reason}
KNOWN = {name: {check: PLACEHOLDER for check in REPORTS}
         for name in ("placeholder_game", "placeholder_mod", "placeholder_legacy", "placeholder_after_copy")}
KNOWN["placeholder_after_copy"]["install"] = PARTIAL
KNOWN["subfolder"] = {check: FOLDER for check in ALL}
KNOWN["parent"] = {check: FOLDER for check in ALL}


def known_ok(name, actual, expected):
    """The Rust side of each known difference is the documented refusal before any write."""
    return (not actual["full_dry/decision"][0] and not actual["basic/decision"][0]
            and not actual["plain"] and actual["install"] == [False, BASELINE])


FILES = {"game/chitin.key": "KEY\n", "mods/a/x.txt": "from A\n", "mods/a/sub/keep.txt": "keep\n",
         "mods/b/x.txt": "from B\n", "mods/b.txt": "b\n"}
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
    work = tempfile.mkdtemp(prefix="rename-destination-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
