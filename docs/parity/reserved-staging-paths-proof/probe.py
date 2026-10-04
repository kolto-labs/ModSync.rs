#!/usr/bin/env python3
"""Reserved device names and Peer staging folders in instruction paths: drives the
production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide with one component
whose step names a Windows device name (`COM¹`, `LPT³`, `CONIN$`, `COM0`, ...) or a
path inside a Peer download staging folder (`<<modDirectory>>/.peer-<nonce>.part/`),
or a control name that must keep working (`COM10`, `com1x.txt`, `CLOCK$`, a game-root
`.peer-<nonce>.part`, ...). Every Rust run gets a fresh copy, `--use-file-selection`:

  plain     `validate -i G -s mods -g game`;
  basic     `validate ... --output json --dry-run`;
  full_dry  `validate ... --output json --full --dry-run` (the desktop Validate step);
  full      `validate ... --output json --full`;
  install   `install -i G -s mods -g game --no-checkpoint`.

Checks per case:
  plain, basic, full_dry, full   decision (exit code zero or not) is the expected one;
  install                        decision and the files afterwards: a refused case
                                 leaves the fixture unchanged (no write), an allowed one
                                 installs what the reference installs;
  wording                        (refused cases) plain `validate`, `install` and the
                                 full report all carry the refusal (`Reserved filename`
                                 or `Peer download staging folder`);
  reference/full_dry, reference/install
                                 Rust's decision (and install files) against the pinned
                                 C# CLI; `KNOWN` lists where Rust deliberately refuses
                                 what C# accepts.

Then Rust-only checkpoint checks: a store written by --writer-bin (a build from before
this slice) while the game held device names stays listable, restorable and
recoverable; a new checkpoint session refuses a device name already on disk with
`Checkpoint cannot record <path>: Windows device name` before any game or mod write.

Usage:
  probe.py [--bin target/release/modsync] [--writer-bin OLD] [--reference DIR [--record]]

With --reference (the d31ccbd3 build output holding ModSync.Core.dll) the C# CLI runs
live under `unshare -Urnm` (no network, a tmpfs over `/Override` when it exists) with
DOTNET_ROLL_FORWARD=Major and a HoloPatcher stub in a copy of its Resources folder (its
full pipeline needs one); --record rewrites csharp-oracle.json. Without it the recorded
results are used. See ../reserved-staging-paths-contract.md.
"""
import argparse
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
G1 = "66666666-7777-8888-9999-aaaaaaaaaaa1"
NONCE = "0123456789abcdef0123456789abcdef"
STAGING = f".peer-{NONCE}.part"
RESERVED, STAGED = "Reserved filename", "Peer download staging folder"


def step(action, sources, destination=None):
    lines = ["[[thisMod.Instructions]]", f'Action = "{action}"', f"Source = {json.dumps(sources)}"]
    if destination is not None:
        lines.append(f"Destination = {json.dumps(destination)}")
    return lines


def guide(*steps):
    lines = ["[[thisMod]]", f'Guid = "{G1}"', 'Name = "A"', "IsSelected = true", ""]
    for s in steps:
        lines += s + [""]
    return "\n".join(lines)


def copy(destination, source=MD + "/m.txt", action="Copy"):
    return guide(step(action, [source], destination))


