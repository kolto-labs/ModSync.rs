#!/usr/bin/env python3
"""Patcher INI brackets proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide whose components run
one Patcher step on `<<modDirectory>>/<patch>/TSLPatcher.exe`, usually followed by a
Rename of the file the patcher's changes.ini InstallList would install. The INI
values (InstallList folder and file names, namespaces.ini IniName and DataPath)
hold `[` or `]` in most cases.

C# (TslPatcherVfsSimulator.SimulateInstallAsync, reached from
Instruction.ExecuteTSLPatcherAsync during a dry run) reads changes.ini from the
patcher folder or its tslpatchdata folder and copies each InstallList entry to
Path.Combine(game, folder, file), so brackets are ordinary characters. The Rename
step only passes when that file exists in the virtual tree.

The Rust binary runs `validate --output json --dry-run` (the validation pipeline),
plain `validate` and `conflicts`, all with `--allow-processes`, on fresh copies. No
program runs: validation and conflict analysis simulate the patcher. The pipeline
and plain validate are compared with the reference InstallationValidationPipeline
(DryRun, file selection): the overall decision, the dry-run decision and the
DryRunSkipped classification (pipeline only). `conflicts` is compared with the
reference FileConflictAnalyzer: the conflict paths, writers (GUID, name,
instruction index), winners, per-component counts and the analyzed component
count. Reference results are in csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated with
`unshare -rn`) instead of using the recorded results; --record also rewrites
csharp-oracle.json from that run. See ../patcher-ini-brackets-contract.md.
"""
import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
KD = "<<kotorDirectory>>"
MD = "<<modDirectory>>"
GUID_A = "77777777-8888-8888-8888-888888888881"
GUID_B = "77777777-8888-8888-8888-888888888882"

BASE_FILES = {
    "game/chitin.key": "KEY\n",
    "game/Override/o.txt": "O\n",
}


def install_list(folder, *entries):
    lines = ["[InstallList]", f"install_folder0={folder}", "", "[install_folder0]"]
    lines += [f"{key}={value}" for key, value in entries]
    return "\n".join(lines) + "\n"


def patch(name, ini, payloads, data="tslpatchdata"):
    """Files for one patcher folder: the executable, its INI and payloads."""
    files = {f"mods/{name}/TSLPatcher.exe": "EXE\n", f"mods/{name}/{data}/changes.ini": ini}
    for payload in payloads:
        files[f"mods/{name}/{data}/{payload}"] = f"payload {payload}\n"
    return files


def namespaces(entries):
    lines = ["[Namespaces]"] + [f"Namespace{i + 1}=ns{i}" for i in range(len(entries))]
    for i, (ini_name, data_path) in enumerate(entries):
        lines += ["", f"[ns{i}]", f"IniName={ini_name}", f"DataPath={data_path}", f"Name=Option {i}"]
    return "\n".join(lines) + "\n"


def step_patcher(name, arguments=""):
    step = ["[[thisMod.Instructions]]", 'Action = "Patcher"',
            f"Source = [{json.dumps(MD + '/' + name + '/TSLPatcher.exe')}]", f"Destination = {json.dumps(KD)}"]
    if arguments:
        step.append(f"Arguments = {json.dumps(arguments)}")
    return step


def step_rename(source, destination="renamed.tga"):
    return ["[[thisMod.Instructions]]", 'Action = "Rename"',
            f"Source = [{json.dumps(KD + '/' + source)}]", f"Destination = {json.dumps(destination)}"]


def component(guid, name, steps):
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true", ""]
    for step in steps:
        lines += step + [""]
    return lines


def single(ini, payloads, renamed=None, arguments="", extra=None):
    steps = [step_patcher("patch", arguments)]
    if renamed:
        steps.append(step_rename(renamed))
    files = patch("patch", ini, payloads)
    files.update(extra or {})
    return files, component(GUID_A, "Patcher Probe", steps)


def pair(ini, payloads):
    files = patch("patchA", ini, payloads)
    files.update(patch("patchB", ini, payloads))
    lines = component(GUID_A, "First Patcher", [step_patcher("patchA")])
    lines += component(GUID_B, "Second Patcher", [step_patcher("patchB")])
    return files, lines


SAME_NS = install_list("Override", ("File0", "tex.tga"))

# name -> (files, guide lines)
CASES = {
    "plain_file": single(install_list("Override", ("File0", "tex.tga")), ["tex.tga"], "Override/tex.tga"),
    "bracket_file": single(install_list("Override", ("File0", "[HD]tex.tga")), ["[HD]tex.tga"],
                           "Override/[HD]tex.tga"),
    "bracket_file_only": single(install_list("Override", ("File0", "[HD]tex.tga")), ["[HD]tex.tga"]),
    "bracket_replace": single(install_list("Override", ("Replace0", "[a]b.2da")), ["[a]b.2da"],
                              "Override/[a]b.2da"),
    "closing_bracket_only": single(install_list("Override", ("File0", "tex]1.tga")), ["tex]1.tga"],
                                   "Override/tex]1.tga"),
    "bracket_class_lookalike": single(install_list("Override", ("File0", "t[ae]x.tga")), ["t[ae]x.tga"],
                                      "Override/t[ae]x.tga"),
    "bracket_folder": single(install_list("Override/[x]", ("File0", "a.tga")), ["a.tga"], "Override/[x]/a.tga"),
    "bracket_payload_missing": single(install_list("Override", ("File0", "[HD]none.tga")), []),
    "bracket_two_entries": single(install_list("Override", ("File0", "[1]a.tga"), ("File1", "b[2].tga")),
                                  ["[1]a.tga", "b[2].tga"], "Override/b[2].tga"),
    # Rust follows namespaces.ini; the reference simulation always reads changes.ini,
    # so both INIs list the same file and only the bracket admission is compared.
    "namespace_bracket_datapath": single(
        SAME_NS, ["tex.tga"], "Override/tex.tga", "0",
        {"mods/patch/tslpatchdata/namespaces.ini": namespaces([("changes.ini", "[A] Option")]),
         "mods/patch/tslpatchdata/[A] Option/changes.ini": SAME_NS,
         "mods/patch/tslpatchdata/[A] Option/tex.tga": "payload tex.tga\n"}),
    "namespace_bracket_ininame": single(
        SAME_NS, ["tex.tga"], "Override/tex.tga", "0",
        {"mods/patch/tslpatchdata/namespaces.ini": namespaces([("[A]changes.ini", "")]),
         "mods/patch/tslpatchdata/[A]changes.ini": SAME_NS}),
    "conflict_plain": pair(install_list("Override", ("File0", "tex.tga")), ["tex.tga"]),
    "conflict_bracket": pair(install_list("Override", ("File0", "[HD]tex.tga")), ["[HD]tex.tga"]),
    # Known differences (deliberate, see KNOWN).
    "star_file": single(install_list("Override", ("File0", "s*.tga")), ["s*.tga"]),
    "question_file": single(install_list("Override", ("File0", "q?.tga")), ["q?.tga"]),
}


