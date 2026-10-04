#!/usr/bin/env python3
"""CleanList list-file choice proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide with one CleanList
step whose Sources name more than one resolving list file. C# reads only
`RealSourcePaths[0]` (Instruction.ExecuteCleanListAsync), the first authored
Source's file; the lists differ, so the deleted game file shows which list was
read. The Rust binary runs `validate --output json --dry-run`, plain `validate`
and `install` on fresh copies. The dry-run report's decision and its
DryRunSkipped classification are compared with the reference dry run; plain
validate and install with the reference install, and for an accepted install the
final game and mod files (names and contents). Reference results are in
csharp-oracle.json.

The reference is DryRunValidator.ValidateInstallationAsync and
ModComponent.InstallAsync with the real file system (accepted on
InstallExitCode.Success), from the oracle in csharp-oracle/ (the copy-move-self
oracle, unchanged) built against ModSync.Core from d31ccbd3.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated
with `unshare -rn`) instead of using the recorded results; --record also
rewrites csharp-oracle.json from that run. See ../cleanlist-list-choice-contract.md.
"""
import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
GUID = "66666666-6666-6666-6666-666666666666"
OV = "<<kotorDirectory>>/Override"
MD = "<<modDirectory>>"
L = MD + "/l"

# Every fixture starts from these files. Each list deletes a different game file,
# so the final tree shows which list was read.
FILES = {
    "game/chitin.key": "KEY\n",
    "game/Override/a.tga": "A\n",
    "game/Override/b.2da": "B\n",
    "game/Override/s.tga": "S\n",
    "game/Override/z.csv": "Mandatory Deletions,s.tga\n",
    "mods/l/a.csv": "Mandatory Deletions,a.tga\n",
    "mods/l/b.csv": "Mandatory Deletions,b.2da\n",
}
EMPTY_DIRS = []


def cleanlist(sources):
    return ("CleanList", sources, OV, None)


# name -> (instructions, read-only fixture files[, extra fixture files])
CASES = {
    "literal_ab": ([cleanlist([L + "/a.csv", L + "/b.csv"])], []),
    "literal_ba": ([cleanlist([L + "/b.csv", L + "/a.csv"])], []),
    "literal_bab_duplicate": ([cleanlist([L + "/b.csv", L + "/a.csv", L + "/b.csv"])], []),
    "wildcard_then_literal": ([cleanlist([L + "/b*.csv", L + "/a.csv"])], []),
    "literal_then_wildcard": ([cleanlist([L + "/a.csv", L + "/b*.csv"])], []),
    "question_then_literal": ([cleanlist([L + "/b.cs?", L + "/a.csv"])], []),
    "literal_then_overlapping_wildcard": ([cleanlist([L + "/b.csv", L + "/*.csv"])], []),
    "backslash_then_literal": ([cleanlist([MD + "\\l\\b.csv", L + "/a.csv"])], []),
    "game_list_then_mod_list": ([cleanlist([OV + "/z.csv", L + "/a.csv"])], []),
    "mod_list_then_game_list": ([cleanlist([L + "/b.csv", OV + "/z.csv"])], []),
    "case_variant_then_literal": ([cleanlist([L + "/B.csv", L + "/a.csv"])], []),
    "single_control": ([cleanlist([L + "/b.csv"])], []),
    "two_steps": ([cleanlist([L + "/b.csv", L + "/a.csv"]), cleanlist([OV + "/z.csv", L + "/a.csv"])], []),
    "folder_then_literal": ([cleanlist([L, L + "/b.csv"])], []),
}


def toml_string(value):
    return json.dumps(value)


def guide(instructions):
    lines = ["[[thisMod]]", f'Guid = "{GUID}"', 'Name = "CleanList List Choice Probe"', "IsSelected = true"]
    for action, sources, destination, overwrite in instructions:
        lines += ["", "[[thisMod.Instructions]]", f"Action = {toml_string(action)}"]
        lines.append("Source = [" + ", ".join(toml_string(s) for s in sources) + "]")
        if destination is not None:
            lines.append(f"Destination = {toml_string(destination)}")
        if overwrite is not None:
            lines.append(f"Overwrite = {'true' if overwrite else 'false'}")
    return "\n".join(lines) + "\n"


