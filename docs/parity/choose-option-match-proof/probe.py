#!/usr/bin/env python3
"""Choose option matching proof: drives the production `modsync`.

C# `Instruction.GetChosenOptions` (Instruction.cs:1609-1611) runs a selected option
when its `Guid.ToString()` (lowercase D form) is an entry of the Choose Source,
compared OrdinalIgnoreCase. Install (ModComponent.cs:1648-1667) and the dry run
(DryRunValidator.ValidateInstallationAsync, which runs ExecuteInstructionsAsync) both
use it. So a braced, parenthesized, N-form or padded entry names no option, and an
entry that names no option is silently ignored.

Rust (contract ../choose-option-match-contract.md) uses the same text rule, and keeps
its fail-closed refusal of a Choose entry that names no option.

Each case is a disposable mod/game fixture: a component whose only instruction is a
Choose, an option `One` (Copy a.txt to Override) and an option `Two` (Copy b.txt).
The reference result (csharp-oracle.json) is, per case, the C# load (Choose Source
and option GUIDs), its dry run and its install (exits and the final game tree), from
csharp-oracle/Program.cs built against ModSync.Core.dll d31ccbd3. The expected Rust
result is derived from it:

- every Choose entry equals (OrdinalIgnoreCase) a reference option GUID: Rust must give
  the reference's decisions and final tree (`validate --output json --dry-run`
  against the dry run, plain `validate` and `install` against the install);
- some entry names no option: C# ignores it, Rust refuses with every verb before any
  write (the deliberate fail-closed difference).

The `gen_*` cases first run `modsync convert --auto-generate-local` (the binary under
test) on an authored guide with options and mods/Foo.zip; the generated guide is the
fixture for every run. C# generators write `Option.Guid.ToString()`, so the generated
guide must give the reference's result.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp ORACLE/probe.dll --reference REF [--record]]
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
COMPONENT = "33333333-3333-3333-3333-333333333333"
G1 = "a4444444-4444-4444-4444-44444444444a"
G2 = "b5555555-5555-5555-5555-55555555555b"
G9 = "99999999-9999-9999-9999-999999999999"
N1 = G1.replace("-", "")

# name -> (format, Choose Source (list or scalar string), option One GUID spelling,
#          option Two selected)
CASES = {
    "plain": ("toml", [G1], G1, False),
    "source_upper": ("toml", [G1.upper()], G1, False),
    "option_upper": ("toml", [G1], G1.upper(), False),
    "option_braced": ("toml", [G1], "{" + G1 + "}", False),
    "option_nform": ("toml", [G1], N1, False),
    "both_selected": ("toml", [G1, G2], G1, True),
    "unselected": ("toml", [G2], G1, False),
    "source_braced": ("toml", ["{" + G1 + "}"], G1, False),
    "source_paren": ("toml", ["(" + G1 + ")"], G1, False),
    "source_nform": ("toml", [N1], G1, False),
    "source_padded": ("toml", [" " + G1 + " "], G1, False),
    "scalar_braced": ("toml", "{" + G1 + "}", G1, False),
    "braced_and_plain": ("toml", ["{" + G1 + "}", G2], G1, True),
    "unknown": ("toml", [G9], G1, False),
    "unknown_and_known": ("toml", [G9, G1], G1, False),
    "not_a_guid": ("toml", ["foo"], G1, False),
    "legacy_single": ("md_legacy", G1, G1, False),
    "legacy_comma": ("md_legacy", f"{G1}, {G2}", G1, True),
    "legacy_semicolon": ("md_legacy", f"{G1};{G2}", G1, True),
    "legacy_braced": ("md_legacy", "{" + G1 + "}", G1, False),
    # Archive generation (`convert --auto-generate-local`) from authored options; the
    # third field lists (option GUID, selected, folders it moves). The binary under test
    # writes the guide every run uses, reference included.
    "gen_plain_option": ("generate", None, [(G1, True, ["FolderA"])], None),
    "gen_braced_option": ("generate", None, [("{" + G1.upper() + "}", True, ["FolderA"])], None),
    "gen_consolidate_braced": ("generate", None, [("{" + G1.upper() + "}", True, ["FolderA", "FolderC"]),
                                                  (G2, True, ["FolderA"])], None),
}
GENERATOR = None
EXT = {"toml": "toml", "md_legacy": "md", "generate": "toml"}
GENERATED = {name for name, case in CASES.items() if case[0] == "generate"}


def toml_guide(source, one, two_selected):
    value = json.dumps(source)
    return f"""[[thisMod]]
Guid = "{COMPONENT}"
Name = "Choose Probe"
IsSelected = true