def fixture(root, case):
    files, lines = case
    for relative in ("scratch", "home", "xdg", "mods", "game"):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    for relative, text in {**BASE_FILES, **files}.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write("\n".join(lines))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def make_writable(root):
    for directory, _, names in os.walk(root):
        os.chmod(directory, stat.S_IRWXU)
        for name in names:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def conflict_view(conflicts, counts, analyzed):
    return {
        "conflicts": [
            {"path": c["path"], "writers": [list(w) for w in c["writers"]], "winner": c["winner"]} for c in conflicts
        ],
        "counts": dict(sorted(counts.items())),
        "analyzed": analyzed,
    }


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    verb = "conflicts" if mode == "conflicts" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game, "--allow-processes"]
    if mode == "pipeline":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {"accepted": accepted, "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else []}
    if mode == "pipeline":
        report = json.loads(done.stdout)
        issues = report.get("dryRun", {}).get("issues", [])
        result["accepted"] = report.get("success", False)
        result["dry_run"] = not any(issue.get("severity") == "Error" for issue in issues)
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
        failed = [s for s in report.get("stages", []) if not s.get("passed")]
        result["stderr_tail"] = [f"{s['stage']}: {(s.get('messages') or [''])[0]}" for s in failed][:1]
    if mode == "conflicts" and accepted:
        report = json.loads(done.stdout)
        result["report"] = conflict_view(
            [{"path": c["destinationPath"],
              "writers": [(w["componentGuid"], w["componentName"], w["instructionIndex"]) for w in c["writers"]],
              "winner": c["winnerComponentGuid"]} for c in report["conflicts"]],
            report["conflictCountsByComponent"], report["analyzedComponentCount"])
    make_writable(root)
    return result


def run_csharp(dll, reference, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, mode, path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):])
    if mode == "pipeline":
        result = {
            "accepted": raw["IsSuccess"],
            "dry_run": not raw["DryRunErrors"],
            "skipped": any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["DryRunWarnings"] or []),
            "raw": raw,
        }
    else:
        # The analyzer catches instruction failures and still returns a report.
        result = {
            "accepted": True,
            "report": conflict_view(
                [{"path": c["DestinationPath"],
                  "writers": [(w["ComponentGuid"], w["ComponentName"], w["InstructionIndex"]) for w in c["Writers"]],
                  "winner": c["WinnerComponentGuid"]} for c in raw["Conflicts"]],
                raw["ConflictCountsByComponent"], raw["AnalyzedComponentCount"]),
        }
    make_writable(root)
    return result


# Known differences: name -> (Rust modes, reason). Rust must refuse where the
# reference accepts; otherwise the check fails.
EVERY = ("pipeline", "validate", "conflicts")
KNOWN = {
    "star_file": (EVERY, "No Windows file can hold `*`; C# on Linux simulates `s*.tga`, Rust refuses the INI entry on every host"),
    "question_file": (EVERY, "No Windows file can hold `?`; C# on Linux simulates `q?.tga`, Rust refuses the INI entry on every host"),
}

# Rust mode -> reference mode.
MODES = {"pipeline": "pipeline", "validate": "pipeline", "conflicts": "conflicts"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="patcher-ini-brackets-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, case, work)
                       for mode in ("pipeline", "conflicts")}
                for name, case in CASES.items()
            }
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            for mode, reference_mode in MODES.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, case, work)
                same = actual["accepted"] == expected["accepted"]
                extra = ""
                if mode == "pipeline":
                    same = same and actual["dry_run"] == expected["dry_run"] and actual["skipped"] == expected["skipped"]
                    extra = (f" dry_run={actual['dry_run']}/{expected['dry_run']}"
                             f" skipped={actual['skipped']}/{expected['skipped']}")
                if mode == "conflicts":
                    same = same and actual.get("report") == expected["report"]
                    extra = f" report={actual.get('report')} | reference report={expected['report']}"
                is_known = name in KNOWN and mode in KNOWN[name][0]
                refused = not actual["accepted"] or (mode == "pipeline" and not actual["dry_run"])
                if same and not is_known:
                    matched += 1
                    verdict = "match"
                elif is_known and not same and refused and expected["accepted"]:
                    known += 1
                    verdict = "known"
                else:
                    failed += 1
                    verdict = "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']}"
                    f" | reference accepted={expected['accepted']}{extra}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
