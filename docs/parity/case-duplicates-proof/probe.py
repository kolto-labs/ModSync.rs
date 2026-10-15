#!/usr/bin/env python3
"""Case duplicates proof: drives the production `modsync`.

C#'s Environment stage (InstallationService.ValidateInstallationEnvironmentAsync,
InstallationService.cs:935-942, :1059-1065) lists every name in the game folder that
differs from another only in case (PathHelper.FindCaseInsensitiveDuplicates, at any
depth) and fails the full report with `You have duplicate files/folders in your
installation directory ...`, a message it sets last, so it replaces the stage's other
messages. Rust's engine scan refused the same trees before any report, so `validate
--full` printed a bare error (JSON: `error`, `errorCount` 0, no stages,
`componentCount` 0) instead of that failed Environment stage.

Each case is a disposable mod/game fixture with one TOML guide; the mod folder holds
`a/x.txt`, the game `chitin.key` and `Override/g.txt`, plus the case's extra files.
Every run (both sides) is under `unshare -Urnm`, where the case's read-only folders are
bind-mounted read-only. Both sides run, on a fresh fixture each:

  full   `validate -i G -s mods -g game --output json --full`;
  text   `validate -i G -s mods -g game --full` (text output);
  dro    `validate ... --output json --full --dry-run-only`;
  basic  `validate ... --output json --dry-run` (no `--full`, so no Environment stage).

Checks:
  full/decision, dro/decision, basic/decision  exit code is zero or not, and `success`;
  full/counts, dro/counts      `errorCount`, `warningCount`, `passedCount`,
                               `componentCount`;
  full/stages                  each stage's name and `passed`;
  full/environment, dro/environment  the Environment stage's first message;
  text/decision                exit code is zero or not;
  text/message                 C#'s duplicate message appears in the output;
  details   (Rust only) the lines Rust lists after that message (paths relative to the
            fixture);
  install   (Rust only) `install -i G -s mods -g game`: exit code is zero or not, the
            last `Error:` line, the `Override` listing and whether an unfinished
            install (`.modsync-*/pending`) was left beside the folders. C#'s install
            runs the full pipeline first (ModBuildConverter.cs:3576-3598) and refuses
            these cases; it is not run here.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy holding a HoloPatcher stub (C#'s Environment stage runs
`holopatcher --install` and expects exit code 2) under `unshare -Urnm` (no network, a
tmpfs over `/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also
rewrites csharp-oracle.json. Without it the recorded results are used. See
../case-duplicates-contract.md.
"""
import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
KD, MD = "<<kotorDirectory>>", "<<modDirectory>>"
GA = "55555555-6666-7777-8888-999999999b01"
OV = KD + "/Override"

STUB = "#!/bin/sh\nexit 2\n"


def guide(steps):
    lines = ["[[thisMod]]", f'Guid = "{GA}"', 'Name = "A"', "IsSelected = true", ""]
    for action, sources, destination in steps:
        lines += ["[[thisMod.Instructions]]", f'Action = "{action}"',
                  f"Source = {json.dumps(sources)}"]
        if destination is not None:
            lines.append(f"Destination = {json.dumps(destination)}")
        lines.append("")
    return "\n".join(lines)


COPY = guide([("Copy", [MD + "/a/x.txt"], OV)])
MISSING = guide([("Extract", [MD + "/m1.zip"], None), ("Copy", [MD + "/m1/f1.txt"], OV)])

DUPLICATES = ("You have duplicate files/folders in your installation directory in a "
              "case-insensitive environment.Please resolve these before continuing. Check "
              "the output window for the specific files to resolve.")
MODS_MSG = ("The Mod directory is not writable! Please ensure administrative privileges or "
            "choose a new mod directory.")
BLOCKED = "Error: Case-colliding "


def detail(path):
    return f"ERROR: <root>/{path} has a duplicate, please resolve before attempting an install."


def refused(kind, path, listing=("A.txt", "a.txt", "g.txt")):
    return [False, f"{BLOCKED}{kind}: <root>/{path}", list(listing), False]