# name -> (guide, extra mod files {relative: text}, extra zips {name: [members]}, refusal or None)
CASES = {
    # Reserved device names (refused on every host after this slice).
    "com_sup1": (copy(KD + "/COM¹"), {}, {}, RESERVED),
    "com_sup2_nested": (copy(KD + "/Override/COM²"), {}, {}, RESERVED),
    "com_sup3_lower_ext": (copy(KD + "/com³.d"), {}, {}, RESERVED),
    "lpt_sup1": (copy(KD + "/LPT¹"), {}, {}, RESERVED),
    "lpt_sup2_mods": (copy(MD + "/LPT²"), {}, {}, RESERVED),
    "lpt_sup3_ext": (copy(KD + "/LPT³.txt"), {}, {}, RESERVED),
    "conin": (copy(KD + "/CONIN$"), {}, {}, RESERVED),
    "conout_lower_ext": (copy(KD + "/conout$.txt"), {}, {}, RESERVED),
    "com0": (copy(KD + "/COM0"), {}, {}, RESERVED),
    "lpt0_ext": (copy(KD + "/lpt0.d"), {}, {}, RESERVED),
    "space_before_ext": (copy(KD + "/CON .d"), {}, {}, RESERVED),
    "source_name": (copy(KD + "/Override", MD + "/COM¹.txt"), {"COM¹.txt": "s\n"}, {}, RESERVED),
    "rename_bare": (guide(step("Rename", [MD + "/m.txt"], "LPT².txt")), {}, {}, RESERVED),
    "archive_member": (guide(step("Extract", [MD + "/dev.zip"])), {}, {"dev.zip": ["CONOUT$.txt"]}, RESERVED),
    # Controls: not device names.
    "control_override": (copy(KD + "/Override"), {}, {}, None),
    "control_com10": (copy(KD + "/COM10"), {}, {}, None),
    "control_com1x": (copy(KD + "/com1x"), {}, {}, None),
    "control_com_sup_twice": (copy(KD + "/COM¹¹"), {}, {}, None),
    "control_lpt_sup_word": (copy(KD + "/LPT²x"), {}, {}, None),
    "control_coninx": (copy(KD + "/CONINx"), {}, {}, None),
    "control_clock": (copy(KD + "/CLOCK$"), {}, {}, None),
    "control_member_com1x": (guide(step("Extract", [MD + "/ok.zip"])), {}, {"ok.zip": ["com1x.txt"]}, None),
    "control_source_com1x": (copy(KD + "/Override", MD + "/com1x.txt"), {"com1x.txt": "s\n"}, {}, None),
    # Peer staging folders under <<modDirectory>> (refused after this slice).
    "staging_destination": (copy(MD + "/" + STAGING), {}, {}, STAGED),
    "staging_subfolder": (copy(MD + "/" + STAGING + "/sub"), {}, {}, STAGED),
    "staging_upper_case": (copy(MD + "/" + STAGING.upper()), {}, {}, STAGED),
    "staging_nested": (copy(MD + "/sub/" + STAGING), {}, {}, STAGED),
    "staging_move": (copy(MD + "/" + STAGING, action="Move"), {}, {}, STAGED),
    "staging_source": (copy(KD + "/Override", MD + "/" + STAGING + "/s.txt"), {}, {}, STAGED),
    "staging_wildcard": (copy(KD + "/Override", MD + "/.peer-*.part/*.txt"), {}, {}, STAGED),
    "staging_extract": (guide(step("Extract", [MD + "/" + STAGING + "/s.zip"])), {}, {}, STAGED),
    # A file (not a folder) named like one; the scan hides only folders (fail closed).
    "staging_named_file": (copy(KD + "/Override", MD + "/.peer-notes.part"), {".peer-notes.part": "n\n"}, {}, STAGED),
    # Controls: not a staging folder, or not under <<modDirectory>>.
    "control_staging_in_game": (copy(KD + "/" + STAGING), {}, {}, None),
    "control_no_dot": (copy(MD + "/" + STAGING[1:]), {}, {}, None),
    "control_partial": (copy(MD + "/" + STAGING + "ial"), {}, {}, None),
    "control_empty_nonce": (copy(MD + "/.peer-.part"), {}, {}, None),
}

FAIL_CLOSED = "Rust refuses a Windows device name C# PathValidator does not list (fail closed)"
STAGING_ONLY = "Rust-only folder: C# has no Peer staging folders and writes there"
# name -> {check: reason}: Rust deliberately refuses what C# accepts.
KNOWN = {name: {"reference/full_dry": FAIL_CLOSED, "reference/install": FAIL_CLOSED}
         for name in ("com_sup1", "com_sup2_nested", "com_sup3_lower_ext", "lpt_sup1", "lpt_sup2_mods",
                      "lpt_sup3_ext", "conin", "conout_lower_ext", "space_before_ext", "source_name",
                      "rename_bare", "archive_member")}
