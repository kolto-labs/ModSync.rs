#!/usr/bin/env python3
"""Rename file name proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide in which component `A`
renames `mods/a/x.txt` to the case's Destination and component `B` copies `b/x.txt`
into `Override` and `b.txt` into the game folder. C# `Instruction.RenameFile` joins
the raw Destination to the source's folder and its validation never checks a Rename
Destination with `PathValidator.IsValidPath`, so on Linux C# renames to names that
are invalid or unsafe on Windows (`..` inside the name, a control character, a
trailing dot or space, a device name). Rust refused some of these (`relative`) and
accepted others; it now refuses every one. Both sides run, on a fresh copy each,
with `--use-file-selection`:

  full_dry  `validate -i G -s mods -g game --output json --full --dry-run`;
  basic     `validate ... --output json --dry-run` (no `--full`);
  install   `install -i G -s mods -g game --no-checkpoint`; C# also gets `-y` but not
            `--skip-validation`, so it validates first;
  plain     (Rust only) `validate -i G -s mods -g game`.

Checks:
  full_dry/decision, basic/decision  exit code is zero or not, and `success`;
  full_dry/counts     `errorCount`, `warningCount`, `passedCount`;
  full_dry/stages     each stage's name and `passed`;
  install             exit code is zero or not, and every file under `game/` and
                      `mods/` with its text afterwards (C#'s `.modsync` folder left out);
  plain               Rust's plain `validate` decides as C# `install`.

Drafter cases (DRAFTS): Directions prose whose new name the engine refuses, drafted
with `convert --parse-directions --format json`, reviewed with `--review-draft`, then
installed. Checks: `draft` (the drafted Action, Source and Destination), `review`
(succeeds), `install` (fails, nothing written) and `csharp` (the drafted steps against
the pinned C# drafter's, csharp-drafts.json, the `0-csharp.json` output of
tools/check_drafts.py over the cases `--cases-json` writes).

The full pipeline's Environment stage needs a HoloPatcher in C#'s `Resources` folder;
the probe copies the reference build into its temporary directory and adds
`Resources/holopatcher`, a shell script that exits 2 (see
../full-pipeline-destination-proof/README.md).

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]
  probe.py --cases-json PATH

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../rename-file-name-contract.md.
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
KD, MD = "<<kotorDirectory>>", "<<modDirectory>>"
GA = "77777777-8888-9999-aaaa-eeeeeeeeeee1"
GB = "77777777-8888-9999-aaaa-eeeeeeeeeee2"
OPTION = "77777777-8888-9999-aaaa-eeeeeeeeeee3"
UNSELECTED = "77777777-8888-9999-aaaa-eeeeeeeeeee9"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def copy(source, destination=KD + "/Override"):
    return step("Copy", [MD + "/" + source], destination, "Overwrite = true")


def rename(destination, extra=""):
    return step("Rename", [MD + "/a/x.txt"], destination, extra)


def component(guid, name, steps, option_steps=None):
    """A selected component; `option_steps` gives it an unselected option `O`."""
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}", "IsSelected = true", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    if option_steps is not None:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "O"', "IsSelected = false", ""]
        for s in option_steps:
            lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return "\n".join(lines)


def guide(*a_steps, option_steps=None):
    """`A` with the given steps, then `B`."""
    return (component(GA, "A", a_steps, option_steps) + "\n"
            + component(GB, "B", [copy("b/x.txt"), copy("b.txt", KD)]))


CASES = {
    # Newly refused: Rust renamed to these names (or, for `./` and a trailing `/`,
    # to `z.txt`); C# on Linux renames to the name as written.
    "dotdot_inside": guide(rename("z..txt")),
    "dotdot_leading": guide(rename("..z.txt")),
    "control_tab": guide(rename("z\t.txt")),
    "control_soh": guide(rename("z\u0001.txt")),
    "dot_part": guide(rename("./z.txt")),
    "trailing_separator": guide(rename("z.txt/")),
    # Already refused by `relative` before this slice (unchanged).
    "trailing_dot": guide(rename("z.txt.")),
    "trailing_space": guide(rename("z.txt ")),
    "device_name": guide(rename("con.txt")),
    "brackets": guide(rename("z[1].txt")),
    # Controls: plain names, a step excluded by its Dependencies, and the same step
    # in an unselected option `O` of `A` (`A`'s first step chooses it).
    "file_name": guide(rename("z.txt")),
    "dots_and_spaces": guide(rename("z .a.txt")),
    "excluded": guide(rename("z..txt", f'Dependencies = ["{UNSELECTED}"]')),
    "unselected_option": guide(step("Choose", [OPTION]), option_steps=[rename("z..txt")]),
}

ALL = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision", "install", "plain")
NEW = ("Rust refuses a Rename Destination that is not one valid Windows file name (fail closed); "
       "C# validation never checks a Rename Destination and its Linux install renames to the name "
       "as written")
OLD = ("Unchanged: Rust's `relative` already refused this Rename Destination; C# renames to it on Linux")
# name -> {check: reason}
REPORTS = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision")
KNOWN = {name: {check: NEW for check in ALL}
         for name in ("dotdot_inside", "dotdot_leading", "control_tab", "control_soh", "dot_part")}
# C#'s install fails this one too (`File.Move` to `.../z.txt/`); only its reports pass.
KNOWN["trailing_separator"] = {check: NEW for check in REPORTS}
OPTION_REPORT = ("Rust's ComponentValidation (archive) stage checks the Rename names of every option, "
                 "selected or not, as it already did for a folder part; C# never checks them. The "
                 "unselected option is not run, so plain `validate` and `install` pass as in C#")
KNOWN["unselected_option"] = {check: OPTION_REPORT for check in REPORTS}
KNOWN.update({name: {check: OLD for check in ALL}
              for name in ("trailing_dot", "trailing_space", "device_name", "brackets")})


def known_ok(check, actual):
    """The Rust side of each known difference is the documented refusal: both reports
    fail and, for an install or plain check, nothing is written."""
    reports = not actual["full_dry/decision"][0] and not actual["basic/decision"][0]
    if check in REPORTS:
        return reports
    return reports and not actual["plain"] and actual["install"] == [False, BASELINE]


FILES = {"game/chitin.key": "KEY\n", "mods/a/x.txt": "from A\n", "mods/b/x.txt": "from B\n",
         "mods/b.txt": "b\n"}
BASELINE = sorted([name, text] for name, text in FILES.items())


def fixture(root, guide):
    for folder in ("game/Override", "home"):
        os.makedirs(os.path.join(root, folder))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    out = []
    for folder in ("game", "mods"):
        base = os.path.join(root, folder)
        for current, dirs, files in os.walk(base):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                full = os.path.join(current, name)
                with open(full) as handle:
                    out.append([os.path.relpath(full, root).replace(os.sep, "/"), handle.read()])
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
        if mode in ("full_dry", "basic"):
            for key, value in result.items():
                if mode == "full_dry" or key == "decision":
                    out[f"{mode}/{key}"] = value
        else:
            out[mode] = result
    return out, texts


# Drafter cases: Directions prose drafted with `convert --parse-directions`. The
# drafter keeps a Rename's new name as a bare Destination only when the engine takes
# it, so these keep the placeholder Destination, as the pinned C# drafter drafts.
CSHARP_DRAFTS = os.path.join(HERE, "csharp-drafts.json")
OLD_TGA = MD + "\\old.tga"
OVERRIDE = KD + "\\Override"
DRAFTS = {
    "draft_device_as": "Rename the file old.tga as con.tga.",
    "draft_device_to": "Rename the file old.tga to con.tga.",
    "draft_dotdot_as": "Rename the file old.tga as new..tga.",
    "draft_leading_as": "Rename the file old.tga as ..new.tga.",
}
DRAFT_GUIDS = {name: "77777777-8888-9999-aaaa-f%011d" % index for index, name in enumerate(DRAFTS, 1)}
DRAFT_FILES = {"game/chitin.key": "KEY\n", "mods/old.tga": "old\n"}
DRAFT_BASELINE = sorted([name, text] for name, text in DRAFT_FILES.items())
DRAFT_EXPECTED = {"draft": [["Rename", [OLD_TGA], OVERRIDE]], "review": True,
                  "install": [False, DRAFT_BASELINE]}
ONE_SOURCE = ("Rust drafts one Source for a Rename (drafted-rename-contract.md); the pinned C# "
              "drafter adds the new name from \"old.tga to con.tga\" as a second Source")
DRAFT_KNOWN = {"draft_device_to": ONE_SOURCE}


def draft_cases_json():
    return [{"Guid": DRAFT_GUIDS[name], "Name": name, "Directions": prose, "IsSelected": True}
            for name, prose in DRAFTS.items()]


def draft_case(binary, work, name):
    """The drafted steps, the review, and `install` of the reviewed draft."""
    root = tempfile.mkdtemp(prefix=f"rust-{name}-", dir=work)
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "home"))
    for file, text in DRAFT_FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, file)), exist_ok=True)
        with open(os.path.join(root, file), "w") as handle:
            handle.write(text)
    source = os.path.join(root, "in.json")
    with open(source, "w") as handle:
        json.dump([c for c in draft_cases_json() if c["Name"] == name], handle)
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))

    def run(*argv):
        return subprocess.run([binary, *argv], capture_output=True, text=True, env=env, cwd=root,
                              timeout=600, stdin=subprocess.DEVNULL)

    out = {}
    drafted = os.path.join(root, "draft.json")
    reviewed = os.path.join(root, "reviewed.json")
    run("convert", source, drafted, "--parse-directions", "--format", "json")
    with open(drafted) as handle:
        component = json.load(handle)[0]
    out["draft"] = [[i["Action"], i["Source"], i.get("Destination") or ""]
                    for i in component.get("Instructions", [])]
    done = run("convert", source, reviewed, "--parse-directions", "--format", "json",
               "--review-draft", DRAFT_GUIDS[name])
    out["review"] = done.returncode == 0
    done = run("install", "-i", reviewed, "-s", os.path.join(root, "mods"), "-g",
               os.path.join(root, "game"), "--no-checkpoint")
    out["install"] = [done.returncode == 0, tree(root)]
    return out, (done.stdout + done.stderr)


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
    parser.add_argument("--cases-json", help="write the drafter cases (tools/check_drafts.py input) and exit")
    args = parser.parse_args()
    if args.cases_json:
        with open(args.cases_json, "w") as handle:
            json.dump(draft_cases_json(), handle, indent=1)
            handle.write("\n")
        return 0
    work = tempfile.mkdtemp(prefix="rename-file-name-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
            actual, texts = run_case([args.bin], guide, work, "rust", False)
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
        with open(CSHARP_DRAFTS) as handle:
            csharp_drafts = {c["Guid"].lower(): c for c in json.load(handle)}
        for name in DRAFTS:
            actual, text = draft_case(os.path.abspath(args.bin), work, name)
            if args.verbose:
                print(f"  rust {name}/install: {last_line(text)!r}")
            reference = csharp_drafts[DRAFT_GUIDS[name]]
            expected_csharp = [[i["Action"], i["Source"], i.get("Destination") or ""]
                               for i in reference["Instructions"]]
            results = [("match" if actual[check] == value else "FAIL", check,
                        f" | rust={actual[check]} | expected={value}")
                       for check, value in DRAFT_EXPECTED.items()]
            same = actual["draft"] == expected_csharp
            if name in DRAFT_KNOWN:
                verdict = "known" if not same and actual["draft"] == DRAFT_EXPECTED["draft"] else "FAIL"
            else:
                verdict = "match" if same else "FAIL"
            results.append((verdict, "csharp", f" | rust={actual['draft']} | reference={expected_csharp}"))
            for verdict, check, detail in results:
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