# name -> (extra files, read-only folders, guide, Rust's detail lines, Rust's install
# result). The install result is [exit 0, last stderr line starting "Error:", Override
# listing, an unfinished install (`.modsync-*/pending`) left beside the folders].
CASES = {
    # Fixed: C# fails its Environment stage with the duplicate message; Rust printed a
    # bare error with no stages.
    "game_files": ({"game/Override/a.txt": "1\n", "game/Override/A.txt": "2\n"}, [], COPY,
                   [detail("game/Override/a.txt")], refused("files", "game/Override/a.txt")),
    "game_folders": ({"game/override/b.txt": "b\n"}, [], COPY,
                       [detail("game/override")], refused("directories", "game/override", ["g.txt"])),
    "game_nested": ({"game/Override/sub/t.tga": "1\n", "game/Override/sub/T.TGA": "2\n"}, [], COPY,
                    [detail("game/Override/sub/t.tga")],
                    refused("files", "game/Override/sub/t.tga", ["g.txt", "sub"])),
    # The duplicate message replaces the component and read-only messages, as in C#.
    "game_files_missing_archive": ({"game/Override/a.txt": "1\n", "game/Override/A.txt": "2\n"}, [],
                                   MISSING, [detail("game/Override/a.txt")],
                                   refused("files", "game/Override/a.txt")),
    "game_files_mods_readonly": ({"game/Override/a.txt": "1\n", "game/Override/A.txt": "2\n"},
                                 ["mods"], COPY, [detail("game/Override/a.txt")],
                                 refused("files", "game/Override/a.txt")),
    # Rust's scan stops at the mod folder's duplicate; the report still finds the game's.
    "both_files": ({"game/Override/a.txt": "1\n", "game/Override/A.txt": "2\n",
                    "mods/a/X.TXT": "AX\n"}, [], COPY,
                   [detail("game/Override/a.txt")], refused("files", "mods/a/x.txt")),
    # Known: a duplicate only in the mod folder (C# does not check it).
    "mods_files": ({"mods/a/X.TXT": "AX\n"}, [], COPY, [], refused("files", "mods/a/x.txt", ["g.txt"])),
    # Controls.
    "ok": ({}, [], COPY, [], [True, None, ["g.txt", "x.txt"], False]),
    "ok_readonly_mods": ({}, ["mods"], COPY, [], None),
}

FULL_CHECKS = ("full/decision", "full/counts", "full/stages", "full/environment",
               "text/decision", "dro/decision", "dro/counts", "dro/environment")
MODS_REASON = ("C# checks only the game folder and passes; Rust's engine scan also refuses a "
               "case duplicate in the mod folder (fail closed), before any report")
BASIC_REASON = ("C# runs its duplicate check only in the full pipeline's Environment stage; "
                "Rust's engine scan refuses the tree in every mode (fail closed)")
# name -> {check: reason}
KNOWN = {
    "mods_files": {check: MODS_REASON for check in FULL_CHECKS + ("basic/decision",)
                   if not check.endswith("/environment")},
}
for _name in ("game_files", "game_folders", "game_nested", "game_files_mods_readonly",
              "both_files"):
    KNOWN[_name] = {"basic/decision": BASIC_REASON}


def lower(value):
    """Rust names the duplicate its scan meets second, which depends on the directory
    listing order, so the Rust-only path lines are compared without case."""
    return json.loads(json.dumps(value).lower())


def known_ok(name, check, actual):
    """The Rust side of each known difference is the documented one."""
    if check.endswith("/decision"):
        value = actual[check]
        return (value is False) if check == "text/decision" else value[0] is False
    if check == "full/stages":
        return actual[check] == []
    if check.endswith("/counts"):
        return actual[check] == [0, 0, 0, 0]
    if check.endswith("/environment"):
        return actual[check] is None
    return False


BASE = {"game/chitin.key": "KEY\n", "game/Override/g.txt": "game g\n", "mods/a/x.txt": "ax\n"}


