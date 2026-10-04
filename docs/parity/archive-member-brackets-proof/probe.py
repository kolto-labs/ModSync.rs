#!/usr/bin/env python3
"""Archive member brackets proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide that extracts a zip
whose member names contain `[` or `]` (or, for the known differences, `*` or `?`)
and then acts on what it extracted. C# extraction
(RealFileSystemProvider.ExtractArchiveAsync and the virtual provider used by the
dry run) writes each member under PathHelper.TryGetZipSafeArchiveEntryExtractPath,
which refuses only rooted names and `..` parts, so brackets are ordinary
characters.

The Rust binary runs `validate --output json --dry-run` (the validation pipeline),
plain `validate`, `install` and `install --component-checkpoints` on fresh copies.
The pipeline is compared with the reference InstallationValidationPipeline
(DryRun, file selection): the overall decision, the dry-run decision and the
DryRunSkipped classification. Plain validate, install and the checkpointed install
are compared with the reference install, and for an accepted install the final
game and mod files (names and contents). Reference results are in
csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated with
`unshare -rn`) instead of using the recorded results; --record also rewrites
csharp-oracle.json from that run. See ../archive-member-brackets-contract.md.
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
GUID = "88888888-8888-8888-8888-888888888888"
SECOND = "99999999-8888-8888-8888-888888888888"
KD = "<<kotorDirectory>>"
MD = "<<modDirectory>>"
OV = KD + "/Override"

FILES = {
    "game/chitin.key": "KEY\n",
    "game/Override/o.txt": "O\n",
    "mods/m.txt": "M\n",
}

BRACKETS = {"Override/x[1].tga": "X1\n", "Override/y.tga": "Y\n"}
FOLDER = {"[HD] textures/t.tga": "T\n", "[HD] textures/u.tpc": "U\n"}
UNMATCHED = {"Override/a[.tga": "A\n", "Override/b].tga": "B\n", "Override/[c].tga": "C\n"}
CLASS = {"Override/x[1].tga": "X1\n", "Override/x1.tga": "PLAIN\n"}


def extract(archive="a.zip", destination=None):
    return ("Extract", [MD + "/" + archive], destination, None)


def copy(source, destination=OV):
    return ("Copy", [source], destination, None)


def move(source, destination=OV):
    return ("Move", [source], destination, None)


# name -> (instructions, {archive path: members} | extra files)
CASES = {
    "plain_control": ([extract(), move(MD + "/a/Override/*")], {"mods/a.zip": {"Override/y.tga": "Y\n"}}),
    "extract_only": ([extract()], {"mods/a.zip": BRACKETS}),
    "move_wildcard": ([extract(), move(MD + "/a/Override/*")], {"mods/a.zip": BRACKETS}),
    "copy_literal": ([extract(), copy(MD + "/a/Override/x[1].tga")], {"mods/a.zip": BRACKETS}),
    "copy_literal_backslash": ([extract(), copy(MD + "\\a\\Override\\x[1].tga")], {"mods/a.zip": BRACKETS}),
    "copy_bracket_wildcard": ([extract(), copy(MD + "/a/Override/x[*")], {"mods/a.zip": BRACKETS}),
    # A literal bracketed Source names only that file, never the class `x1.tga`.
    "copy_literal_not_class": ([extract(), copy(MD + "/a/Override/x[1].tga")], {"mods/a.zip": CLASS}),
    "bracket_folder_copy": ([extract(), copy(MD + "/a/[HD] textures/*")], {"mods/a.zip": FOLDER}),
    "unmatched_brackets": ([extract(), move(MD + "/a/Override/*")], {"mods/a.zip": UNMATCHED}),
    "rename_bracket_source": (
        [extract(), ("Rename", [MD + "/a/Override/x[1].tga"], "x.tga", None)],
        {"mods/a.zip": BRACKETS},
    ),
    "delete_bracket_source": (
        [extract(), ("Delete", [MD + "/a/Override/x[1].tga"], None, None)],
        {"mods/a.zip": BRACKETS},
    ),
    "bracket_archive_name": ([extract("[e]xample.zip"), move(MD + "/[e]xample/Override/*")], {"mods/[e]xample.zip": BRACKETS}),
    # No Extract: the Source names a file inside the unextracted archive's folder.
    "recovery_copy_literal": ([copy(MD + "/a/Override/x[1].tga")], {"mods/a.zip": BRACKETS}),
    "recovery_plain_control": ([copy(MD + "/a/Override/y.tga")], {"mods/a.zip": BRACKETS}),
    # The game already holds a bracketed file; only the checkpointed install differed.
    "existing_game_bracket_file": ([copy(MD + "/m.txt")], {"game/Override/pre[1].txt": "P\n"}),
    # `*` and `?` cannot be Windows file names: C# on Linux writes them, Rust refuses.
    "star_member": ([extract(), move(MD + "/a/Override/*")], {"mods/a.zip": {"Override/s*.tga": "S\n"}}),
    "question_member": ([extract(), move(MD + "/a/Override/*")], {"mods/a.zip": {"Override/q?.tga": "Q\n"}}),
    # A second component runs a program after the first deployed a bracketed file.
    # With --managed, Rust captures a preimage of every game file before the process.
    "execute_after_bracket_deploy": (
        [extract(), move(MD + "/a/Override/*")],
        {"mods/a.zip": BRACKETS, "mods/run.sh": "#!/bin/sh\nexit 0\n"},
        [("Execute", [MD + "/run.sh"], None, None)],
    ),
    "execute_after_plain_deploy": (
        [extract(), move(MD + "/a/Override/*")],
        {"mods/a.zip": {"Override/y.tga": "Y\n"}, "mods/run.sh": "#!/bin/sh\nexit 0\n"},
        [("Execute", [MD + "/run.sh"], None, None)],
    ),
}


def has_process(case):
    steps = list(case[0]) + list(case[2] if len(case) > 2 and case[2] else [])
    return any(step[0] in ("Execute", "Run", "Patcher") for step in steps)


# FOMOD archives with saved choices. The reference install does not map a FOMOD at
# Extract time (C# maps it into a component when the download is configured), so
# these are checked against expected results: the FOMOD mapper evidence in
# csharp-fomod-map.json (FomodToComponentMapper keeps `[`/`]` in a mapped Source,
# and a literal Copy Source is matched as `copy_literal` shows), and for wrapper
# folders the same result as the unbracketed `HD Mod` control.
# name -> (archive name, wrapper folder, ModuleConfig file source)
FOMOD_CASES = {
    "fomod_control": ("a.zip", "HD Mod", "a.tga"),
    "fomod_bracket_wrapper": ("a.zip", "[HD] Mod", "a.tga"),
    "fomod_bracket_archive_name": ("[e]xample.zip", "", "a.tga"),
    "fomod_bracket_source": ("a.zip", "", "[x]/a.tga"),
    "fomod_bracket_wrapper_and_source": ("a.zip", "[HD] Mod", "[x]/a.tga"),
}


def fomod_case(archive, wrapper, source):
    prefix = wrapper + "/" if wrapper else ""
    config = (
        "<config><moduleName>T</moduleName><requiredInstallFiles>"
        f'<file source="{source}" destination="Override"/></requiredInstallFiles></config>'
    )
    members = {prefix + "fomod/ModuleConfig.xml": config, prefix + source: "FOMOD\n"}
    choices = (
        "{ version = 1, archives = [ { archiveFileName = "
        + toml_string(archive)
        + ", selections = [] } ] }"
    )
    return ([extract(archive)], {"mods/" + archive: members}, None, choices)


def toml_string(value):
    return json.dumps(value)


def steps(lines, instructions):
    for action, sources, destination, arguments in instructions:
        lines += ["", "[[thisMod.Instructions]]", f"Action = {toml_string(action)}"]
        lines.append("Source = [" + ", ".join(toml_string(s) for s in sources) + "]")
        if destination is not None:
            lines.append(f"Destination = {toml_string(destination)}")
        if arguments is not None:
            lines.append(f"Arguments = {toml_string(arguments)}")


def guide(instructions, second=None, choices=None):
    lines = ["[[thisMod]]", f'Guid = "{GUID}"', 'Name = "Archive Member Brackets Probe"', "IsSelected = true"]
    if choices:
        lines.append(f"FomodChoices = {choices}")
    steps(lines, instructions)
    if second:
        lines += ["", "[[thisMod]]", f'Guid = "{SECOND}"', 'Name = "Second Component"', "IsSelected = true"]
        lines.append(f'InstallAfter = ["{GUID}"]')
        steps(lines, second)
    return "\n".join(lines) + "\n"


def fixture(root, case):
    instructions, extra = case[0], case[1]
    second = case[2] if len(case) > 2 else None
    choices = case[3] if len(case) > 3 else None
    for relative in ("scratch", "home", "xdg", "mods", "game"):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    tree = dict(FILES)
    tree.update(extra)
    for relative, text in tree.items():
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if isinstance(text, dict):
            with zipfile.ZipFile(path, "w") as archive:
                for member, data in text.items():
                    archive.writestr(member, data)
            continue
        with open(path, "w") as handle:
            handle.write(text)
        if path.endswith(".sh"):
            os.chmod(path, 0o755)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(instructions, second, choices))
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def files(base):
    result = {}
    for directory, dirs, names in os.walk(base):
        # Rust keeps its recovery and checkpoint state in hidden folders.
        dirs[:] = [d for d in dirs if not d.startswith(".")]
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
        os.chmod(directory, stat.S_IRWXU)
        for name in names:
            full = os.path.join(directory, name)
            if not os.path.islink(full):
                os.chmod(full, stat.S_IRUSR | stat.S_IWUSR)


def run_rust(binary, mode, case, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    verb = "validate" if mode in ("pipeline", "validate") else "install"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if verb == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "checkpoint":
        argv.append("--component-checkpoints")
    if mode == "managed":
        argv += ["--component-checkpoints", "--managed", "--staging-dir", os.path.join(root, "staging"),
                 "--manifest-dir", os.path.join(root, "manifests")]
    if has_process(case):
        argv.append("--allow-processes")
    if mode == "pipeline":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game) if accepted and verb == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if not accepted else [],
    }
    if mode == "pipeline":
        report = json.loads(done.stdout)
        issues = report.get("dryRun", {}).get("issues", [])
        result["accepted"] = report.get("success", False)
        result["dry_run"] = not any(issue.get("severity") == "Error" for issue in issues)
        result["skipped"] = any(issue["category"] == "DryRunSkipped" for issue in issues)
        failed = [s for s in report.get("stages", []) if not s.get("passed")]
        result["stderr_tail"] = [f"{s['stage']}: {(s.get('messages') or [''])[0]}" for s in failed][:1]
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
            "tree": None,
            "dry_run": not raw["DryRunErrors"],
            "skipped": any(w.startswith("Skipped '") and "during dry-run" in w for w in raw["DryRunWarnings"] or []),
            "raw": raw,
        }
    else:
        accepted = all(exit.endswith(": Success") for exit in raw["Exits"])
        # The final tree is recorded for failures too; only accepted trees are compared.
        result = {"accepted": accepted, "tree": trees(mods, game), "raw": raw}
    make_writable(root)
    return result


# Known differences: name -> (Rust modes, reason). Rust must refuse where the
# reference accepts, or the check fails.
STAR = ("pipeline", "validate", "install", "checkpoint")
KNOWN = {
    "star_member": (STAR, "a member named with `*` cannot be a Windows file; C# on Linux writes it, Rust refuses the archive"),
    "question_member": (STAR, "a member named with `?` cannot be a Windows file; C# on Linux writes it, Rust refuses the archive"),
}

# Rust mode -> reference mode. Cases that run a program add a managed install.
MODES = {"pipeline": "pipeline", "validate": "install", "install": "install", "checkpoint": "install"}
MANAGED = {"managed": "install"}
FOMOD_MAP = os.path.join(HERE, "csharp-fomod-map.json")
# (ModuleConfig file source, archive name) pairs given to the reference mapper.
MAPPINGS = [("a.tga", "a.zip"), ("[x]/a.tga", "a.zip"), ("a.tga", "[e]xample.zip")]


def run_fomod_map(dll, reference):
    result = {}
    for source, archive in MAPPINGS:
        argv = ["unshare", "-rn", "dotnet", dll, reference, source, archive]
        done = subprocess.run(argv, capture_output=True, text=True, timeout=300)
        result[f"{archive}|{source}"] = [l for l in done.stdout.splitlines() if l.startswith("Copy [")]
    return result


def check_fomod(binary, work, mapping):
    """Mapper evidence first, then Rust validate and install of each FOMOD case."""
    matched = failed = 0
    for source, archive in MAPPINGS:
        folder = archive.rsplit(".", 1)[0]
        want = [f"Copy [<<modDirectory>>/{folder}/{source}] -> <<kotorDirectory>>/Override"]
        ok = mapping[f"{archive}|{source}"] == want
        matched, failed = matched + ok, failed + (not ok)
        print(f"{'match' if ok else 'FAIL':5} mapper {archive}|{source}: {mapping[f'{archive}|{source}']}")
    game = {"Override/a.tga": "FOMOD\n", "Override/o.txt": "O\n", "chitin.key": "KEY\n"}
    for name, spec in FOMOD_CASES.items():
        case = fomod_case(*spec)
        for mode in ("validate", "install"):
            actual = run_rust(binary, mode, case, work)
            tree = actual["tree"]["game"] if actual["tree"] else None
            ok = actual["accepted"] and (mode == "validate" or tree == game)
            matched, failed = matched + ok, failed + (not ok)
            print(
                f"{'match' if ok else 'FAIL':5} {name}/{mode}: rust accepted={actual['accepted']} game={tree}"
                f" | expected accepted=True game={game if mode == 'install' else None}"
                + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
            )
    return matched, failed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--fomod-map", help="reference FOMOD mapper (csharp-fomod-map/) to run live")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="archive-member-brackets-proof-")
    try:
        if args.csharp:
            oracle = {
                name: {mode: run_csharp(args.csharp, args.reference, mode, case, work) for mode in ("pipeline", "install")}
                for name, case in CASES.items()
            }
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        if args.fomod_map:
            mapping = run_fomod_map(args.fomod_map, args.reference)
            if args.record:
                with open(FOMOD_MAP, "w") as handle:
                    json.dump(mapping, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(FOMOD_MAP) as handle:
                mapping = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            for mode, reference_mode in {**MODES, **(MANAGED if has_process(case) else {})}.items():
                expected = oracle[name][reference_mode]
                actual = run_rust(args.bin, mode, case, work)
                expected_tree = expected["tree"] if mode in ("install", "checkpoint", "managed") and expected["accepted"] else None
                same = actual["accepted"] == expected["accepted"] and actual["tree"] == expected_tree
                extra = ""
                if mode == "pipeline":
                    same = same and actual["dry_run"] == expected["dry_run"] and actual["skipped"] == expected["skipped"]
                    extra = (f" dry_run={actual['dry_run']}/{expected['dry_run']}"
                             f" skipped={actual['skipped']}/{expected['skipped']}")
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
                    f"{verdict:5} {name}/{mode}: rust accepted={actual['accepted']} tree={actual['tree']}"
                    f" | reference accepted={expected['accepted']} tree={expected_tree}{extra}"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        fomod_matched, fomod_failed = check_fomod(args.bin, work, mapping)
        matched += fomod_matched
        failed += fomod_failed
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
