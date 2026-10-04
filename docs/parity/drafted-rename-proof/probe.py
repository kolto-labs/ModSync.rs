#!/usr/bin/env python3
"""Drafted Rename proof: drives the production `modsync`.

Each case is one component whose Directions prose the natural-language drafter turns
into instructions (`convert --parse-directions`, or Markdown ingestion on load). The
fixture is `game/{chitin.key, Override/}` and `mods/old.tga`. C# `RenameFile` takes a
Rename Destination as the new file name in the source's folder; the drafter gave every
drafted Rename a `<<kotorDirectory>>\\Override` Destination (and, for "rename a.tga to
b.tga", the new name as a second Source), which Rust refuses (C# never renames it as
meant). The drafter now keeps the new name as a bare file name. Per case, on a fresh
copy each:

  draft    `convert IN D --parse-directions --format json`: the drafted instructions
           (Action, Source, Destination) and `DraftReview.RequiresReview`;
  csharp   the same drafted instructions against the pinned C# drafter
           (csharp-drafts.json, recorded with tools/check_drafts.py);
  gate     `install -i D -s mods -g game --no-checkpoint` on the unreviewed draft
           fails and writes nothing (the review gate);
  review   `convert IN R --parse-directions --review-draft GUID --format json`
           succeeds (it refuses a draft with unparsed prose);
  plain    `validate -i R -s mods -g game` exit code;
  full_dry `validate -i R ... --output json --full --dry-run` exit code and `success`;
  install  `install -i R -s mods -g game --no-checkpoint` exit code and every file
           under `game/` and `mods/` with its text afterwards.

The Markdown case also runs `validate` and `install` on the `.md` guide itself, which
drafts on load and must stop at the review gate. Expected results are in EXPECTED;
`csharp` differences listed in KNOWN are the documented intentional ones.

Usage:
  probe.py [--bin target/release/modsync] [--cases-json PATH] [--verbose]

--cases-json writes the JSON cases (the input for tools/check_drafts.py) and exits.
See ../drafted-rename-contract.md.
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
CSHARP = os.path.join(HERE, "csharp-drafts.json")
MD, KD = "<<modDirectory>>", "<<kotorDirectory>>"
OLD = MD + "\\old.tga"
OVERRIDE = KD + "\\Override"

# name -> Directions prose
CASES = {
    "rename_file": "Rename the file old.tga to new.tga.",
    "rename_quoted": "Rename the file 'old.tga' to 'new.tga'.",
    "rename_short": "Rename old.tga to new.tga.",
    "rename_as": "Rename the file old.tga as new.tga.",
    # Fail closed: no usable new file name, or no Rename drafted at all.
    "no_extension": "Rename the file old.tga to newname.",
    "folder_part": "Rename the file old.tga to sub/new.tga.",
    "pronoun": "Take the two copied files and rename them to PLC_CompPnl_b, retaining their extensions.",
    "placeholder": "Rename the file old.tga to <<kotorDirectory>>/Override/new.tga.",
}
MARKDOWN = "markdown"
GUIDS = {name: "77777777-8888-9999-aaaa-%012d" % index
         for index, name in enumerate(list(CASES) + [MARKDOWN], 1)}

FILES = {"game/chitin.key": "KEY\n", "mods/old.tga": "old\n"}
BASELINE = sorted([name, text] for name, text in FILES.items())
RENAMED = sorted([["game/chitin.key", "KEY\n"], ["mods/new.tga", "old\n"]])

RENAMES = {"draft": [[["Rename", [OLD], "new.tga"]], True], "gate": True, "review": True,
           "plain": True, "full_dry": [True, True], "install": [True, RENAMED]}
REFUSED = {"gate": True, "review": True, "plain": False, "full_dry": [False, False],
           "install": [False, BASELINE]}
EXPECTED = {
    "rename_file": RENAMES, "rename_quoted": RENAMES, "rename_short": RENAMES, "rename_as": RENAMES,
    MARKDOWN: dict(RENAMES, md_validate=False, md_install=[False, BASELINE]),
    "no_extension": dict(draft=[[["Rename", [OLD], OVERRIDE]], True], **REFUSED),
    "folder_part": dict(draft=[[["Rename", [OLD], OVERRIDE]], True], **REFUSED),
    "pronoun": dict(draft=[[["Rename", [MD + "\\them"], OVERRIDE]], True], **REFUSED),
    # The unparsed prose blocks the review, so nothing runs.
    "placeholder": {"draft": [[], True], "gate": True, "review": False},
}
INTENTIONAL = ("Rust drafts the new name as a bare file name (and not as a second Source); "
               "the pinned C# drafter gives a placeholder Destination that every install refuses")
KNOWN = {name: INTENTIONAL for name in ("rename_file", "rename_quoted", "rename_short", "rename_as")}


def cases_json():
    return [{"Guid": GUIDS[name], "Name": name, "Directions": prose, "IsSelected": True}
            for name, prose in CASES.items()]


def markdown_guide():
    return f"""# Probe build

## Mod List