[[thisMod.Instructions]]
Action = "Choose"
Source = {value}

[[thisMod.Options]]
Guid = "{one}"
Name = "One"
IsSelected = true

[[thisMod.Options.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/a.txt"]
Destination = "<<kotorDirectory>>/Override"

[[thisMod.Options]]
Guid = "{G2}"
Name = "Two"
IsSelected = {'true' if two_selected else 'false'}

[[thisMod.Options.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/b.txt"]
Destination = "<<kotorDirectory>>/Override"
"""


def legacy_guide(source, one, two_selected):
    def option(number, guid, name, selected, file):
        return [f"##### Option {number}", f"- **GUID:** {guid}", f"- **Name:** {name}",
                f"- **Is Selected:** {'true' if selected else 'false'}", "- **Instruction:**",
                "  - **Action:** Copy", f"  - **Source:** <<modDirectory>>/{file}",
                "  - **Destination:** <<kotorDirectory>>/Override"]
    meta = [f"- **GUID:** {COMPONENT}", "#### Instructions",
            "1. **GUID:** 00000000-0000-0000-0000-000000000001",
            "   **Action:** Choose", f"   **Source:** {source}", "#### Options"]
    meta += option(1, one, "One", True, "a.txt") + option(2, G2, "Two", two_selected, "b.txt")
    return ("# Guide\n\n## Mod List\n\n### Choose Probe\n\n**Name:** Choose Probe\n\n"
            "Some description.\n\n<!--<<ModSync>>\n" + "\n".join(meta) + "\n-->\n")


def authored_guide(options):
    lines = ["[[thisMod]]", f'Guid = "{COMPONENT}"', 'Name = "Choose Probe"', "IsSelected = true", "",
             '[thisMod.ResourceRegistry."https://example.invalid/Foo.zip".Files]', '"Foo.zip" = true']
    for number, (guid, selected, folders) in enumerate(options, 1):
        lines += ["", "[[thisMod.Options]]", f'Guid = "{guid}"', f'Name = "Authored{number}"',
                  f"IsSelected = {'true' if selected else 'false'}"]
        for folder in folders:
            lines += ["", "[[thisMod.Options.Instructions]]", 'Action = "Move"',
                      f"Source = ['<<modDirectory>>\\Foo\\{folder}\\*']",
                      "Destination = '<<gameDirectory>>\\Override'", "Overwrite = true"]
    return "\n".join(lines) + "\n"


def generate(binary, root, options):
    """Authored guide plus mods/Foo.zip, then `modsync convert --auto-generate-local`."""
    import zipfile
    mods = os.path.join(root, "mods")
    with zipfile.ZipFile(os.path.join(mods, "Foo.zip"), "w") as archive:
        for folder in ("FolderA", "FolderB", "FolderC"):
            archive.writestr(f"{folder}/{folder[-1].lower()}.tga", folder + "\n")
    authored = os.path.join(root, "authored.toml")
    with open(authored, "w") as handle:
        handle.write(authored_guide(options))
    path = os.path.join(root, "guide.toml")
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    subprocess.run([binary, "convert", authored, path, "-f", "toml", "--auto-generate-local",
                    "--source-path", mods], capture_output=True, text=True, env=env, timeout=300, check=True)
    return path


def fixture(root, name, binary=None):
    fmt, source, one, two_selected = CASES[name]
    mods = os.path.join(root, "mods")
    game = os.path.join(root, "game")
    os.makedirs(os.path.join(game, "Override"))
    os.makedirs(mods)
    os.makedirs(os.path.join(root, "scratch"))
    for file, text in (("a.txt", "A\n"), ("b.txt", "B\n")):
        with open(os.path.join(mods, file), "w") as handle:
            handle.write(text)
    if fmt == "generate":
        return generate(binary or GENERATOR, root, one), mods, game
    path = os.path.join(root, "guide." + EXT[fmt])
    with open(path, "w") as handle:
        handle.write((toml_guide if fmt == "toml" else legacy_guide)(source, one, two_selected))
    return path, mods, game


def tree(game):
    result = {}
    for directory, _, files in os.walk(game):
        for file in files:
            full = os.path.join(directory, file)
            with open(full) as handle:
                result[os.path.relpath(full, game).replace(os.sep, "/")] = handle.read()
    return dict(sorted(result.items()))


def run_rust(binary, mode, name, work):
    root = tempfile.mkdtemp(prefix=f"rust-{mode}-", dir=work)
    path, mods, game = fixture(root, name, binary)
    verb = "install" if mode == "install" else "validate"
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game]
    if mode == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "dryrun":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    result = {
        "accepted": done.returncode == 0,
        "tree": tree(game) if mode == "install" else None,
        "stderr_tail": done.stderr.strip().splitlines()[-1:] if done.returncode else [],
    }
    if mode == "dryrun":
        report = json.loads(done.stdout)
        # A guide that fails to load has errorCount 0 and an "error" field.
        result["accepted"] = done.returncode == 0 and report["errorCount"] == 0 and "error" not in report
        if not result["accepted"]:
            issues = report.get("dryRun", {}).get("issues", []) + report.get("issues", [])
            errors = [i.get("message", "") for i in issues if i.get("severity") == "Error"]
            result["stderr_tail"] = (errors or [report.get("error", "")])[-1:]
    return result


def run_csharp(dll, reference, name, work):
    """Reference load, dry run and install, each on its own fixture, without network."""
    out = {}
    for mode in ("load", "dryrun", "install"):
        root = tempfile.mkdtemp(prefix=f"csharp-{mode}-", dir=work)
        path, mods, game = fixture(root, name)
        env = dict(os.environ, HOME=os.path.join(root, "home"), DOTNET_CLI_TELEMETRY_OPTOUT="1",
                   DOTNET_ROLL_FORWARD="Major")
        argv = ["unshare", "-rn", "dotnet", dll, reference, mode, path, mods, game]
        done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=300)
        lines = [line for line in done.stdout.splitlines() if line.startswith("@@JSON@@")]
        value = json.loads(lines[-1][len("@@JSON@@"):]) if lines else None
        if mode == "load":
            out["load"] = value
        elif mode == "dryrun":
            out["dryrun_errors"] = None if value is None else value["Errors"]
        else:
            out["install_exits"] = None if value is None else value["Exits"]
            out["install_tree"] = tree(game)
    return out


def expected(reference, name=None):
    """Expected Rust decision per mode, derived from the reference."""
    refused = {m: {"accepted": False, "tree": {}} for m in ("dryrun", "validate", "install")}
    load = reference["load"]
    if not load:
        return "reference refuses to load", refused
    component = load[0]
    options = [o["Guid"].lower() for o in component["Options"]]
    entries = [s for i in component["Instructions"] if i["Action"] == "Choose" for s in i["Source"]]
    # C# generators write `Option.Guid.ToString()` (AutoInstructionGenerator.cs:1911,
    # :1916, :2058, :2063), so a generated guide must name its options: anything else
    # is a Rust generator bug, not the fail-closed difference.
    if name not in GENERATED and not all(entry.lower() in options for entry in entries):
        return "an entry names no option: C# ignores it, Rust refuses (fail-closed)", refused
    installed = reference["install_exits"] == ["Choose Probe: Success"]
    return "every entry names an option: reference result", {
        "dryrun": {"accepted": reference["dryrun_errors"] == [], "tree": {}},
        "validate": {"accepted": installed, "tree": {}},
        "install": {"accepted": installed, "tree": reference["install_tree"] if installed else {}},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    global GENERATOR
    GENERATOR = args.bin
    work = tempfile.mkdtemp(prefix="choose-option-match-proof-")
    try:
        recorded = {}
        if os.path.exists(ORACLE):
            with open(ORACLE) as handle:
                recorded = json.load(handle)
        matched = failed = 0
        if args.csharp:
            live = {name: run_csharp(args.csharp, args.reference, name, work) for name in CASES}
            for name in CASES:
                same = recorded.get(name) == live[name]
                if not args.record:
                    matched, failed = matched + same, failed + (not same)
                    print(f"{'match' if same else 'FAIL':5} {name}/reference: live {live[name]}")
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(live, handle, indent=1, sort_keys=True)
                    handle.write("\n")
                recorded = live
        for name in CASES:
            basis, wanted = expected(recorded[name], name)
            for mode in ("dryrun", "validate", "install"):
                actual = run_rust(args.bin, mode, name, work)
                want = wanted[mode]
                same = actual["accepted"] == want["accepted"]
                if mode == "install":
                    same = same and actual["tree"] == want["tree"]
                matched, failed = matched + same, failed + (not same)
                print(
                    f"{'match' if same else 'FAIL':5} {name}/{mode}: rust accepted={actual['accepted']}"
                    + (f" tree={sorted(actual['tree'])}" if mode == "install" else "")
                    + f" | expected accepted={want['accepted']}"
                    + (f" tree={sorted(want['tree'])}" if mode == "install" else "")
                    + f" ({basis})"
                    + (f" | rust: {actual['stderr_tail'][0]}" if actual["stderr_tail"] else "")
                )
        print(f"{matched} match / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
