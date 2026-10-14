#!/usr/bin/env python3
"""Extract Destination proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide. Component `A` extracts
`mods/m.zip` (`Override/a.tga`, `readme.txt`) with the case's Destination and, in
some cases, then moves or copies the extracted files; component `B` copies `b.txt`
into `Override`. C# validation (`ComponentValidation.ParseDestinationWithAction`,
AttemptFixes on by default) clears an Extract Destination, so C# always extracts to
`mods/m/`, next to the archive; Rust extracted into the Destination. Rust now never
uses it either. Both sides run, on a fresh copy each, with `--use-file-selection`:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`;
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts     `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages     each stage's name and `passed`;
  basic/counts        `errorCount`, `warningCount`, `passedCount`;
  install             exit code is zero or not, and every file under `game/` and
                      `mods/` with its text afterwards (C#'s `.modsync` folder left out);
  plain               Rust's plain `validate` decides as C# `install`.

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder;
the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2 (see
../full-pipeline-destination-proof/README.md).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../extract-destination-contract.md.
"""
import argparse
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ORACLE = os.path.join(HERE, "csharp-oracle.json")
KD, MD = "<<kotorDirectory>>", "<<modDirectory>>"
OV = KD + "/Override"
GA = "77777777-8888-9999-aaaa-ddddddddddd1"
GB = "77777777-8888-9999-aaaa-ddddddddddd2"
OPTION = "77777777-8888-9999-aaaa-ddddddddddd3"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def extract(destination=None):
    return step("Extract", [MD + "/m.zip"], destination)


def move(source, destination=OV):
    return step("Move", [MD + "/" + source], destination, "Overwrite = true")


def copy(source, destination=OV):
    return step("Copy", [MD + "/" + source], destination, "Overwrite = true")


def component(guid, name, steps, option_steps=None):
    """A selected component; `option_steps` gives it a selected option `O`."""
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    if option_steps is not None:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "O"', "IsSelected = true", ""]
        for s in option_steps:
            lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return "\n".join(lines)


def guide(*a_steps, option_steps=None):
    """`A` with the given steps, then `B`."""
    return (component(GA, "A", a_steps, option_steps) + "\n"
            + component(GB, "B", [copy("b.txt")]))


CASES = {
    # Changed: Rust extracted into the Destination; C# extracts to mods/m/.
    "override": guide(extract(OV)),
    "override_backslash": guide(extract(KD + "\\Override")),
    "game_root": guide(extract(KD)),
    "mod_folder": guide(extract(MD + "/out")),
    "missing_folder": guide(extract(MD + "/new")),
    # Changed decision: the guide's later step names the folder C# extracts to.
    "override_then_move": guide(extract(OV), move("m/Override/*")),
    "mod_folder_then_move": guide(extract(MD + "/out"), move("m/Override/*")),
    # Changed decision: a later step that names the Destination finds nothing there.
    "mod_folder_then_copy": guide(extract(MD + "/out"), copy("out/Override/a.tga")),
    # The same Extract in a chosen option.
    "option_override": guide(step("Choose", [OPTION]), option_steps=[extract(OV)]),
    # Unchanged: a Destination Rust refuses as a path (fail closed); C# clears it.
    "outside_roots": guide(extract(MD + "/../outside")),
    # Unchanged controls: no Destination, the archive's own folder, and a Delete
    # with a Destination (C# clears it too; Delete never used it).
    "no_destination": guide(extract(), move("m/Override/*")),
    "archive_folder": guide(extract(MD), move("m/Override/*")),
    "delete_destination": guide(extract(), step("Delete", [MD + "/m/readme.txt"], OV)),
}

ALL = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision",
       "basic/counts", "install", "plain")
REPORTS = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision",
           "basic/counts")
OUTSIDE = ("Rust refuses an Extract Destination outside the install roots as a path "
           "(fail closed), though it never uses it; C#'s validation clears it and installs")