KNOWN.update({name: {"reference/full_dry": STAGING_ONLY, "reference/install": STAGING_ONLY}
              for name in ("staging_destination", "staging_subfolder", "staging_upper_case",
                           "staging_nested", "staging_move", "staging_source", "staging_wildcard",
                           "staging_extract", "staging_named_file")})


def fixture(root, case):
    text, extra, zips, _ = case
    for folder in ("game/Override", "mods/sub", f"mods/{STAGING}", "home"):
        os.makedirs(os.path.join(root, folder))
    files = {"game/chitin.key": "KEY\n", "mods/m.txt": "m\n", f"mods/{STAGING}/s.txt": "s\n"}
    files.update({f"mods/{name}": body for name, body in extra.items()})
    for name, body in files.items():
        with open(os.path.join(root, name), "w") as handle:
            handle.write(body)
    # Every placeholder Destination folder exists, so each report mode passes a control
    # and the refusals come from the name alone.
    for line in text.splitlines():
        if line.startswith("Destination = "):
            value = json.loads(line.split(" = ", 1)[1])
            for prefix, folder in ((KD, "game"), (MD, "mods")):
                if value.startswith(prefix):
                    os.makedirs(os.path.join(root, folder, value[len(prefix) + 1:]), exist_ok=True)
    zips = dict(zips, **{f"{STAGING}/s.zip": ["y.txt"]})
    for name, members in zips.items():
        with zipfile.ZipFile(os.path.join(root, "mods", name), "w") as archive:
            for member in members:
                archive.writestr(member, "z\n")
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(text)
    return path, os.path.join(root, "mods"), os.path.join(root, "game")


def tree(root):
    out = []
    for folder in ("game", "mods"):
        for current, dirs, files in os.walk(os.path.join(root, folder)):
            dirs[:] = sorted(d for d in dirs if d != ".modsync")
            for name in files:
                out.append(os.path.relpath(os.path.join(current, name), root).replace(os.sep, "/"))
    return sorted(out)


def run_one(head, case, work, label, mode, csharp, extra_env):
    root = tempfile.mkdtemp(prefix=f"{label}-{mode}-", dir=work)
    path, mods, game = fixture(root, case)
    before = tree(root)
    common = ["-i", path, "-s", mods, "-g", game, "--use-file-selection"]
    if mode == "install":
        argv = head + ["install"] + common + ["--no-checkpoint"] + (["-y"] if csharp else [])
    elif mode == "plain":
        argv = head + ["validate"] + common
    else:
        flags = {"full_dry": ["--full", "--dry-run"], "full": ["--full"], "basic": ["--dry-run"]}[mode]
        argv = head + ["validate"] + common + ["--output", "json"] + flags
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"),
               **(extra_env or {}))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    after = tree(root)
    return {"ok": done.returncode == 0, "text": done.stdout + done.stderr,
            "files": after, "unchanged": after == before}


def run_case(head, case, work, label, csharp, extra_env=None):
    modes = ("full_dry", "install") if csharp else ("plain", "basic", "full_dry", "full", "install")
    return {mode: run_one(head, case, work, label, mode, csharp, extra_env) for mode in modes}


def stub_reference(reference, work):
    copy_dir = os.path.join(work, "reference")
    shutil.copytree(reference, copy_dir, ignore=shutil.ignore_patterns("Logs"))
    os.makedirs(os.path.join(copy_dir, "Resources"), exist_ok=True)
    stub = os.path.join(copy_dir, "Resources", "holopatcher")
    with open(stub, "w") as handle:
        handle.write("#!/bin/sh\nexit 2\n")
    os.chmod(stub, 0o755)
    return copy_dir


CHECKPOINT_GUIDE = copy(KD + "/Override")
DEVICE_FILES = ("COM0", "CONIN$", "COM\u00b9")