def fixture(root, extra, text):
    os.makedirs(os.path.join(root, "home"))
    for name, content in list(BASE.items()) + list(extra.items()):
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(content)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(text)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def contained(root, readonly, prefix, argv):
    """`argv` under `unshare -Urnm`, with `readonly` folders of `root` bind-mounted
    read-only and `prefix` shell commands run first."""
    mounts = "".join(f"mount --bind {shlex.quote(p)} {shlex.quote(p)} && "
                     f"mount -o remount,bind,ro {shlex.quote(p)} && "
                     for p in (os.path.join(root, r) for r in readonly))
    return ["unshare", "-Urnm", "sh", "-c", prefix + mounts + 'exec "$0" "$@"'] + argv


def run_one(head, prefix, case, work, label, mode, extra_env):
    extra, readonly, text, _, _ = case
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, extra, text)
    argv = head + ["install" if mode == "install" else "validate", "-i", path, "-s", mods, "-g", game]
    argv += {"full": ["--output", "json", "--full"], "text": ["--full"],
             "dro": ["--output", "json", "--full", "--dry-run-only"],
             "basic": ["--output", "json", "--dry-run"], "install": []}[mode]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(contained(root, readonly, prefix, argv), capture_output=True, text=True,
                          env=env, cwd=root, timeout=600, stdin=subprocess.DEVNULL)
    if mode == "text":
        return {"text/decision": done.returncode == 0,
                "text/message": DUPLICATES in done.stdout + done.stderr}, []
    if mode == "install":
        errors = [line.replace(root, "<root>") for line in done.stderr.splitlines()
                  if line.startswith("Error:")]
        pending = any(n.startswith(".modsync-") and os.path.exists(os.path.join(root, n, "pending"))
                      for n in os.listdir(root))
        return {"install": [done.returncode == 0, errors[-1] if errors else None,
                            sorted(os.listdir(os.path.join(game, "Override"))), pending]}, []
    out = done.stdout
    report = json.loads(out[out.index("{"):].replace(json.dumps(root)[1:-1], "<root>"))
    view = {f"{mode}/decision": [done.returncode == 0, report.get("success")]}
    if mode == "basic":
        return view, []
    view[f"{mode}/counts"] = [report.get("errorCount"), report.get("warningCount"),
                              report.get("passedCount"), report.get("componentCount")]
    stages = report.get("stages", [])
    if mode == "full":
        view["full/stages"] = [[s.get("stage"), s.get("passed")] for s in stages]
    environment = next((s for s in stages if s.get("stage") == "Environment"), {})
    messages = environment.get("messages") or []
    view[f"{mode}/environment"] = messages[0] if messages else None
    return view, messages[1:] if mode == "full" else []


def run_case(head, prefix, case, work, label, extra_env=None, install=False):
    out, details = {}, []
    for mode in ("full", "text", "dro", "basic") + (("install",) if install else ()):
        view, found = run_one(head, prefix, case, work, label, mode, extra_env)
        out.update(view)
        details += found
    return out, details


def stub_reference(reference, work):
    """A copy of the reference build with a HoloPatcher stub in its Resources."""
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    stub = os.path.join(copy_dir, "Resources", "holopatcher")
    with open(stub, "w") as handle:
        handle.write(STUB)
    os.chmod(stub, 0o755)
    return copy_dir


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="case-duplicates-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            head = ["dotnet", os.path.join(reference, "ModSync.Core.dll")]
            prefix = "if [ -d /Override ]; then mount -t tmpfs none /Override; fi; "
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_case(head, prefix, case, work, "csharp", env)[0]
                      for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual, details = run_case([os.path.abspath(args.bin)], "", case, work, "rust",
                                       install=case[4] is not None)
            expected = oracle[name]
            extra = ["details"] + (["install"] if case[4] is not None else [])
            for check in sorted(expected) + extra:
                if check == "details":
                    same = lower(details) == lower(case[3])
                    detail_text = f" | rust={details} | expected={case[3]}"
                elif check == "install":
                    same = lower(actual[check]) == lower(case[4])
                    detail_text = f" | rust={actual[check]} | expected={case[4]}"
                else:
                    same = actual[check] == expected[check]
                    detail_text = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    verdict = "known" if not same and known_ok(name, check, actual) else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                print(f"{verdict:5} {name}/{check}{detail_text if verdict == 'FAIL' else ''}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
