#!/usr/bin/env python3
"""Execute Arguments proof: drives the production `modsync`.

Each case is a disposable mod/game fixture plus a TOML guide with one Execute step
that runs `mods/args.sh` with the case's Arguments. The script writes each argument
it receives, one per line as `<arg>`, to `mods/args.txt`, so the final tree shows
how the Arguments text was split.

C# (Instruction.ExecuteProgramAsync) substitutes the root placeholders into the
Arguments text and hands the string to ProcessStartInfo.Arguments
(PlatformAgnosticMethods.GetProcessStartInfo). .NET splits that string with the
Windows command-line rules on every platform: only space and tab separate
arguments, double quotes group, backslashes are literal unless they precede a
double quote, single quotes and `#` are ordinary characters.

The Rust binary runs `validate --output json --dry-run` (the validation pipeline),
plain `validate`, `install`, `install --component-checkpoints` and
`install --component-checkpoints --managed`, all with `--allow-processes`, on fresh
copies. The pipeline is compared with the reference InstallationValidationPipeline
(DryRun, file selection): the overall decision, the dry-run decision and the
DryRunSkipped classification. The other modes are compared with the reference
install, and for an accepted install the final game and mod files (names and
contents, with the fixture folder written as `<ROOT>`). Reference results are in
csharp-oracle.json.

Usage:
  probe.py [--bin target/debug/modsync] [--csharp path/to/probe.dll --reference DIR [--record]]

With --csharp, the reference oracle is run live (network namespace isolated with
`unshare -rn`) instead of using the recorded results; --record also rewrites
csharp-oracle.json from that run. See ../execute-arguments-contract.md.
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
GUID = "77777777-8888-8888-8888-888888888888"
KD = "<<kotorDirectory>>"
MD = "<<modDirectory>>"

SCRIPT = (
    "#!/bin/sh\n"
    'out="$(dirname "$0")/args.txt"\n'
    ': > "$out"\n'
    'for a in "$@"; do printf \'<%s>\\n\' "$a" >> "$out"; done\n'
)

FILES = {
    "game/chitin.key": "KEY\n",
    "game/Override/o.txt": "O\n",
    "mods/args.sh": SCRIPT,
}

# name -> Arguments text, or (Arguments text, game folder name)
CASES = {
    "plain": "a b",
    "no_arguments": "",
    "double_quoted": '"a b" c',
    "empty_quoted": '"" x',
    "tab_separated": "a\tb",
    "escaped_double_quote": 'a\\"b',
    "doubled_quote_in_quotes": '"a""b"',
    "trailing_backslash_in_quotes": '"C:\\dir\\\\" x',
    "dollar_literal": "$HOME `x`",
    "single_quoted": "'a b'",
    "backslash_path": "sub\\file.txt",
    "backslash_space": "a\\ b",
    "hash_word": "x #y z",
    "newline": "a\nb",
    "placeholder_backslash": KD + "\\Override",
    "placeholder_forward": KD + "/Override " + MD,
    "windows_flag_value": '--dir="C:\\Program Files\\x" -q',
    # Known differences (deliberate, see KNOWN).
    "unclosed_quote": '"a b',
    # `\"` escapes the closing quote, as on Windows, so the quote stays open.
    "quoted_placeholder_backslash": '"' + KD + '\\Override\\"',
    "unknown_placeholder": "<<foo>>",
    "lowercase_placeholder": "<<kotordirectory>>",
    "root_with_space": (KD, "game dir"),
}


def toml_string(value):
    return json.dumps(value)


def guide(arguments):
    lines = [
        "[[thisMod]]",
        f'Guid = "{GUID}"',
        'Name = "Execute Arguments Probe"',
        "IsSelected = true",
        "",
        "[[thisMod.Instructions]]",
        'Action = "Execute"',
        f'Source = [{toml_string(MD + "/args.sh")}]',
        f"Arguments = {toml_string(arguments)}",
    ]
    return "\n".join(lines) + "\n"


def spec(case):
    return case if isinstance(case, tuple) else (case, "game")


def fixture(root, case):
    arguments, game_name = spec(case)
    for relative in ("scratch", "home", "xdg", "mods", game_name):
        os.makedirs(os.path.join(root, relative), exist_ok=True)
    for relative, text in FILES.items():
        relative = relative.replace("game/", game_name + "/", 1)
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as handle:
            handle.write(text)
        if path.endswith(".sh"):
            os.chmod(path, 0o755)
    path = os.path.join(root, "guide.toml")
    with open(path, "w") as handle:
        handle.write(guide(arguments))
    return path, os.path.join(root, "mods"), os.path.join(root, game_name)


def files(base, root):
    result = {}
    for directory, dirs, names in os.walk(base):
        # Rust keeps its recovery and checkpoint state in hidden folders.
        dirs[:] = [d for d in dirs if not d.startswith(".")]
        for name in names:
            full = os.path.join(directory, name)
            key = os.path.relpath(full, base).replace(os.sep, "/")
            with open(full) as handle:
                result[key] = handle.read().replace(root, "<ROOT>")
    return dict(sorted(result.items()))


def trees(mods, game, root):
    return {"game": files(game, root), "mods": files(mods, root)}


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
    argv = [binary, verb, "-i", path, "-s", mods, "-g", game, "--allow-processes"]
    if verb == "install":
        argv += ["--scratch-dir", os.path.join(root, "scratch")]
    if mode == "checkpoint":
        argv.append("--component-checkpoints")
    if mode == "managed":
        argv += ["--component-checkpoints", "--managed", "--staging-dir", os.path.join(root, "staging"),
                 "--manifest-dir", os.path.join(root, "manifests")]
    if mode == "pipeline":
        argv += ["--output", "json", "--dry-run"]
    env = dict(os.environ, HOME=os.path.join(root, "home"), XDG_CONFIG_HOME=os.path.join(root, "xdg"))
    done = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=300)
    accepted = done.returncode == 0
    result = {
        "accepted": accepted,
        "tree": trees(mods, game, root) if accepted and verb == "install" else None,
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
        result = {"accepted": accepted, "tree": trees(mods, game, root), "raw": raw}
    make_writable(root)
    return result


# Known differences: name -> (Rust modes, reason, Rust args.txt). Rust must refuse
# where the reference accepts or, when an args.txt is given, accept and pass exactly
# those arguments; otherwise the check fails.
# Plain validate writes nothing, so the last two are compared normally there.
INSTALLS = ("install", "checkpoint", "managed")
EVERY = ("pipeline", "validate") + INSTALLS
KNOWN = {
    "unclosed_quote": (EVERY, "C# closes an unclosed double quote at the end; Rust refuses malformed quoting before any program runs", None),
    "quoted_placeholder_backslash": (EVERY, "`\\\"` is an escaped quote, so the opening quote is never closed; C# closes it at the end, Rust refuses", None),
    "unknown_placeholder": (EVERY, "C# passes an unknown <<name>> through as text; Rust refuses it", None),
    "lowercase_placeholder": (INSTALLS, "C# replaces placeholders case-sensitively; Rust matches them case-insensitively", "<<ROOT>/game>\n"),
    "root_with_space": (INSTALLS, "C# substitutes roots before splitting, so a root with a space becomes two arguments; Rust keeps one", "<<ROOT>/game dir>\n"),
}

# Rust mode -> reference mode.
MODES = {"pipeline": "pipeline", "validate": "install", "install": "install", "checkpoint": "install",
         "managed": "install"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=os.path.join(ROOT, "target", "debug", "modsync"))
    parser.add_argument("--csharp")
    parser.add_argument("--reference")
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    work = tempfile.mkdtemp(prefix="execute-arguments-proof-")
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
        matched = known = failed = 0
        for name, case in CASES.items():
            for mode, reference_mode in MODES.items():
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
                if is_known and KNOWN[name][2] is not None:
                    # Rust accepts and passes the documented argument instead of refusing.
                    refused = actual["accepted"] and actual["tree"]["mods"].get("args.txt") == KNOWN[name][2]
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
        print(f"{matched} match / {known} known difference / {failed} fail")
        return 1 if failed else 0
    finally:
        make_writable(work)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