# Both basic reports fail it (C#: the Destination; Rust: that and the unsafe path).
KNOWN = {"outside_roots": {check: OUTSIDE for check in ALL if check != "basic/decision"}}
SOURCE = ("Not this slice: C# checks a Source of a later step against the archives first "
          "(`VerifyExtractPaths`, two `Failed to find` errors here, and so no Destination "
          "error) and its full pipeline fails it in the Environment stage; Rust gives one "
          "error in its ComponentValidation stage. Both refuse")
KNOWN["mod_folder_then_copy"] = {check: SOURCE for check in
                                 ("full_dry/counts", "full_dry/stages", "basic/counts")}


def known_ok(check, actual):
    """The Rust side of each known difference is the documented refusal: both reports
    fail and, for an install or plain check, nothing is written."""
    reports = not actual["full_dry/decision"][0] and not actual["basic/decision"][0]
    if check in REPORTS:
        return reports
    return reports and not actual["plain"] and actual["install"] == [False, BASELINE]


def archive():
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w") as handle:
        handle.writestr("Override/a.tga", "a from m.zip\n")
        handle.writestr("readme.txt", "readme\n")
    return data.getvalue()


FILES = {"game/chitin.key": "KEY\n", "mods/b.txt": "b\n", "mods/out/keep.txt": "keep\n"}
ARCHIVE = "mods/m.zip"
BASELINE = sorted([name, text] for name, text in list(FILES.items()) + [[ARCHIVE, "<archive>"]])


def fixture(root, guide):
    for folder in ("game/Override", "home"):
        os.makedirs(os.path.join(root, folder))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    with open(os.path.join(root, ARCHIVE), "wb") as handle:
        handle.write(archive())
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    """Every file under game/ and mods/ with its text; the archive is `<archive>`."""
    out = []
    for folder in ("game", "mods"):
        base = os.path.join(root, folder)
        for current, dirs, files in os.walk(base):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                full = os.path.join(current, name)
                relative = os.path.relpath(full, root).replace(os.sep, "/")
                if relative == ARCHIVE:
                    out.append([relative, "<archive>"])
                    continue
                with open(full) as handle:
                    out.append([relative, handle.read()])
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
    return report_view(done.returncode, done.stdout), done.stdout


def run_case(argv_head, guide, work, label, csharp, extra_env=None):
    out, texts = {}, {}
    for mode in ("full_dry", "basic", "install") + (() if csharp else ("plain",)):
        result, text = run_one(argv_head, guide, work, label, mode, csharp, extra_env)
        texts[mode] = text
        if mode == "full_dry":
            for key, value in result.items():
                out[f"{mode}/{key}"] = value
        elif mode == "basic":
            out["basic/decision"] = result["decision"]
            out["basic/counts"] = result["counts"]
        else:
            out[mode] = result
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


def last_line(text):
    lines = [line for line in (text or "").strip().splitlines() if line.strip()]
    return lines[-1] if lines else ""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--verbose", action="store_true", help="print the last output line per run")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="extract-destination-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {}
            for name, guide in CASES.items():
                oracle[name], texts = run_case(head, guide, work, "csharp", True, env)
                if args.verbose:
                    print(f"  csharp {name}/install: {last_line(texts['install'])!r}")
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, guide in CASES.items():
            actual, texts = run_case([os.path.abspath(args.bin)], guide, work, "rust", False)
            expected = oracle[name]
            if args.verbose:
                for mode in ("plain", "install"):
                    print(f"  rust {name}/{mode}: {last_line(texts[mode])!r}")
            for check in list(expected) + ["plain"]:
                if check == "plain":
                    same = actual["plain"] == expected["install"][0]
                    detail = f" | rust={actual['plain']} | reference install={expected['install'][0]}"
                else:
                    same = actual[check] == expected[check]
                    detail = f" | rust={actual[check]} | reference={expected[check]}"
                if check in KNOWN.get(name, {}):
                    verdict = "known" if not same and known_ok(check, actual) else "FAIL"
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
