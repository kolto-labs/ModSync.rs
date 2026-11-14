#!/usr/bin/env python3
"""Thin repository-local adapters for existing archive and install tools."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import tarfile
import zipfile
import artstore_client
import fullrun_archive_manifest
import cache
import gui_diag
import fullrun_contract as contract

HERE = Path(__file__).resolve().parent


def checked(command, log, env=None):
    log = Path(log)
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, check=False, env=env)
    if result.returncode:
        raise ValueError("stage failed; inspect its retained encrypted evidence")
    return result


def installer_environment():
    return {name: value for name, value in cache.scrubbed_env().items()
            if not gui_diag.SECRET_ENV_NAME.search(name)
            and not name.startswith(("SSH_", "GIT_CONFIG_"))}


def selected_seed_ids(root, games):
    import seed_archives
    pins = seed_archives.load_pins(seed_archives.DEFAULT_PINS)
    paths = guides(root, games)[1::2]
    selected = sorted(item["id"] for item in cache.discover(paths) if item["id"] in pins)
    if not selected:
        raise ValueError("no reviewed seed pins match the requested guide scope")
    return selected


def guides(root, games):
    result = []
    for game in games:
        if game not in ("k1", "k2"):
            raise ValueError("invalid game")
        path = Path(root) / "content" / game / "full.md"
        if path.is_symlink() or not path.is_file():
            raise ValueError("pinned guide file is unavailable")
        result.extend(["--guide", str(path)])
    return result


def remote_options():
    if os.environ.get("RCLONE_CONFIG_MODCACHE_ACCESS_KEY_ID") and os.environ.get("MOD_CACHE_BUCKET"):
        bucket = os.environ["MOD_CACHE_BUCKET"]
        if not bucket or any(c in bucket for c in "/\\:\r\n"):
            raise ValueError("invalid cache bucket name")
        return ["--remote", "MODCACHE:" + bucket + "/modsync-archives"]
    return []


def cache_key(path):
    def stable(value):
        if isinstance(value, dict):
            return {key: stable(item) for key, item in value.items() if key not in ("checked_at", "imported_at")}
        if isinstance(value, list):
            return [stable(item) for item in value]
        return value
    document = json.loads(Path(path).read_text())
    canonical = json.dumps(stable(document), sort_keys=True, separators=(",", ":")).encode()
    return "mod-archives-" + hashlib.sha256(canonical).hexdigest()[:16]


def identity(engine, guide_root, engine_sha, guide_sha, output):
    actual_engine = subprocess.check_output(["git", "-C", str(engine), "rev-parse", "HEAD"], text=True).strip()
    actual_guide = subprocess.check_output(["git", "-C", str(guide_root), "rev-parse", "HEAD"], text=True).strip()
    if actual_engine != contract.exact_sha(engine_sha) or actual_guide != contract.exact_sha(guide_sha):
        raise ValueError("checked-out engine or guide does not match the admitted immutable revision")
    contract.write_json(output, {"schema_version": 1, "engine_sha": actual_engine,
                                "guide_sha": actual_guide, "execution_repository": os.environ.get("GITHUB_REPOSITORY"),
                                "execution_sha": os.environ.get("GITHUB_SHA"),
                                "artifact_repository": os.environ.get("ARTSTORE_REPOSITORY") or os.environ.get("GITHUB_REPOSITORY"),
                                "plan_id": os.environ.get("PLAN_ID"), "run_id": os.environ.get("GITHUB_RUN_ID"),
                                "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "cross_lane_parity": "unverified"})


def uncached_options(games):
    options = ["--allow-uncached-by-policy", "Optional Install Assistant - Quicker TSLPatching::Quicker TSLPatching"]
    for game in games:
        cutscene = ("Optional Widescreen & Extended Widescreen Support Mods::Remastered Cutscenes" if game == "k1"
                    else "Enhanced Widescreen Support Mods::K2 Cutscenes Rescaled")
        options += ["--allow-uncached-by-policy", cutscene]
    return options


def refresh(args, games):
    guides(args.guides, games)
    fullrun_archive_manifest.recover(args.cache, args.guides, games)
    cache_root = Path(args.cache)
    cache_root.parent.mkdir(parents=True, exist_ok=True)
    evidence = Path(args.output)
    receipts = []
    # Restore verifies every required archive; the cache cap is a storage policy,
    # never permission to count an unfetched OVER_LIMIT object as verified.
    for game in games:
        report_path = evidence / ("verify-%s.json" % game)
        with tempfile.TemporaryDirectory(prefix="archive-verification-", dir=cache_root.parent) as temporary:
            checked([sys.executable, str(HERE / "cache.py"), "restore", *guides(args.guides, [game]),
                     "--cache-dir", args.cache, "--destination", temporary, "--report", str(report_path),
                     *uncached_options([game]), *remote_options()], evidence / ("verify-%s-private.log" % game))
            report = json.loads(report_path.read_text())
            entries = report.get("entries")
            expected = {item["id"] for item in cache.discover(guides(args.guides, [game])[1::2])}
            if (report.get("ok") is not True or not isinstance(entries, list) or not entries
                    or len(entries) != len(expected) or {entry.get("id") for entry in entries} != expected
                    or any(entry.get("status") not in ("restored", cache.UNCACHED_SKIPPED) for entry in entries)):
                raise ValueError("Archive verification did not restore every required guide input")
            receipts.append({"game": game, "restored": sum(entry["status"] == "restored" for entry in entries),
                             "policy_skipped": sum(entry["status"] == cache.UNCACHED_SKIPPED for entry in entries)})
    manifest_path = cache_root / "manifest.json"
    document = json.loads(manifest_path.read_text())
    cache.prune(cache_root, document, 2684354560)
    key = cache_key(manifest_path)
    contract.emit_outputs({"archive_cache_key": key})
    contract.write_json(evidence / "cache-receipt.json", {
        "archive_cache_key": key, "games": games, "status": "verified", "verification": receipts})


def object_cache_prune_scope(root, archives, protected, runner_temp, actions, runner_environment):
    """Pure scope gate: this opt-in cleanup belongs only to an isolated hosted lane."""
    if actions != "true" or runner_environment != "github-hosted":
        raise ValueError("Object-cache pruning requires an explicit GitHub-hosted lane")
    if (not runner_temp.is_absolute() or runner_temp == Path("/")
            or not root.is_absolute() or root == runner_temp or not root.is_relative_to(runner_temp)):
        raise ValueError("Object-cache pruning requires a strict RUNNER_TEMP child")
    for destination in (archives, *protected):
        if root == destination or root.is_relative_to(destination) or destination.is_relative_to(root):
            raise ValueError("Object cache overlaps a consumer or evidence directory")


def prune_restored_object_cache(args, archives, report_path, map_path):
    """Release only ephemeral hash objects after independent archive copies are admitted."""
    raw_temp = os.environ.get("RUNNER_TEMP", "")
    if not raw_temp:
        raise ValueError("Object-cache pruning requires RUNNER_TEMP")
    declared_temp, declared_cache = Path(raw_temp), Path(args.cache)
    if (not declared_temp.is_absolute() or not declared_cache.is_absolute()
            or ".." in declared_temp.parts or ".." in declared_cache.parts):
        raise ValueError("Object-cache pruning paths must be absolute without parent traversal")
    # Check before resolving: symlinks must not authorize a different ephemeral root.
    for path in (declared_temp, declared_cache):
        for part in (path, *path.parents):
            if part.is_symlink():
                raise ValueError("Object-cache pruning refuses symlinked paths")
    runner_temp = declared_temp.resolve(strict=True)
    root, archives = declared_cache.resolve(strict=True), Path(archives).resolve(strict=True)
    protected = tuple(Path(path).resolve(strict=True) for path in
                      (args.game_dir, args.work_dir, args.output))
    object_cache_prune_scope(root, archives, protected, runner_temp,
                             os.environ.get("GITHUB_ACTIONS"), os.environ.get("RUNNER_ENVIRONMENT"))
    objects, manifest_path = root / "objects", root / "manifest.json"
    if (objects.is_symlink() or not objects.is_dir() or manifest_path.is_symlink()
            or not manifest_path.is_file()):
        raise ValueError("Object-cache pruning requires a regular cache and manifest")
    provenance_paths = (manifest_path, report_path, map_path)
    if any(path.is_symlink() or not path.is_file() for path in provenance_paths):
        raise ValueError("Object-cache pruning requires retained regular provenance files")
    provenance = {path: path.read_bytes() for path in provenance_paths}
    manifest, report, mapping = (json.loads(provenance[path]) for path in provenance_paths)
    if (manifest.get("schema") != 1 or not isinstance(manifest.get("entries"), dict)
            or report.get("ok") is not True or report.get("command") != "restore"
            or not isinstance(report.get("entries"), list) or not isinstance(mapping, dict)):
        raise ValueError("Object-cache pruning requires a successful complete restore report")
    restored = {}
    for entry in report["entries"]:
        if entry.get("status") != "restored":
            continue
        files = entry.get("files", [{"file": entry.get("file"), "sha256": entry.get("sha256")}])
        if not isinstance(files, list) or not files:
            raise ValueError("Restored archive file evidence is missing")
        for record in files:
            name, checksum = record.get("file"), record.get("sha256")
            if (not isinstance(name, str) or name != name.strip() or name in ("", ".", "..")
                    or len(name) > 255 or any(char in name for char in ("/", "\\"))
                    or any(ord(char) < 32 or ord(char) == 127 for char in name)
                    or not isinstance(checksum, str) or cache.SHA256.fullmatch(checksum) is None):
                raise ValueError("Restored archive file identity is unsafe")
            path = archives / name
            if path.is_symlink() or not path.is_file() or path.resolve().parent != archives:
                raise ValueError("An independent restored archive is missing")
            source = objects / checksum
            if source.exists() and os.path.samefile(source, path):
                raise ValueError("Restored archive must be independent of the disposable object cache")
            info = path.stat()
            restored[path] = (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)
    if not restored:
        raise ValueError("No independently restored archive copies were admitted")
    for values in mapping.values():
        values = [values] if isinstance(values, str) else values
        if not isinstance(values, list) or any(archives / str(name) not in restored for name in values):
            raise ValueError("Archive-map consumer input is not in the successful restore report")
    candidates = []
    for path in objects.iterdir():
        if path.is_symlink():
            raise ValueError("Object-cache pruning refuses symlinked object entries")
        if cache.SHA256.fullmatch(path.name):
            info = path.stat()
            if not stat.S_ISREG(info.st_mode):
                raise ValueError("Object-cache pruning refuses nonregular hash entries")
            candidates.append(info.st_size)
    cache.prune(root, manifest, 0)
    if any(cache.SHA256.fullmatch(path.name) for path in objects.iterdir()):
        raise ValueError("Ephemeral hash-object pruning did not finish")
    if any(path.read_bytes() != data for path, data in provenance.items()):
        raise ValueError("Object-cache pruning changed provenance")
    for path, expected in restored.items():
        info = path.stat()
        if (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns) != expected:
            raise ValueError("Object-cache pruning changed an admitted archive")
    contract.write_json(Path(args.output) / "object-cache-prune.json", {
        "schema_version": 1, "status": "ephemeral-hash-objects-pruned",
        "objects_pruned": len(candidates), "object_bytes_released": sum(candidates),
        "retained_archive_files": len(restored), "retained_archive_bytes": sum(info[2] for info in restored.values()),
        "manifest_sha256": hashlib.sha256(provenance[manifest_path]).hexdigest(),
        "restore_report_sha256": hashlib.sha256(provenance[report_path]).hexdigest(),
        "archive_map_sha256": hashlib.sha256(provenance[map_path]).hexdigest()})
    print("Released %d bytes from %d ephemeral hash objects; retained %d independent archive files"
          % (sum(candidates), len(candidates), len(restored)), flush=True)


def install(args):
    temp = Path(args.output)
    temp.mkdir(parents=True, exist_ok=True)
    work = Path(args.work_dir)
    work.mkdir(parents=True, exist_ok=True)
    fullrun_archive_manifest.recover(args.cache, args.guides, [args.game])
    options = uncached_options([args.game])
    checked([sys.executable, str(HERE / "cache.py"), "restore", *guides(args.guides, [args.game]),
             "--cache-dir", args.cache, "--destination", str(work / "archives"),
             "--archive-map", str(temp / "archive-map.json"), "--report", str(temp / "restore.json"),
             *options, *remote_options()], temp / "restore-private.log")
    if getattr(args, "prune_restored_object_cache", False):
        prune_restored_object_cache(args, work / "archives", temp / "restore.json", temp / "archive-map.json")
    try:
        checked([sys.executable, str(HERE / "ports.py"), "--implementation", "rust", "--mode", args.mode, "--complete-guide", "--max-quarantine", "0",
                 "--source", args.engine, "--guide", str(Path(args.guides) / "content" / args.game / "full.md"),
                 "--game-dir", args.game_dir, "--archives", str(work / "archives"),
                 "--archive-map", str(temp / "archive-map.json"), "--restore-report", str(temp / "restore.json"),
                 "--work-dir", str(work / "work"), "--report", str(temp / "report.json")], temp / "install-private.log",
                env=installer_environment())
    finally:
        retain_port_diagnostics(work / "work", temp / "native-diagnostics")
    digest = hashlib.sha256((Path(args.guides) / "content" / args.game / "full.md").read_bytes()).hexdigest()
    result = contract.require_complete(json.loads((temp / "report.json").read_text()), args.mode, digest)
    contract.write_json(temp / "completion.json", result)


def retain_port_diagnostics(work, destination):
    """Retain bounded native text evidence, never build, archive or game trees.

    The workflow seals the entire private evidence directory before upload.
    Include failed/timed-out subprocess logs and exact converted recipes, which
    are otherwise outside that directory and lost when the runner exits.
    """
    if not work.exists():
        return
    if work.is_symlink() or not work.is_dir():
        raise ValueError("unsafe native diagnostics directory")
    allowed = {"resolved-archives.json", "gui-report.json"}
    remaining = 64 * 1024 ** 2
    destination.mkdir(mode=0o700)
    for directory in sorted(work.iterdir()):
        if not directory.name.startswith("ports-"):
            continue
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError("unsafe native diagnostics entry")
        target = destination / directory.name
        target.mkdir(mode=0o700)
        for path in sorted(directory.iterdir()):
            if not (path.suffix == ".log" or
                    path.name.startswith("converted") and path.suffix == ".toml" or
                    path.name in allowed):
                continue
            limit = min(16 * 1024 ** 2, remaining)
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(descriptor, "rb") as incoming:
                info = os.fstat(incoming.fileno())
                if not stat.S_ISREG(info.st_mode):
                    raise ValueError("native diagnostics must be regular files")
                if info.st_size > limit:
                    raise ValueError("native diagnostics exceeds evidence budget")
                data = incoming.read(limit + 1)
                if len(data) > limit:
                    raise ValueError("native diagnostics exceeds evidence budget")
            with (target / path.name).open("xb") as output:
                os.chmod(target / path.name, 0o600)
                output.write(data)
            remaining -= len(data)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("refresh", "seed", "install", "identity"):
        child = sub.add_parser(name)
        child.add_argument("--guides", required=True)
        child.add_argument("--output", required=True)
        if name in ("refresh", "seed"):
            child.add_argument("--games", default='["k1","k2"]')
        if name == "refresh":
            child.add_argument("--cache", required=True)
        if name == "seed":
            child.add_argument("--dry-run", action="store_true")
        if name in ("identity", "install"):
            child.add_argument("--engine", required=True)
        if name == "identity":
            child.add_argument("--engine-sha", required=True)
            child.add_argument("--guide-sha", required=True)
        if name == "install":
            child.add_argument("--prune-restored-object-cache", action="store_true",
                               help="hosted CI only: release hash objects after verified independent archive restore")
            child.add_argument("--cache", required=True)
            child.add_argument("--work-dir", required=True)
            child.add_argument("--game-dir", required=True)
            child.add_argument("--game", choices=("k1", "k2"), required=True)
            child.add_argument("--mode", choices=("cli", "gui"), required=True)
    args = parser.parse_args(argv)
    old = os.umask(0o077)
    try:
        if args.command == "identity":
            identity(args.engine, args.guides, args.engine_sha, args.guide_sha, args.output)
        elif args.command == "install":
            install(args)
        else:
            requested_games = json.loads(args.games)
            if not isinstance(requested_games, list) or not requested_games:
                raise ValueError("games must be a nonempty list")
            if args.command == "refresh":
                refresh(args, requested_games)
            else:
                command = [sys.executable, str(HERE / "seed_archives.py"), *guides(args.guides, requested_games)]
                for seed_identity in selected_seed_ids(args.guides, requested_games):
                    command += ["--only", seed_identity]
                if args.dry_run:
                    command.append("--dry-run")
                checked(command, Path(args.output) / "seed-private.log")
        return 0
    except (OSError, ValueError, subprocess.SubprocessError, artstore_client.ArtifactError,
            tarfile.TarError, zipfile.BadZipFile):
        print("::error::full-run stage failed; inspect retained stage evidence.", file=sys.stderr)
        return 1
    finally:
        os.umask(old)


if __name__ == "__main__":
    sys.exit(main())
