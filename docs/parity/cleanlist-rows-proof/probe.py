#!/usr/bin/env python3
"""CleanList row and listed-name proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide with one CleanList
step, `CleanList [<<modDirectory>>/l.csv] -> <<kotorDirectory>>/Override`; only the
list's text changes. C# (Instruction.ExecuteCleanListAsync) splits the list on CR
and LF, splits each row on every comma with no CSV quoting, matches the trimmed
first field against the selected component names (ModComponent IsModSelected),
and for each other trimmed field deletes Path.Combine(target, field) when
FileExists says it is there. A field that cannot name an existing file (a quote,
`<`, `>`, `|`, `:`, `*`, `?`) is skipped as missing; `[` and `]` are literal.

The Rust binary runs `validate --output json --dry-run` (the validation pipeline),
plain `validate` and `install` on fresh copies. The pipeline is compared with the
reference InstallationValidationPipeline (DryRun, file selection): the
archive/destination stage's Passed flag, the dry-run decision and the
DryRunSkipped classification. Plain validate and install are compared with the
reference install, and for an accepted install the final game and mod files
(names and contents). Reference results are in csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated with
`unshare -rn`) instead of using the recorded results; --record also rewrites
csharp-oracle.json from that run. See ../cleanlist-rows-contract.md.
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
GUID = "88888888-8888-8888-8888-888888888888"
NAME = "CleanList Row Probe"

# Every fixture starts from these files; mods/l.csv is the case's list text, with
# {GAME} replaced by the fixture's absolute game directory.
FILES = {
    "game/chitin.key": "KEY\n",
    "game/x.txt": "X\n",
    "game/Override/a.tga": "A\n",
    "game/Override/b.2da": "B\n",
    "game/Override/c[1].tga": "C1\n",
    "game/Override/d.tga": "D\n",
    "game/Override/sub/x.tga": "SX\n",
}

MD = "Mandatory Deletions"

# name -> list text
CASES = {
    # Row grammar: blank rows, CR/LF mixes, whitespace, empty fields, BOM.
    "control": f"{MD},a.tga\n",
    "blank_rows": f"\n\n   \n\t\n{MD},a.tga\r\n\r\n\r\n",
    "cr_only": f"{MD},a.tga\r{MD},b.2da\r",
    "no_final_newline": f"{MD},a.tga",
    "padded_fields": f"  {MD}  ,  a.tga  ,\t b.2da\t\n",
    "empty_fields": f"{MD},,  ,a.tga,\n",
    "bom": f"﻿{MD},a.tga\n",
    "duplicate_field": f"{MD},a.tga,a.tga\n",
    "no_comma_row": f"{MD}\n{MD},a.tga\n",
    "semicolon_row": f"{MD};a.tga\n{MD},b.2da\n",
    # Row matching against the selected component name.
    "mandatory_lower": "mandatory deletions,a.tga\n",
    "mandatory_suffix": f"{MD} (K1 only),a.tga\n",
    "empty_name": ",a.tga\n",
    "stop_words_name": "The Mod,a.tga\n",
    "unselected_name": "Some Other Thing,a.tga\n",
    "selected_name": f"{NAME},a.tga\n",
    "selected_name_case": f"cleanlist row PROBE,a.tga\n",
    "name_substring": "Row Probe,a.tga\n",
    "token_overlap": "Probes for Rows Pack,a.tga\n",
    "single_token": "Probe Extra Words,a.tga\n",
    # Comments are ordinary rows.
    "comment_mandatory": f"# {MD},a.tga\n{MD},b.2da\n",
    "comment_selected": f"#{NAME},a.tga\n",
    # CSV quoting is not parsed: quotes stay in the fields.
    "quoted_name": f'"{MD}",a.tga\n',
    "quoted_selected_name": f'"{NAME}",a.tga\n',
    "quoted_file": f'{MD},"a.tga",b.2da\n',
    "quoted_row": f'"{MD},a.tga",b.2da\n',
    "quoted_name_with_comma": f'"{NAME}, by Someone",a.tga\n',
    "quoted_files_with_comma": f'{MD},"a.tga,b.2da",d.tga\n',
    # Listed names that cannot name a file are skipped; brackets are literal.
    "bracket_literal": f"{MD},c[1].tga\n",
    "bracket_missing": f"{MD},c[2].tga,a.tga\n",
    "star": f"{MD},*.tga,b.2da\n",
    "question": f"{MD},?.tga,b.2da\n",
    "colon": f"{MD},ab:c.tga,sub/x:y.tga,b.2da\n",
    "angle_pipe": f"{MD},a<b.tga,a>b.tga,a|b.tga,b.2da\n",
    "star_in_subdir": f"{MD},sub/*.tga,b.2da\n",
    # Listed paths.
    "subdir_slash": f"{MD},sub/x.tga\n",
    "dot_part": f"{MD},./a.tga\n",
    "directory_name": f"{MD},sub,a.tga\n",
    "missing_file": f"{MD},none.tga,a.tga\n",
    "backslash_subdir": f"{MD},sub\\x.tga,a.tga\n",
    "case_variant": f"{MD},A.TGA,b.2da\n",
    "dotdot": f"{MD},../x.txt,a.tga\n",
    "absolute": f"{MD},{{GAME}}/x.txt,a.tga\n",
    "drive_relative": f"{MD},a:b.tga,b.2da\n",
    "drive_absolute": f"{MD},C:\\Windows\\x.txt,b.2da\n",
    "trailing_dot": f"{MD},a.tga.,b.2da\n",
    "reserved_name": f"{MD},nul.tga,b.2da\n",
}


def toml_string(value):
    return json.dumps(value)


def guide():
    return "\n".join(
        [
            "[[thisMod]]",
            f'Guid = "{GUID}"',
            f"Name = {toml_string(NAME)}",
            "IsSelected = true",
            "",
            "[[thisMod.Instructions]]",
            'Action = "CleanList"',
            'Source = ["<<modDirectory>>/l.csv"]',
            'Destination = "<<kotorDirectory>>/Override"',
        ]
    ) + "\n"


def fixture(root, text):
    for relative in ("scratch", "home", "xdg", "mods", "game"):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    game = os.path.join(root, "game")
    tree = dict(FILES)
    tree["mods/l.csv"] = text.replace("{GAME}", game)
    for relative, content in tree.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(content.encode("utf-8"))
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide())
    return path, os.path.join(root, "mods"), game


def files(base, game):
    result = {}
    for directory, _, names in os.walk(base):
        for name in names:
            full = os.path.join(directory, name)
            with open(full, "rb") as handle:
                # The list text may hold the absolute fixture path; normalise it.
                result[os.path.relpath(full, base).replace(os.sep, "/")] = (
                    handle.read().decode("utf-8").replace(game, "{GAME}")
                )
    return dict(sorted(result.items()))


def trees(mods, game):
    return {"game": files(game, game), "mods": files(mods, game)}


def make_writable(root):
    for directory, _, names in os.walk(root):
        for name in names:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def run_rust(binary, mode, text, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, text)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "pipeline":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    if mode == "pipeline":
        report = json.loads(done.stdout)
        stages = {stage["stage"]: stage for stage in report.get("stages", [])}
        issues = report.get("dryRun", {}).get("issues", [])
        component = stages.get("ComponentValidation", {})
        result["accepted"] = component.get("passed", False)
        result["dry_run"] = not any(issue.get("severity") == "Error" for issue in issues)
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
        errors = [issue.get("message", "") for issue in issues if issue.get("severity") == "Error"]
        result["stderr_tail"] = (component.get("messages", []) if not result["accepted"] else errors)[:1]
    make_writable(root)
    return result


def run_csharp(dll, reference, mode, text, work):
    root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
    path, mods, game = fixture(root, text)
    env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    argv = ["unshare", "-rn", "dotnet", dll, reference, mode, path, mods, game]
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
    line = next(l for l in done.stdout.splitlines() if l.startswith("@@JSON@@"))
    raw = json.loads(line[len("@@JSON@@"):].replace(game, "{GAME}"))
    if mode == "pipeline":
        stages = {stage["Stage"]: stage for stage in raw["Stages"]}
        result = {
            "accepted": stages["ComponentValidation"]["Passed"],
            "tree": None,
            "dry_run": not raw["DryRunErrors"],
            "skipped": any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["DryRunWarnings"]),
            "raw": raw,
        }
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
        # The final tree is recorded for failures too; only accepted trees are compared.
        result = {"accepted": accepted, "tree": trees(mods, game), "raw": raw}
    make_writable(root)
    return result


# Known refusals: name -> (Rust modes, reason). Rust must refuse (a failed stage,
# a dry-run error or a refused verb) where the reference accepts, or the check fails.
KNOWN = {
    "dotdot": (
        ("pipeline", "validate", "install"),
        "C# Path.Combine deletes game/x.txt above the target; Rust refuses `..` in a listed name",
    ),
    "absolute": (
        ("pipeline", "validate", "install"),
        "C# Path.Combine takes a rooted field as is and deletes game/x.txt; Rust refuses an absolute listed name",
    ),
    "drive_relative": (
        ("pipeline", "validate", "install"),
        "C# on Linux skips `a:b.tga` (missing); on Windows it is drive-qualified and Path.Combine"
        " leaves the target; Rust refuses a drive-qualified listed name as absolute",
    ),
    "drive_absolute": (
        ("pipeline", "validate", "install"),
        "C# on Linux skips `C:\\Windows\\x.txt` (missing); on Windows it deletes that file outside"
        " the target; Rust refuses a drive-qualified listed name as absolute",
    ),
    "trailing_dot": (
        ("pipeline", "validate", "install"),
        "C# on Linux skips `a.tga.` (missing) and on Windows deletes a.tga (Win32 trims the dot);"
        " Rust refuses a name whose meaning differs by platform",
    ),
    "reserved_name": (
        ("pipeline", "validate", "install"),
        "C# on Linux skips `nul.tga` (missing); Rust refuses Windows device names on every host",
    ),
}


# Known tree differences on an accepted install: name -> reason. Both accept; the
# Rust tree must equal the case's expected Windows-semantics tree.
KNOWN_TREES = {
    "backslash_subdir": "C# on Linux treats `sub\\x.tga` as one literal name and skips it; on Windows,"
    " and in Rust on every host, `\\` is a separator and Override/sub/x.tga is deleted",
    "case_variant": "C# FileExists is case-sensitive on Linux and skips `A.TGA`; on Windows, and in Rust"
    " on every host, the lookup is case-insensitive and a.tga is deleted",
}
WINDOWS_DELETES = {
    "backslash_subdir": ("Override/sub/x.tga",),
    "case_variant": ("Override/a.tga",),
}


# Rust mode -> reference mode.
MODES = {"pipeline": "pipeline", "validate": "install", "install": "install"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="cleanlist-rows-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, text, work) for mode in ("pipeline", "install")}
                for name, text in CASES.items()
            }
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, text in CASES.items():
            for mode, reference_mode in MODES.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, text, work)
                expected_tree = expected["tree"] if mode == "install" and expected["accepted"] else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                extra = ""
                if mode == "pipeline":
                    same = same and actual["dry_run"] == expected["dry_run"] and actual["skipped"] == expected["skipped"]
                    extra = (f" dry_run={actual['dry_run']}/{expected['dry_run']}"
                             f" skipped={actual['skipped']}/{expected['skipped']}")
                is_known = name in KNOWN and mode in KNOWN[name][0]
                refused = not actual["accepted"] or (mode == "pipeline" and not actual["dry_run"])
                windows_tree = None
                if name in KNOWN_TREES and mode == "install" and expected_tree is not None:
                    windows_tree = json.loads(json.dumps(expected_tree))
                    for relative in WINDOWS_DELETES[name]:
                        windows_tree["game"].pop(relative, None)
                if same and not is_known and windows_tree is None:
                    matched += 1
                    verdict = "match"
                elif is_known and not same and refused:
                    known += 1
                    verdict = "known"
                elif windows_tree is not None and actual["accepted"] and actual["tree"] == windows_tree:
                    known += 1
                    verdict = "known"
                else:
                    failed += 1
                    verdict = "FAIL"
                print(
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}{extra}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