def fixture(root, case):
    instructions, readonly = case[0], case[1]
    files = dict(FILES)
    files.update(case[2] if len(case) > 2 else {})
    for relative in ("scratch", "home", "xdg", *EMPTY_DIRS):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    for relative, text in files.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if relative.endswith(".sh"):
            with open(path, "w") as handle:
                handle.write(text)
            os.chmod(path, 0o755)
            continue
        if isinstance(text, dict):
            with zipfile.ZipFile(path, "w") as archive:
                for member, data in text.items():
                    archive.writestr(member, data)
            continue
        with open(path, "w") as handle:
            handle.write(text)
    for relative in readonly:
        os.chmod(os.path.join(root, relative), stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def files(base):
    result = {}
    for directory, _, names in os.walk(base):
        for name in names:
            full = os.path.join(directory, name)
            key = os.path.relpath(full, base).replace(os.sep, "/")
            if zipfile.is_zipfile(full):
                with zipfile.ZipFile(full) as archive:
                    result[key] = "zip: " + ", ".join(sorted(archive.namelist()))
                continue
            with open(full) as handle:
                result[key] = handle.read()
    return dict(sorted(result.items()))


def trees(mods, game):
    return {"game": files(game), "mods": files(mods)}


def make_writable(root):
    for directory, _, names in os.walk(root):
        for name in names:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    argv += case[3].get("rust_args", []) if len(case) > 3 else []
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "dryrun":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    if mode == "dryrun":
        report = json.loads(done.stdout)
        issues = report.get("dryRun", {}).get("issues", [])
        result["accepted"] = report["errorCount"] == 0
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
        result["stderr_tail"] = []
    make_writable(root)
    return result


def run_csharp(dll, reference, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, "dryrun" if mode == "validate" else "install", path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):])
    if mode == "validate":
        result = {
            "accepted": not raw["Errors"],
            "tree": None,
            "skipped": any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["Warnings"]),
            "raw": raw,
        }
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
        # The final tree is recorded for failures too; only accepted trees are compared.
        result = {"accepted": accepted, "tree": trees(mods, game), "raw": raw}
    make_writable(root)
    return result


# Known differences: name -> (Rust modes, reason). The Rust result must be a
# refusal where the reference accepts (fail-closed), or the check fails.
KNOWN = {
    "folder_then_literal": (
        ("dryrun", "validate", "install"),
        "Rust requires every Source to match (F05-X2); C# drops the folder Source and reads b.csv",
    ),
}


# Deliberately unchanged acceptances (none in this proof).
KNOWN_ACCEPTS = {}


# Rust mode -> (reference mode, compare the dry-run skip classification).
MODES = {"dryrun": ("validate", True), "validate": ("install", False), "install": ("install", False)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="cleanlist-list-choice-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, case, work) for mode in ("validate", "install")}
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
            for mode, (reference_mode, compare_skip) in MODES.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, case, work)
                expected_tree = expected["tree"] if mode == "install" and expected["accepted"] else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                if compare_skip:
                    same = same and actual["skipped"] == expected["skipped"]
                refused = not actual["accepted"] or (compare_skip and actual["skipped"])
                is_known = name in KNOWN and mode in KNOWN[name][0]
                accepts = name in KNOWN_ACCEPTS and mode in KNOWN_ACCEPTS[name][0]
                if same and not is_known and not accepts:
                    matched += 1
                    verdict = "match"
                elif (
                    accepts
                    and not same
                    and not refused
                    and (not expected["accepted"] or (compare_skip and expected["skipped"]))
                ):
                    known += 1
                    verdict = "known"
                elif is_known and not same and refused:
                    known += 1
                    verdict = "known"
                else:
                    failed += 1
                    verdict = "FAIL"
                skip = f" skipped={actual['skipped']}/{expected['skipped']}" if compare_skip else ""
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}{skip}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
