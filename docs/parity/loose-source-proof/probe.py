#!/usr/bin/env python3
"""Loose Source proof: drives the production `modsync`.

C# validation (`ComponentValidation.VerifyExtractPaths`, run by the CLI `install`
before installing and by the desktop Validate step) refuses a step whose Source in
the mod folder no archive of its component holds (`Failed to find '...' in any
archives!`), once the component extracts at least one archive that exists. Rust's
reports ran that check, but plain `validate` and `install` did not, so Rust
installed guides the C# CLI refuses. Each case is a disposable mod/game fixture plus
a TOML guide: component `A` (the case's steps, often extracting `mods/m.zip`, which
holds `Override/a.tga` and `readme.txt`) and component `B`, which copies the loose
`b.txt` into `Override` with no Extract. Both sides run, on a fresh copy each, with
`--use-file-selection`:

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
`Resources/holopatcher`, a shell script that exits 2.

Usage:
  probe.py [--bin target/release/modsync] [--reference DIR [--record]] [--verbose]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll), the C# CLI is
run live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists)
with DOTNET_ROLL_FORWARD=Major; --record also rewrites csharp-oracle.json. Without it
the recorded results are used. See ../loose-source-contract.md.
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
GA = "88888888-9999-aaaa-bbbb-ccccccccccc1"
GB = "88888888-9999-aaaa-bbbb-ccccccccccc2"
GZ = "88888888-9999-aaaa-bbbb-ccccccccccc3"
OPTION = "88888888-9999-aaaa-bbbb-ccccccccccc4"


def step(action, sources, destination=None, extra=""):
    lines = [f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    if extra:
        lines.append(extra)
    return lines


def extract(archive="m.zip"):
    return step("Extract", [MD + "/" + archive])


def move(source, destination=OV):
    return step("Move", [MD + "/" + source], destination, "Overwrite = true")


def copy(source, destination=OV, extra=""):
    return step("Copy", [MD + "/" + source], destination,
                "Overwrite = true" + ("\n" + extra if extra else ""))


def component(guid, name, steps, option_steps=None, option_selected=True, selected=True):
    """A component; `option_steps` gives it an option `O`."""
    lines = ["[[thisMod]]", f'Guid = "{guid}"', f"Name = {json.dumps(name)}",
             f"IsSelected = {'true' if selected else 'false'}", ""]
    for s in steps:
        lines += ["[[thisMod.Instructions]]"] + s + [""]
    if option_steps is not None:
        lines += ["[[thisMod.Options]]", f'Guid = "{OPTION}"', 'Name = "O"',
                  f"IsSelected = {'true' if option_selected else 'false'}", ""]
        for s in option_steps:
            lines += ["[[thisMod.Options.Instructions]]"] + s + [""]
    return "\n".join(lines)


def guide(*a_steps, name="A", option_steps=None, option_selected=True, extra=""):
    """`A` with the given steps, then `B` (a loose copy, no Extract), then `extra`."""
    return (component(GA, name, a_steps, option_steps, option_selected) + "\n"
            + component(GB, "B", [copy("b.txt")]) + ("\n" + extra if extra else ""))


CHOOSE = step("Choose", [OPTION])
CASES = {
    # Changed: a Source in the mod folder that no archive of the component holds.
    "loose_copy": guide(extract(), copy("loose/x.tga")),
    "loose_wildcard": guide(extract(), move("loose/*")),
    "loose_delete": guide(extract(), step("Delete", [MD + "/loose/x.tga"]), move("m/Override/*")),
    "selected_option_loose": guide(extract(), CHOOSE, option_steps=[copy("loose/x.tga")]),
    "unselected_option_loose": guide(extract(), move("m/Override/*"),
                                     option_steps=[copy("loose/x.tga")], option_selected=False),
    "unselected_option_extract": guide(copy("loose/x.tga"), option_steps=[extract()],
                                       option_selected=False),
    "renamed_source": guide(extract(), step("Rename", [MD + "/m/readme.txt"], "notes.txt"),
                            copy("m/notes.txt")),
    # `M` (first) extracts m.zip; `A` extracts n.zip and copies a file of m.zip.
    "other_component_archive": component(GZ, "M", [extract(), move("m/readme.txt")]) + "\n"
    + guide(extract("n.zip"), copy("m/Override/a.tga")),
    # Unchanged controls: Sources the component's archives hold, game Sources,
    # loose files in a component with no Extract, a step whose condition is not met,
    # a missing archive, and a component named "Improved AI", which C# exempts.
    "member_copy": guide(extract(), copy("m/Override/a.tga")),
    "member_wildcard": guide(extract(), move("m/Override/*")),
    "game_source": guide(extract(), step("Delete", [OV + "/old.tga"]), move("m/Override/*")),
    "no_extract": guide(copy("loose/x.tga"), step("Delete", [MD + "/loose/x.tga"])),
    "condition_unmet": guide(extract(), move("m/Override/*"),
                             copy("loose/x.tga", extra=f'Dependencies = ["{GZ}"]'),
                             extra=component(GZ, "Z", [copy("b.txt")], selected=False)),
    "missing_archive": guide(extract("gone.zip"), copy("loose/x.tga")),
    # An option that is not chosen: its archives that exist are searched, one Rust
    # cannot list is not needed when an earlier archive holds the Source, and a missing
    # one is left out, as in C#.
    "unchosen_dup_archive": guide(extract(), copy("m/Override/a.tga"),
                                  option_steps=[extract("dup.zip")], option_selected=False),
    "unchosen_link_archive": guide(extract(), copy("m/Override/a.tga"),
                                   option_steps=[extract("link.zip")], option_selected=False),
    "unchosen_missing_loose": guide(extract(), copy("loose/x.tga"),
                                    option_steps=[extract("gone.zip")], option_selected=False),
    "improved_ai": guide(extract(), copy("loose/x.tga"), name="Improved AI"),
    # Known, Rust accepts: C# matches members case-sensitively, and reads a
    # DelDuplicate's Sources (extensions) as paths.
    "member_case": guide(extract(), copy("M/OVERRIDE/A.TGA")),
    "delduplicate": guide(extract(), move("m/Override/*"),
                          step("DelDuplicate", [".tga", ".tpc"], OV)),
    # Known, Rust refuses: C# rewrites `d/Override/a.tga` to `d/d/Override/a.tga`
    # (AttemptFixes) and installs it.
    "doubled_folder": guide(extract("d.zip"), copy("d/Override/a.tga")),
}

ALL = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision",
       "basic/counts", "install", "plain")
REPORTS = ("full_dry/decision", "full_dry/counts", "full_dry/stages", "basic/decision",
           "basic/counts")
# Both refuse, but C# gives two errors for each such Source in its basic report (the
# check and its AttemptFixes retry with the first folder doubled), and its full
# pipeline stops in the Environment stage, which runs ComponentValidation first; Rust
# gives one error, in its ComponentValidation stage.
COUNTS = ("C# gives two `Failed to find` errors per Source and stops its full pipeline "
          "in the Environment stage; Rust gives one error in ComponentValidation. Both refuse")
REFUSED = ("loose_copy", "loose_wildcard", "loose_delete", "selected_option_loose",
           "unselected_option_loose", "unselected_option_extract", "renamed_source",
           "other_component_archive", "unchosen_missing_loose")
KNOWN = {name: {check: COUNTS for check in ("full_dry/counts", "full_dry/stages", "basic/counts")}
         for name in REFUSED}
KNOWN["missing_archive"] = {
    check: "Not this slice: C#'s full pipeline stops in the Environment stage; Rust "
           "reports the missing archive in ComponentValidation. Both refuse"
    for check in ("full_dry/counts", "full_dry/stages")}
FIXED = ("C# rewrites the Source to the doubled folder (AttemptFixes) and installs it; "
         "Rust refuses the Source as written (fail closed)")
# Both basic reports fail it: C# keeps the first error after its fix.
KNOWN["doubled_folder"] = {check: FIXED for check in ALL if check != "basic/decision"}
# Rust's side of these is the result of a control case C# accepts with the same files.
ACCEPTED = {"member_case": "member_copy", "delduplicate": "member_wildcard"}
for name in ACCEPTED:
    KNOWN[name] = {check: "C# refuses; Rust accepts and installs (see the contract)"
                   for check in ALL}


def known_ok(name, check, actual, oracle):
    """The Rust side of each known difference: for an ACCEPTED case, the C# result of
    its control case; otherwise the documented refusal, where both reports fail and,
    for an install or plain check, nothing is written."""
    if name in ACCEPTED:
        control = oracle[ACCEPTED[name]]
        return actual[check] == (control["install"][0] if check == "plain" else control[check])
    reports = not actual["full_dry/decision"][0] and not actual["basic/decision"][0]
    if check in REPORTS:
        return reports
    return reports and not actual["plain"] and actual["install"] == [False, BASELINE]


def archive(entries):
    """A ZIP of `entries`; a name starting with `link:` is a symbolic link entry."""
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w") as handle:
        for name, text in entries.items():
            if name.startswith("link:"):
                info = zipfile.ZipInfo(name[len("link:"):])
                info.create_system = 3
                info.external_attr = (0o120777 << 16)
                handle.writestr(info, text)
            else:
                handle.writestr(name, text)
    return data.getvalue()


FILES = {"game/chitin.key": "KEY\n", "game/Override/old.tga": "old\n", "mods/b.txt": "b\n",
         "mods/loose/x.tga": "loose x\n"}
ARCHIVES = {
    "mods/m.zip": {"Override/a.tga": "a from m.zip\n", "readme.txt": "readme\n"},
    "mods/n.zip": {"n.txt": "n\n"},
    "mods/d.zip": {"d/Override/a.tga": "a from d.zip\n"},
    # Rust's admission rules refuse these two (a case-duplicate entry, a link).
    "mods/dup.zip": {"x.txt": "x\n", "X.txt": "X\n"},
    "mods/link.zip": {"y.txt": "y\n", "link:z.txt": "y.txt"},
}
BASELINE = sorted([name, text] for name, text in
                  list(FILES.items()) + [[name, "<archive>"] for name in ARCHIVES])


def fixture(root, guide):
    os.makedirs(os.path.join(root, "home"))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)
    for name, entries in ARCHIVES.items():
        with open(os.path.join(root, name), "wb") as handle:
            handle.write(archive(entries))
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    """Every file under game/ and mods/ with its text; an archive is `<archive>`."""
    out = []
    for folder in ("game", "mods"):
        base = os.path.join(root, folder)
        for current, dirs, files in os.walk(base):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                full = os.path.join(current, name)
                relative = os.path.relpath(full, root).replace(os.sep, "/")
                if relative in ARCHIVES:
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
    work = tempfile.mkdtemp(prefix="loose-source-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
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
                    verdict = "known" if not same and known_ok(name, check, actual, oracle) else "FAIL"
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
