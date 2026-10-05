#!/usr/bin/env python3
"""Writable roots proof: drives the production `modsync`.

C#'s Environment stage (InstallationService.ValidateInstallationEnvironmentAsync,
InstallationService.cs:946-966, :1029-1043) creates and deletes a file in the game
folder and in the mod folder (UtilityHelper.IsDirectoryWritable, Utility.cs:156-195)
and fails the full report when it cannot: `The Install directory is not writable!
...` or, when the mod folder fails too, `The Mod directory is not writable! ...`.
Rust's Environment stage did not check, so `validate --full` passed (exit 0) a guide
whose game or mod folder is read-only, and `install` took its recovery snapshot and
then stopped at the first write, leaving that snapshot to restore.

Each case is a disposable mod/game fixture with one TOML guide; the mod folder holds
`a/x.txt`, the game `chitin.key` and `Override/g.txt`. Every run (both sides) is under
`unshare -Urnm`, where the case's folders are bind-mounted read-only, so even the
namespace root cannot write to them. Both sides run, on a fresh fixture each:

  full   `validate -i G -s mods -g game --output json --full`;
  text   `validate -i G -s mods -g game --full` (text output);
  dro    `validate ... --output json --full --dry-run-only`.

Checks:
  full/decision, dro/decision  exit code is zero or not, and `success`;
  full/counts, dro/counts      `errorCount`, `warningCount`, `passedCount`;
  full/stages                  each stage's name and `passed`;
  full/environment, dro/environment  the Environment stage's first message;
  text/decision                exit code is zero or not;
  details   (Rust only) the lines Rust lists after that message;
  install   (Rust only) `install -i G -s mods -g game`: exit code is zero or not,
            the error line, the Override listing and whether an unfinished
            install (`.modsync-*/pending`, a snapshot to restore) was left beside
            the folders. C#'s install runs the full pipeline first
            (ModBuildConverter.cs:3576-3598), so it refuses these
            cases before installing; it is not run here because its confirmation
            callback then tries chmod, chown and sudo on the folder.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy holding a HoloPatcher stub (C#'s Environment stage runs
`holopatcher --install` and expects exit code 2) under `unshare -Urnm` (no network, a
tmpfs over `/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also
rewrites csharp-oracle.json. Without it the recorded results are used. See
../writable-roots-contract.md.
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
GA = "55555555-6666-7777-8888-999999999a01"
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

GAME_MSG = ("The Install directory is not writable! Please ensure administrative privileges "
            "or reinstall KOTOR to a directory with write access.")
MODS_MSG = ("The Mod directory is not writable! Please ensure administrative privileges or "
            "choose a new mod directory.")
FAILED = ("ERROR: Some components failed to validate. Check the output/console window for "
          "details.\nIf you are seeing this as an end user you most likely need to whitelist "
          "ModSync and HoloPatcher in your antivirus, or download the missing mods.")
BLOCKED = "Error: Installation blocked before any write: "


def refused(message):
    return [False, BLOCKED + message, ["g.txt"], False]


# name -> (read-only folders, guide, Rust's detail lines, Rust's install result)
# The install result is [exit 0, last stderr line starting "Error:", Override
# listing, an unfinished install (`.modsync-*/pending`) left beside the folders].
CASES = {
    # Fixed: C# fails its Environment stage; Rust passed.
    "game_readonly": (["game"], COPY, [], refused(GAME_MSG)),
    "mods_readonly": (["mods"], COPY, [], refused(MODS_MSG)),
    "both_readonly": (["mods", "game"], COPY, [], refused(MODS_MSG)),
    # C#'s component message wins over a read-only folder (--dry-run-only); the full
    # report is a known difference (below).
    "game_readonly_missing_archive": (["game"], MISSING,
                                      ["ERROR: A: Missing Required Archives: [<<modDirectory>>/m1.zip]"],
                                      refused(GAME_MSG)),
    # Controls: unchanged.
    "ok": ([], COPY, [], [True, None, ["g.txt", "x.txt"], False]),
    "override_readonly": (["game/Override"], COPY, [], None),
}

# name -> {check: reason}
KNOWN = {
    "game_readonly_missing_archive": {
        "full/environment": "C#'s Environment stage also runs the component checks, and "
                            "their message replaces the read-only one; Rust's full report "
                            "runs them later, in its ComponentValidation stage, so its "
                            "Environment stage gives the read-only message",
    },
}


def known_ok(name, check, actual):
    """The Rust side of each known difference is the documented one."""
    if check == "full/environment":
        return actual[check] == "ERROR: " + GAME_MSG
    return False


BASE = {"game/chitin.key": "KEY\n", "game/Override/g.txt": "game g\n", "mods/a/x.txt": "ax\n"}


def fixture(root, text):
    os.makedirs(os.path.join(root, "home"))
    for name, content in BASE.items():
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
    readonly, text, _, _ = case
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, text)
    argv = head + ["install" if mode == "install" else "validate", "-i", path, "-s", mods, "-g", game]
    argv += {"full": ["--output", "json", "--full"], "text": ["--full"],
             "dro": ["--output", "json", "--full", "--dry-run-only"], "install": []}[mode]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(contained(root, readonly, prefix, argv), capture_output=True, text=True,
                          env=env, cwd=root, timeout=600, stdin=subprocess.DEVNULL)
    if mode == "text":
        return {"text/decision": done.returncode == 0}, []
    if mode == "install":
        errors = [line for line in done.stderr.splitlines() if line.startswith("Error:")]
        pending = any(n.startswith(".modsync-") and os.path.exists(os.path.join(root, n, "pending"))
                      for n in os.listdir(root))
        return {"install": [done.returncode == 0, errors[-1] if errors else None,
                            sorted(os.listdir(os.path.join(game, "Override"))), pending]}, []
    out = done.stdout
    report = json.loads(out[out.index("{"):].replace(json.dumps(root)[1:-1], "<root>"))
    view = {
        f"{mode}/decision": [done.returncode == 0, report.get("success")],
        f"{mode}/counts": [report.get("errorCount"), report.get("warningCount"), report.get("passedCount")],
    }
    stages = report.get("stages", [])
    if mode == "full":
        view["full/stages"] = [[s.get("stage"), s.get("passed")] for s in stages]
    environment = next((s for s in stages if s.get("stage") == "Environment"), {})
    messages = environment.get("messages") or []
    view[f"{mode}/environment"] = messages[0] if messages else None
    return view, messages[1:] if mode == "dro" else []


def run_case(head, prefix, case, work, label, extra_env=None, install=False):
    out, details = {}, []
    for mode in ("full", "text", "dro") + (("install",) if install else ()):
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
    work = tempfile.mkdtemp(prefix="writable-roots-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
                                       install=case[3] is not None)
            expected = oracle[name]
            extra = ["details"] + (["install"] if case[3] is not None else [])
            for check in sorted(expected) + extra:
                if check == "details":
                    same = details == case[2]
                    detail = f" | rust={details} | expected={case[2]}"
                elif check == "install":
                    same = actual[check] == case[3]
                    detail = f" | rust={actual[check]} | expected={case[3]}"
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