def checkpoint_root(work, label, device_files):
    root = tempfile.mkdtemp(prefix=f"checkpoint-{label}-", dir=work)
    for folder in ("game/Override", "mods", "home"):
        os.makedirs(os.path.join(root, folder))
    files = {"game/chitin.key": "KEY\n", "mods/m.txt": "m\n"}
    files.update({f"game/Override/{name}": "d\n" for name in device_files})
    for name, body in files.items():
        with open(os.path.join(root, name), "w") as handle:
            handle.write(body)
    with open(os.path.join(root, "guide.toml"), "w") as handle:
        handle.write(CHECKPOINT_GUIDE)
    return root


def run_cli(binary, root, *argv):
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run([binary, *argv], capture_output=True, text=True, env=env, cwd=root, timeout=600,
                          stdin=subprocess.DEVNULL)
    return done.returncode, done.stdout + done.stderr


def report(text):
    return json.loads(text[text.index("{"):text.rindex("}") + 1])


def checkpoint_checks(binary, writer, work):
    """(check, passed, detail) for checkpoint stores and new captures; Rust only."""
    out = []
    roots_of = lambda root: ["--source-dir", os.path.join(root, "mods"), "--game-dir", os.path.join(root, "game")]
    # (a) A store an earlier build wrote while the game held device names stays usable.
    if writer:
        root = checkpoint_root(work, "legacy", DEVICE_FILES)
        roots = roots_of(root)
        code, text = run_cli(writer, root, "checkpoint", "capture", *roots)
        out.append(("legacy_store/written_by_writer", code == 0, text[-300:]))
        if code == 0:
            captured = report(text)
            session, checkpoint = captured["sessionId"], captured["checkpointId"]
            for name, argv in (
                ("list", ["list"]),
                ("verify", ["verify", "--session", session]),
                ("history", ["history", "--session", session]),
                ("inspect", ["inspect", "--session", session, "--checkpoint", checkpoint]),
                ("provenance", ["provenance", "--session", session, "--tree", "game", "--path", "Override/COM0"]),
            ):
                code, text = run_cli(binary, root, "checkpoint", *argv, *roots)
                out.append((f"legacy_store/{name}", code == 0, text[-300:]))
            # Restore puts the recorded device names back; its safety capture records
            # the folders as they are.
            with open(os.path.join(root, "game", "new.txt"), "w") as handle:
                handle.write("x\n")
            code, text = run_cli(binary, root, "checkpoint", "restore", "--session", session,
                                 "--checkpoint", checkpoint, *roots)
            files = sorted(os.listdir(os.path.join(root, "game", "Override")))
            restored = code == 0 and files == sorted(DEVICE_FILES) and not os.path.exists(
                os.path.join(root, "game", "new.txt"))
            out.append(("legacy_store/restore", restored, f"rc={code} Override={files} {text[-200:]}"))
            # Recovery restores the unfinished session's boundary with the device names on disk.
            code, text = run_cli(binary, root, "checkpoint", "recover", *roots)
            ok = code == 0 and report(text).get("recoveryError") is None
            out.append(("legacy_store/recover", ok, text[-300:]))
            code, text = run_cli(binary, root, "checkpoint", "list", *roots)
            out.append(("legacy_store/list_after_recover", code == 0, text[-300:]))
    else:
        print("skip  legacy_store/* (no --writer-bin)")
    # (b) A new checkpoint session refuses a device name already in the game folder,
    # before any game or mod write.
    for name, argv in (
        ("component_checkpoints", ["install", "-i", "{guide}", "-s", "{mods}", "-g", "{game}",
                                   "--use-file-selection", "--component-checkpoints"]),
        ("capture", ["checkpoint", "capture", "--source-dir", "{mods}", "--game-dir", "{game}"]),
    ):
        root = checkpoint_root(work, name, ("CONIN$",))
        before = tree(root)
        values = {"guide": os.path.join(root, "guide.toml"), "mods": os.path.join(root, "mods"),
                  "game": os.path.join(root, "game")}
        code, text = run_cli(binary, root, *[a.format(**values) for a in argv])
        wanted = "Checkpoint cannot record Override/CONIN$: Windows device name"
        out.append((f"new_capture/{name}", code != 0 and wanted in text and tree(root) == before,
                    f"rc={code} unchanged={tree(root) == before} {text[-300:]}"))
        if name == "component_checkpoints":
            # The whole-run recovery snapshot was taken first: `restore` clears it, and an
            # install without checkpoints still works with the file present.
            code, text = run_cli(binary, root, "restore", "--source-dir", values["mods"],
                                 "--game-dir", values["game"])
            out.append(("new_capture/restore_snapshot", code == 0, text[-300:]))
            code, text = run_cli(binary, root, "install", "-i", values["guide"], "-s", values["mods"],
                                 "-g", values["game"], "--use-file-selection")
            installed = os.path.exists(os.path.join(root, "game", "Override", "m.txt"))
            out.append(("new_capture/install_without_checkpoints", code == 0 and installed, text[-300:]))
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "modsync"))
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--writer-bin", help="a build from before the wider device-name rule (e756730)")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="reserved-staging-paths-proof-", dir=os.environ.get("MODSYNC_PROOF_OUT"))
    try:
        if args.reference:
            reference = stub_reference(os.path.abspath(args.reference), work)
            script = 'if [ -d /Override ]; then mount -t tmpfs none /Override; fi; exec dotnet "$0" "$@"'
            head = ["unshare", "-Urnm", "sh", "-c", script, os.path.join(reference, "ModSync.Core.dll")]
            env = {"DOTNET_ROLL_FORWARD": "Major", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}
            oracle = {}
            for name, case in CASES.items():
                runs = run_case(head, case, work, "csharp", True, env)
                oracle[name] = {"full_dry": runs["full_dry"]["ok"],
                                "install": [runs["install"]["ok"], runs["install"]["files"]]}
            if args.record:
                with open(ORACLE, "w") as handle:
                    json.dump(oracle, handle, indent=1, sort_keys=True, ensure_ascii=False)
                    handle.write("\n")
        else:
            with open(ORACLE) as handle:
                oracle = json.load(handle)
        matched = known = failed = 0
        for name, case in CASES.items():
            refusal = case[3]
            runs = run_case([args.bin], case, work, "rust", False)
            results = {}
            for mode in ("plain", "basic", "full_dry", "full"):
                results[mode] = (runs[mode]["ok"] == (refusal is None), f"ok={runs[mode]['ok']}")
            install = runs["install"]
            if refusal:
                same = not install["ok"] and install["unchanged"]
            else:
                same = install["ok"] and install["files"] == oracle[name]["install"][1]
            results["install"] = (same, f"ok={install['ok']} files={install['files']}")
            if refusal:
                carried = [mode for mode in ("plain", "full_dry", "install") if refusal in runs[mode]["text"]]
                results["wording"] = (len(carried) == 3, f"carried by {carried}")
            reference = oracle[name]
            results["reference/full_dry"] = (runs["full_dry"]["ok"] == reference["full_dry"],
                                             f"rust={runs['full_dry']['ok']} reference={reference['full_dry']}")
            rust_install = [install["ok"], install["files"]]
            results["reference/install"] = (rust_install == reference["install"],
                                            f"rust={rust_install} reference={reference['install']}")
            for check, (same, detail) in results.items():
                if check in KNOWN.get(name, {}):
                    # A known difference holds only while Rust's own checks pass.
                    own = all(results[c][0] for c in results if not c.startswith("reference/"))
                    verdict = "known" if not same and own else "FAIL"
                else:
                    verdict = "match" if same else "FAIL"
                matched += verdict == "match"
                known += verdict == "known"
                failed += verdict == "FAIL"
                print(f"{verdict:5} {name}/{check}{'' if verdict == 'match' else ' | ' + detail}")
        for check, same, detail in checkpoint_checks(args.bin, args.writer_bin, work):
            verdict = "match" if same else "FAIL"
            matched += same
            failed += not same
            print(f"{verdict:5} checkpoint/{check}{'' if same else ' | ' + detail}")
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
