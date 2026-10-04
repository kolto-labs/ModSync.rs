#!/usr/bin/env python3
"""Plain report simulation proof: drives the production `modsync validate` without a dry run.

Rust has two routes for `validate` with both roots and no `--dry-run`: the plain
command (no output flag, or `--json`) simulates every step and stops at the first
failure, while the report (`--output json`, `--output text`, `--validation-report`,
`--errors-only`, `--full`) ran only the archive/destination and FOMOD stages, so it
passed a guide whose Copy, Move, Patcher, Execute, strict Delete, Rename or CleanList
source is missing. After this slice the report runs the same simulation once every
stage has passed, and a failure fails the `ComponentValidation` stage.

The pinned C# CLI fails every case here: its basic `validate` never sets the mod
directory (ModBuildConverter.cs:3137-3143), so the FOMOD stage fails
(InstallationValidationPipeline.cs:440-449), and a Patcher step's
`<<kotorDirectory>>` Destination throws `'path' cannot be null or whitespace`
(ComponentValidation.cs:331-341, PathHelper.cs:441-446). `--full` fails its
environment stage without a packaged HoloPatcher. So C# agrees on every refusal, and
a passing Rust control is a known difference (a C# accident).

Each case is a disposable mod/game fixture plus a TOML guide. `mods/` holds `a.txt` to
`d.txt`; `game/` holds `chitin.key` and `Override/b.txt`. A present patcher is
`mods/patch/TSLPatcher.exe` with `tslpatchdata/changes.ini` installing `tex.tga` into
Override. Every run gets `--use-file-selection` (so the guide's `IsSelected` flags
hold) and a fresh copy of the fixture.

Checks per case:
  decision  `validate -i G -s mods -g game --output json` (plus the case's flags): the
            exit code is zero or not, and the report's `success`; compared with C#;
  modes     Rust only: plain `validate` (no output flag), `--output text`,
            `--validation-report` and `--output json` (each plus the case's flags,
            except that plain never gets `--full`) all decide alike;
  report    Rust only: the JSON report's `errorCount`, and the `ComponentValidation`
            stage's `passed` and messages, as expected for the case;
  wording   Rust only, for a failing simulation (every case but the controls): its
            `ERROR:` line is plain `validate`'s error text.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -rn` (no network) with DOTNET_ROLL_FORWARD=Major; --record
also rewrites csharp-oracle.json. Without it the recorded results are used. The
recorded results also keep C#'s reason (the report's `error`, or its failed stages'
messages) and its text-mode exit code, for the contract.
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


def step(action, sources, destination=None, overwrite=None, depends=None):
    return {"action": action, "sources": sources, "destination": destination, "overwrite": overwrite,
            "depends": depends}


def copy(name, action="Copy", **extra):
    return step(action, [f"{MD}/{name}"], OV, **extra)


def patcher(**extra):
    return step("Patcher", [EXE], KD, **extra)


def component(key, steps, selected=True, options=()):
    return {"key": key, "steps": steps, "selected": selected, "options": list(options)}


def toml_steps(path, steps):
    lines = []
    for s in steps:
        lines += ["", f"[[{path}.Instructions]]", f"Action = {json.dumps(s['action'])}",
                  "Source = [" + ", ".join(json.dumps(x) for x in s["sources"]) + "]"]
        if s["destination"]:
            lines.append(f"Destination = {json.dumps(s['destination'])}")
        if s["overwrite"] is not None:
            lines.append(f"Overwrite = {'true' if s['overwrite'] else 'false'}")
        if s["depends"]:
            lines.append(f"Dependencies = [{json.dumps(G[s['depends']])}]")
    return lines


def guide(components):
    lines = []
    for c in components:
        lines += ["[[thisMod]]", f'Guid = "{G[c["key"]]}"', f'Name = "{c["key"].upper()}"',
                  f"IsSelected = {'true' if c['selected'] else 'false'}"]
        lines += toml_steps("thisMod", c["steps"])
        for key, steps in c["options"]:
            lines += ["", "[[thisMod.Options]]", f'Guid = "{OPT[key]}"', f'Name = "Option {key}"',
                      "IsSelected = true"]
            lines += toml_steps("thisMod.Options", steps)
        lines.append("")
    return "\n".join(lines) + "\n"


def miss(step_text):
    """The expected report of a one-component guide whose simulation fails."""
    return [1, False, [f"ERROR: B / {step_text}"]]


OK_B = [0, True, ["OK: B"]]
MISSING_COPY = f"step 1 (Copy): Source matches no files: {MD}/nope.txt"
# name: (flags, files, components, expected report [errorCount, passed, messages])
CASES = {
    # A missing source of every action kind: plain `validate` refuses.
    "copy_missing": ([], {}, [component("b", [copy("nope.txt")])], miss(MISSING_COPY)),
    "move_missing": ([], {}, [component("b", [copy("nope.txt", "Move")])],
                     miss(f"step 1 (Move): Source matches no files: {MD}/nope.txt")),
    "patcher_missing": ([], {}, [component("b", [patcher()])],
                        miss(f"step 1 (Patcher): Missing external program/mod: {EXE}")),
    # Execute is refused before its Source is looked up unless --allow-processes.
    "execute_missing": ([], {}, [component("b", [step("Execute", [f"{MD}/nope.exe"])])],
                        miss("step 1 (Execute): Execute needs explicit trust: enable external programs"
                             " (--allow-processes)")),
    "delete_strict_missing": ([], {}, [component("b", [step("Delete", [f"{OV}/nope.txt"], overwrite=True)])],
                              miss(f"step 1 (Delete): Source matches no files: {OV}/nope.txt")),
    "rename_missing": ([], {}, [component("b", [step("Rename", [f"{OV}/nope.txt"], "x.txt")])],
                       miss(f"step 1 (Rename): Source matches no files: {OV}/nope.txt")),
    "cleanlist_missing": ([], {}, [component("b", [step("CleanList", [f"{MD}/nope.csv"], OV)])],
                          miss(f"step 1 (CleanList): Source matches no files: {MD}/nope.csv")),
    "missing_after_present": ([], {}, [component("b", [copy("a.txt"), copy("nope.txt")])],
                              miss(f"step 2 (Copy): Source matches no files: {MD}/nope.txt")),
    "later_component_missing": ([], {}, [component("a", [copy("a.txt")]), component("b", [copy("nope.txt")]),
                                         component("c", [copy("c.txt")])],
                                [1, False, ["OK: A", f"ERROR: B / {MISSING_COPY}", "OK: C"]]),
    "option_missing": ([], {}, [component("b", [copy("a.txt"), step("Choose", [OPT["1"]])],
                                          options=[("1", [patcher()])])],
                       miss(f"step 2 (Choose): Option 1 / step 1 (Patcher): Missing external program/mod: {EXE}")),
    "full_copy_missing": (["--full"], {}, [component("b", [copy("nope.txt")])], miss(MISSING_COPY)),
    # Plain `validate` refuses a component without instructions (deliberate).
    "no_instructions": ([], {}, [component("a", [copy("a.txt")]), component("b", [])],
                        [1, False, ["OK: A", "ERROR: B has no executable instructions; review the guide"]]),
    # Controls: plain `validate` passes, or the archive stage already fails.
    "control_copy_present": ([], {}, [component("b", [copy("a.txt")])], OK_B),
    "control_patcher_present": ([], PATCH, [component("b", [patcher()])], OK_B),
    "control_delete_missing_lenient": ([], {}, [component("b", [step("Delete", [f"{OV}/nope.txt"])])], OK_B),
    "control_condition_unmet": ([], {}, [component("a", [copy("a.txt")], selected=False),
                                         component("b", [copy("b.txt"), copy("nope.txt", depends="a")])], OK_B),
    "control_unselected_missing": ([], {}, [component("a", [copy("a.txt")]),
                                            component("b", [copy("nope.txt")], selected=False)],
                                   [0, True, ["OK: A"]]),
    "control_extract_missing": ([], {}, [component("b", [step("Extract", [f"{MD}/nope.zip"])])],
                                [1, False, [f"ERROR: B: Missing Required Archives: [{MD}/nope.zip]"]]),
    "control_full_present": (["--full"], {}, [component("b", [copy("a.txt")])], OK_B),
}
# C# fails every basic report (no mod directory for the FOMOD stage; the Patcher
# null-path exception) and every `--full` report (no packaged HoloPatcher).
KNOWN = {name: {"decision"} for name, case in CASES.items()
         if name.startswith("control_") and case[3][1]}


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


def invoke(argv_head, case, work, label, mode_flags, extra_env=None):
    flags, files, components, _ = case
    root = tempfile.mkdtemp(prefix=f"{label}-", dir=work)
    path, mods, game = fixture(root, files, components)
    argv = argv_head + ["validate", "-i", path, "-s", mods, "-g", game, "--use-file-selection"] + mode_flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    return subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)


def parse(done):
    return json.loads(done.stdout[done.stdout.index("{"):])


def run_csharp(head, env, case, work):
    flags = case[0]
    done = invoke(head, case, work, "csharp", ["--output", "json"] + flags, env)
    report = parse(done)
    reason = report.get("error") or [m for s in report.get("stages") or [] if not s["passed"]
                                     for m in s.get("messages") or []]
    text = invoke(head, case, work, "csharp-text", flags, env)
    return {"decision": [done.returncode == 0, report.get("success")], "reason": reason,
            "text_exit_zero": text.returncode == 0}


def run_rust(binary, case, work):
    flags = case[0]
    done = invoke([binary], case, work, "rust", ["--output", "json"] + flags)
    report = parse(done)
    plain = invoke([binary], case, work, "rust-plain", [])
    text = invoke([binary], case, work, "rust-text", ["--output", "text"] + flags)
    legacy = invoke([binary], case, work, "rust-report", ["--validation-report"] + flags)
    stage = next((s for s in report.get("stages") or [] if s["stage"] == "ComponentValidation"), {})
    messages = stage.get("messages") or []
    errors = [m for m in messages if m.startswith("ERROR: ")]
    lines = [line for line in plain.stderr.splitlines() if line.startswith("Error: ")]
    return {
        "decision": [done.returncode == 0, report.get("success")],
        "modes": [plain.returncode == 0, text.returncode == 0, legacy.returncode == 0, done.returncode == 0],
        "report": [report.get("errorCount"), stage.get("passed"), messages],
        "wording": bool(errors) and bool(lines) and errors[0][len("ERROR: "):] == lines[-1][len("Error: "):],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="plain-report-simulation-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            head = ["unshare", "-rn", "dotnet", os.path.join(args.reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {name: run_csharp(head, env, case, work) for name, case in CASES.items()}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            actual = run_rust(args.bin, case, work)
            expected = {
                "decision": oracle[name]["decision"],
                "modes": [actual["modes"][0]] * 4,
                "report": case[3],
            }
            checks = ["decision", "modes", "report"]
            if not name.startswith("control_"):
                expected["wording"] = True
                checks.append("wording")
            for check in checks:
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
                detail = "" if same else f" | rust={actual[check]} | expected={expected[check]}"
                print(f"{label:5} {name}/{check}{detail}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
