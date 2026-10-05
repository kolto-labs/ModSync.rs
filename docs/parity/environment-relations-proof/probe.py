#!/usr/bin/env python3
"""Environment relations proof: drives the production `modsync`.

C#'s Environment stage (InstallationService.ValidateInstallationEnvironmentAsync,
InstallationService.cs:967-1003, :1051-1057) checks each selected component's
Restrictions (a selected component or immediate option they name) and Dependencies
(one they name that is not selected), logs `Cannot install '<A>' due to '<C>' being
selected for install.` (or `... not being selected for install.`) and fails the full
report with `Some components failed to validate. ...`, one error, and the report ends.
Rust's Environment stage passed such a guide and its Conflicts, InstallOrder and
FomodConfiguration stages (and, with --dry-run-only, its dry run) each failed it, so
the report had three to five errors and named no Environment failure.

Each case is a disposable mod/game fixture with one TOML guide; the mod folder holds
`a/x.txt`, `a/y.txt`, `a/z.txt`, the game `chitin.key` and `Override/g.txt`. Every run
(both sides) is under `unshare -Urnm`, where a case's read-only folders are
bind-mounted read-only. Both sides run, on a fresh fixture each, with the case's
selection arguments:

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
  text/message                 C#'s component message appears in the output;
  details   (Rust only) the lines Rust lists in the --full Environment stage after the
            first message;
  install   (Rust only) `install -i G -s mods -g game` with the selection arguments:
            exit code is zero or not, the last `Error:` line, the `Override` listing
            and whether an unfinished install (`.modsync-*/pending`) was left. Expected
            unchanged: refused before any write. C#'s install runs the full pipeline
            first (ModBuildConverter.cs:3576-3598) and refuses the same selections;
            it is not run here.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live from a copy holding a HoloPatcher stub (C#'s Environment stage runs
`holopatcher --install` and expects exit code 2) under `unshare -Urnm` (no network, a
tmpfs over `/Override` when it exists) with DOTNET_ROLL_FORWARD=Major; --record also
rewrites csharp-oracle.json. Without it the recorded results are used. See
../environment-relations-contract.md.
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
OV = KD + "/Override"

STUB = "#!/bin/sh\nexit 2\n"


def gid(n):
    return f"55555555-6666-7777-8888-99999999e{n:03d}"


def component(n, name, selected=True, deps=(), restr=(), category=None, steps=None,
              options=()):
    lines = ["[[thisMod]]", f'Guid = "{gid(n)}"', f'Name = "{name}"',
             f"IsSelected = {str(selected).lower()}"]
    if category:
        lines.append(f'Category = ["{category}"]')
    if deps:
        lines.append(f"Dependencies = {json.dumps([gid(d) for d in deps])}")
    if restr:
        lines.append(f"Restrictions = {json.dumps([gid(r) for r in restr])}")
    lines.append("")
    for action, sources, destination in steps or [("Copy", [f"{MD}/a/x.txt"], OV)]:
        lines += ["[[thisMod.Instructions]]", f'Action = "{action}"',
                  f"Source = {json.dumps(sources)}"]
        if destination is not None:
            lines.append(f"Destination = {json.dumps(destination)}")
        lines.append("")
    for n_option, option_name in options:
        lines += ["[[thisMod.Options]]", f'Guid = "{gid(n_option)}"', f'Name = "{option_name}"',
                  "IsSelected = true", "", "[[thisMod.Options.Instructions]]", 'Action = "Copy"',
                  f'Source = ["{MD}/a/y.txt"]', f'Destination = "{OV}"', ""]
    return "\n".join(lines)


COPY_Y = [("Copy", [f"{MD}/a/y.txt"], OV)]
COPY_Z = [("Copy", [f"{MD}/a/z.txt"], OV)]
MISSING = [("Extract", [f"{MD}/m1.zip"], None), ("Copy", [f"{MD}/m1/f1.txt"], OV)]

FAILED = "Some components failed to validate. Check the output/console window for details."
FAILED_FULL = (FAILED + "\nIf you are seeing this as an end user you most likely need to "
               "whitelist ModSync and HoloPatcher in your antivirus, or download the missing mods.")


def cannot(a, c, negated=False):
    return (f"ERROR: Cannot install '{a}' due to '{c}' "
            f"{'not ' if negated else ''}being selected for install.")


def refused(message, listing=("g.txt",)):
    return [False, f"Error: Installation blocked before any write: FOMOD configuration check "
            f"failed: {message}", list(listing), False]


RESTRICT_C = f"A conflicts with {gid(3)}"

# name -> (guide, selection arguments, read-only folders, Rust's detail lines, Rust's
# install result). The install result is [exit 0, last stderr line starting "Error:",
# Override listing, an unfinished install (`.modsync-*/pending`) left beside the
# folders].
CASES = {
    # Fixed: C#'s Environment stage fails with its component message; Rust's passed
    # and three later stages failed.
    "restrict_all": (component(1, "A", restr=[3]) + component(2, "B", steps=COPY_Y)
                     + component(3, "C", steps=COPY_Z), [], [],
                     [cannot("A", "C")], refused(RESTRICT_C)),
    "restrict_select": (component(1, "A", restr=[3], category="X")
                        + component(2, "B", category="Y", steps=COPY_Y)
                        + component(3, "C", category="X", steps=COPY_Z),
                        ["--select", "category:X"], [],
                        [cannot("A", "C")], refused(RESTRICT_C)),
    "restrict_mutual": (component(1, "A", restr=[3]) + component(3, "C", restr=[1], steps=COPY_Z),
                        [], [], [cannot("A", "C"), cannot("C", "A")], refused(RESTRICT_C)),
    # The component message replaces a missing archive's and a read-only folder's.
    "restrict_missing_archive": (component(1, "A", restr=[3], steps=MISSING)
                                 + component(3, "C", steps=COPY_Z), [], [],
                                 [cannot("A", "C")], None),
    "restrict_readonly_mods": (component(1, "A", restr=[3]) + component(3, "C", steps=COPY_Z),
                               [], ["mods"], [cannot("A", "C")], None),
    # Known: the selections differ before the Environment stage.
    "depends_select": (component(1, "A", deps=[2], category="X")
                       + component(2, "B", category="Y", steps=COPY_Y),
                       ["--select", "category:X"], [], [],
                       [True, None, ["g.txt", "x.txt", "y.txt"], False]),
    "depends_file_selection": (component(1, "A", deps=[2])
                               + component(2, "B", selected=False, steps=COPY_Y),
                               ["--use-file-selection"], [],
                               [cannot("A", "B", negated=True)],
                               refused(f"A requires selected component {gid(2)}")),
    "restrict_option": (component(1, "A", restr=[5])
                        + component(3, "C", steps=COPY_Z, options=[(5, "O")]), [], [],
                        [cannot("A", "O")], refused(f"A conflicts with {gid(5)}")),
    # Controls.
    "restrict_unknown": (component(1, "A", restr=[9]) + component(3, "C", steps=COPY_Z),
                         [], [], [], [True, None, ["g.txt", "x.txt", "z.txt"], False]),
    "ok": (component(1, "A", deps=[2]) + component(2, "B", steps=COPY_Y), [], [], [],
           [True, None, ["g.txt", "x.txt", "y.txt"], False]),
}

ENV_FAILED = [1, 0, 0]
SELECT_REASON = ("C#'s --select filters select no Dependency, so its Environment stage fails "
                 "A; Rust's --select adds the dependency closure (a documented Rust selection "
                 "policy), so B is selected and every stage passes")
FILE_REASON = ("C#'s loader (AttemptFixes, AutoFixComponentIssues) selects every Dependency "
               "and passes; Rust does not reproduce the loader's fixes (documented), so B "
               "stays unselected and its Environment stage fails A as C# does for that "
               "selection")
OPTION_REASON = ("C#'s loader (AttemptFixes) deselects an option a selected component "
                 "restricts and passes; Rust keeps it selected, so its Environment stage "
                 "fails A (fail closed, documented)")
BASIC_REASON = ("C#'s basic report has no Environment or Conflicts stage and its dry run does "
                "not check Restrictions, so it passes; Rust's dry run refuses a selected "
                "component whose Restriction is selected (fail closed, unchanged)")
# name -> {check: (reason, Rust's documented result)}
KNOWN = {
    "depends_select": {
        "full/decision": (SELECT_REASON, [True, True]),
        "full/counts": (SELECT_REASON, [0, 0, 5, 2]),
        "full/stages": (SELECT_REASON, [["Environment", True], ["Conflicts", True],
                                        ["InstallOrder", True], ["ComponentValidation", True],
                                        ["FomodConfiguration", True]]),
        "full/environment": (SELECT_REASON, None),
        "text/decision": (SELECT_REASON, True),
        "text/message": (SELECT_REASON, False),
        "dro/decision": (SELECT_REASON, [True, True]),
        "dro/counts": (SELECT_REASON, [0, 0, 5, 2]),
        "dro/environment": (SELECT_REASON, None),
    },
    "depends_file_selection": {
        "full/decision": (FILE_REASON, [False, False]),
        "full/counts": (FILE_REASON, [1, 0, 0, 1]),
        "full/stages": (FILE_REASON, [["Environment", False]]),
        "full/environment": (FILE_REASON, "ERROR: " + FAILED_FULL),
        "text/decision": (FILE_REASON, False),
        "text/message": (FILE_REASON, True),
        "dro/decision": (FILE_REASON, [False, False]),
        "dro/counts": (FILE_REASON, [1, 0, 0, 1]),
        "dro/environment": (FILE_REASON, "ERROR: " + FAILED_FULL),
        "basic/decision": (FILE_REASON, [False, False]),
    },
    "restrict_option": {
        "full/decision": (OPTION_REASON, [False, False]),
        "full/counts": (OPTION_REASON, [1, 0, 0, 2]),
        "full/stages": (OPTION_REASON, [["Environment", False]]),
        "full/environment": (OPTION_REASON, "ERROR: " + FAILED_FULL),
        "text/decision": (OPTION_REASON, False),
        "text/message": (OPTION_REASON, True),
        "dro/decision": (OPTION_REASON, [False, False]),
        "dro/counts": (OPTION_REASON, [1, 0, 0, 2]),
        "dro/environment": (OPTION_REASON, "ERROR: " + FAILED_FULL),
        "basic/decision": (OPTION_REASON, [False, False]),
    },
}

for _name in ("restrict_all", "restrict_select", "restrict_mutual", "restrict_readonly_mods"):
    KNOWN[_name] = {"basic/decision": (BASIC_REASON, [False, False])}

BASE = {"game/chitin.key": "KEY\n", "game/Override/g.txt": "game g\n", "mods/a/x.txt": "ax\n",
        "mods/a/y.txt": "ay\n", "mods/a/z.txt": "az\n"}


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
    text, selection, readonly, _, _ = case
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, text)
    argv = head + ["install" if mode == "install" else "validate", "-i", path, "-s", mods, "-g", game]
    argv += {"full": ["--output", "json", "--full"], "text": ["--full"],
             "dro": ["--output", "json", "--full", "--dry-run-only"],
             "basic": ["--output", "json", "--dry-run"], "install": []}[mode]
    argv += selection
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               XDG_DATA_HOME=os.path.join(root, "xdg-data"), **(extra_env or {}))
    done = subprocess.run(contained(root, readonly, prefix, argv), capture_output=True, text=True,
                          env=env, cwd=root, timeout=600, stdin=subprocess.DEVNULL)
    if mode == "text":
        return {"text/decision": done.returncode == 0,
                "text/message": FAILED in done.stdout + done.stderr}, []
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
    work = tempfile.mkdtemp(prefix="environment-relations-proof-",
                            dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
                    same = details == case[3]
                    detail_text = f" | rust={details} | expected={case[3]}"
                elif check == "install":
                    same = actual[check] == case[4]
                    detail_text = f" | rust={actual[check]} | expected={case[4]}"
                else:
                    same = actual[check] == expected[check]
                    detail_text = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    documented = KNOWN[name][check][1]
                    verdict = "known" if not same and actual[check] == documented else "FAIL"
                    if verdict == "FAIL":
                        detail_text += f" | documented={documented}"
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
