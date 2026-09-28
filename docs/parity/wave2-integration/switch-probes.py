#!/usr/bin/env python3
"""Direct-CLI acceptance probes for DeploymentAction::Switch.

Invokes the production `modsync` binary only; no Rust test harness. Every
probe uses its own disposable /tmp root and never touches another root.
"""
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

BINARY = os.environ.get(
    "MODSYNC_BIN", "/home/brunner56/.cache/modsync-rs-target-lead/debug/modsync"
)


def run(*args, cwd=None):
    proc = subprocess.run(
        [BINARY, *args], capture_output=True, text=True, cwd=cwd
    )
    return {"argv": [BINARY, *args], "exit": proc.returncode, "stdout": proc.stdout, "stderr": proc.stderr}


def active_profile(settings_dir):
    path = os.path.join(settings_dir, "settings.json")
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f).get("activeProfileName")


def sha256_file(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def lock_path(game_dir):
    game_dir = os.path.realpath(game_dir)
    key = hashlib.sha256(game_dir.encode()).hexdigest()[:16]
    return os.path.join(os.path.dirname(game_dir), f".modsync-lock-{key}")


def new_root():
    return tempfile.mkdtemp(prefix="modsync-switch-probe-")


def make_profile(settings_dir, name):
    return run("profile", "--action", "create", "--name", name, "--settings-dir", settings_dir)


def deploy(game_dir, settings_dir, profile, component, files):
    stage = os.path.join(settings_dir, "profiles", profile, "staging", component)
    os.makedirs(stage, exist_ok=True)
    for rel, content in files.items():
        with open(os.path.join(stage, rel), "w") as f:
            f.write(content)
    return run(
        "deployment", "deploy",
        "--game-dir", game_dir,
        "--profile", profile,
        "--settings-dir", settings_dir,
        "--component", component,
    )


def switch(game_dir, settings_dir, to, purge=False):
    args = ["deployment", "switch", "--game-dir", game_dir, "--to", to, "--settings-dir", settings_dir]
    if purge:
        args.append("--purge-current")
    return run(*args)


def check(results, name, condition, detail):
    results.append({"name": name, "passed": bool(condition), "detail": detail})


def probe_1(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    r = deploy(game, settings, "A", "comp1", {"file.txt": "hello"})
    check(results, "p1_deploy_ok", r["exit"] == 0, r)
    r = switch(game, settings, "B")
    check(results, "p1_no_purge_refused_with_live_current", r["exit"] != 0, r)
    check(results, "p1_active_unchanged", active_profile(settings) == "A", active_profile(settings))
    shutil.rmtree(root)


def probe_2(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    path = lock_path(game)
    fd = os.open(path, os.O_RDWR | os.O_CREAT)
    fcntl.flock(fd, fcntl.LOCK_EX)
    try:
        r = switch(game, settings, "B")
        check(results, "p2_refused_with_external_lock_held", r["exit"] != 0, r)
        check(results, "p2_active_unchanged", active_profile(settings) == "A", active_profile(settings))
    finally:
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)
    shutil.rmtree(root)


def probe_3(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    with open(os.path.join(settings, "profiles", "A.json"), "w") as f:
        f.write("not json")
    r = switch(game, settings, "B")
    check(results, "p3_corrupt_current_profile_refused", r["exit"] != 0, r)
    check(results, "p3_active_unchanged", active_profile(settings) == "A", active_profile(settings))
    shutil.rmtree(root)


def probe_4(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    r = switch(os.path.join(root, "does-not-exist"), settings, "B")
    check(results, "p4_missing_game_dir_refused", r["exit"] != 0, r)
    check(results, "p4_active_unchanged", active_profile(settings) == "A", active_profile(settings))
    shutil.rmtree(root)


def probe_5(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    r = switch(game, settings, "B")
    check(results, "p5_valid_empty_switch_succeeds", r["exit"] == 0, r)
    check(results, "p5_active_now_target", active_profile(settings) == "B", active_profile(settings))
    shutil.rmtree(root)


def probe_6(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "B", "--settings-dir", settings)
    shared = os.path.join(game, "shared.txt")
    with open(shared, "w") as f:
        f.write("original-bytes")
    original_hash = sha256_file(shared)
    r = deploy(game, settings, "B", "comp2", {"shared.txt": "modded-bytes"})
    check(results, "p6_deploy_over_existing_ok", r["exit"] == 0, r)
    r = switch(game, settings, "A", purge=True)
    check(results, "p6_purge_and_switch_succeeds", r["exit"] == 0, r)
    check(results, "p6_original_bytes_restored", sha256_file(shared) == original_hash, sha256_file(shared))
    check(results, "p6_active_is_target", active_profile(settings) == "A", active_profile(settings))
    shutil.rmtree(root)


def probe_7(results):
    root = new_root()
    game = os.path.join(root, "game")
    settings = os.path.join(root, "settings")
    os.makedirs(game)
    make_profile(settings, "A")
    make_profile(settings, "B")
    run("profile", "--action", "activate", "--name", "A", "--settings-dir", settings)
    r = deploy(game, settings, "A", "comp3", {"locked.txt": "aaa"})
    check(results, "p7_deploy_ok", r["exit"] == 0, r)
    mode = os.stat(game).st_mode
    os.chmod(game, 0o555)
    try:
        r = switch(game, settings, "B", purge=True)
        check(results, "p7_failed_purge_refused", r["exit"] != 0, r)
        check(results, "p7_active_unchanged", active_profile(settings) == "A", active_profile(settings))
    finally:
        os.chmod(game, mode)
    r = run("deployment", "status", "--game-dir", game, "--profile", "A", "--settings-dir", settings)
    manifests = json.loads(r["stdout"]) if r["exit"] == 0 else []
    check(results, "p7_manifest_retained", len(manifests) == 1, manifests)
    shutil.rmtree(root)


def main():
    results = []
    for probe in (probe_1, probe_2, probe_3, probe_4, probe_5, probe_6, probe_7):
        probe(results)
    receipt = {"binary": BINARY, "checks": results, "all_passed": all(c["passed"] for c in results)}
    out_path = os.path.join(os.path.dirname(__file__), "switch-probes.json")
    with open(out_path, "w") as f:
        json.dump(receipt, f, indent=2)
    print(json.dumps(receipt, indent=2))
    sys.exit(0 if receipt["all_passed"] else 1)


if __name__ == "__main__":
    main()