### Rename mod
**Name:** Rename mod
**Author:** Probe
**Description:** Probe.
**Installation Instructions:** {CASES["rename_file"]}
<!--<<ModSync>>
Guid: {GUIDS[MARKDOWN]}
IsSelected: true
-->
___
"""


def fixture(root):
    os.makedirs(os.path.join(root, "game", "Override"))
    os.makedirs(os.path.join(root, "home"))
    for name, text in FILES.items():
        os.makedirs(os.path.dirname(os.path.join(root, name)), exist_ok=True)
        with open(os.path.join(root, name), "w") as handle:
            handle.write(text)


def tree(root):
    out = []
    for folder in ("game", "mods"):
        for current, dirs, files in os.walk(os.path.join(root, folder)):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                full = os.path.join(current, name)
                with open(full) as handle:
                    out.append([os.path.relpath(full, root).replace(os.sep, "/"), handle.read()])
    return sorted(out)


class Runner:
    def __init__(self, binary, work, verbose):
        self.binary, self.work, self.verbose = binary, work, verbose

    def run(self, root, *argv):
        env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
        done = subprocess.run([self.binary, *argv], capture_output=True, text=True, env=env, cwd=root,
                              timeout=600, stdin=subprocess.DEVNULL)
        if self.verbose:
            text = (done.stdout + done.stderr).strip().splitlines()
            print(f"    {argv[0]} rc={done.returncode}: {text[-1] if text else ''}")
        return done

    def fresh(self, label):
        root = tempfile.mkdtemp(prefix=label + "-", dir=self.work)
        fixture(root)
        return root

    def guide_run(self, guide_text, suffix, label, mode):
        """`mode` on a guide in a fresh fixture: plain, full_dry or install."""
        root = self.fresh(label)
        path = os.path.join(root, "guide" + suffix)
        with open(path, "w") as handle:
            handle.write(guide_text)
        common = ["-i", path, "-s", os.path.join(root, "mods"), "-g", os.path.join(root, "game")]
        if mode == "install":
            done = self.run(root, "install", *common, "--no-checkpoint")
            return [done.returncode == 0, tree(root)]
        if mode == "full_dry":
            done = self.run(root, "validate", *common, "--output", "json", "--full", "--dry-run")
            try:
                success = json.loads(done.stdout[done.stdout.index("{"):]).get("success")
            except ValueError:
                success = None
            return [done.returncode == 0, success]
        return self.run(root, "validate", *common).returncode == 0

    def convert(self, source_text, suffix, label, review):
        root = self.fresh(label)
        source = os.path.join(root, "in" + suffix)
        with open(source, "w") as handle:
            handle.write(source_text)
        out = os.path.join(root, "out.json")
        argv = ["convert", source, out, "--parse-directions", "--format", "json"]
        if review:
            argv += ["--review-draft", review]
        done = self.run(root, *argv)
        if done.returncode != 0:
            return None
        with open(out) as handle:
            return handle.read()


def drafted(text, guid):
    component = next(c for c in json.loads(text) if c["Guid"].lower() == guid)
    steps = [[i["Action"], i["Source"], i.get("Destination") or ""] for i in component.get("Instructions", [])]
    return [steps, (component.get("DraftReview") or {}).get("RequiresReview")]


def run_case(runner, name):
    guid = GUIDS[name]
    if name == MARKDOWN:
        source, suffix = markdown_guide(), ".md"
    else:
        source, suffix = json.dumps([c for c in cases_json() if c["Guid"] == guid]), ".json"
    out = {}
    draft = runner.convert(source, suffix, name + "-draft", None)
    out["draft"] = drafted(draft, guid) if draft else None
    out["gate"] = draft is not None and runner.guide_run(draft, ".json", name + "-gate", "install") == [False, BASELINE]
    reviewed = runner.convert(source, suffix, name + "-review", guid)
    out["review"] = reviewed is not None
    if reviewed is not None:
        for mode in ("plain", "full_dry", "install"):
            out[mode] = runner.guide_run(reviewed, ".json", f"{name}-{mode}", mode)
    if name == MARKDOWN:
        out["md_validate"] = runner.guide_run(source, ".md", name + "-md-validate", "plain")
        out["md_install"] = runner.guide_run(source, ".md", name + "-md-install", "install")
    return out


def csharp_check(name, actual, oracle):
    """The drafted instructions against the pinned C# drafter's for the same prose."""
    reference = next(c for c in oracle if c["Guid"].lower() == GUIDS[name])
    expected = [[i["Action"], i["Source"], i.get("Destination") or ""] for i in reference["Instructions"]]
    return expected == actual["draft"][0], expected


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--cases-json")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    if args.cases_json:
        with open(args.cases_json, "w") as handle:
            json.dump(cases_json(), handle, indent=1)
            handle.write("\n")
        return 0
    with open(CSHARP) as handle:
        oracle = json.load(handle)
    work = tempfile.mkdtemp(prefix="drafted-rename-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    runner = Runner(os.path.abspath(args.bin), work, args.verbose)
    matched = known = failed = 0
    try:
        for name in list(CASES) + [MARKDOWN]:
            if args.verbose:
                print(f"  {name}")
            actual = run_case(runner, name)
            results = []
            for check, expected in EXPECTED[name].items():
                same = actual.get(check) == expected
                results.append(("match" if same else "FAIL", check,
                                f" | rust={actual.get(check)} | expected={expected}"))
            if name != MARKDOWN:
                same, expected = csharp_check(name, actual, oracle) if actual["draft"] else (False, None)
                if name in KNOWN:
                    verdict = "known" if not same and actual["draft"] == EXPECTED[name]["draft"] else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                results.append((verdict, "csharp", f" | rust={actual['draft'] and actual['draft'][0]} | reference={expected}"))
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
