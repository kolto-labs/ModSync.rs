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
import live_output
import gzip
import evidence_receipt
import k1_widescreen_inputs
import supplemental_public_inputs
import fullrun_contract as contract

HERE = Path(__file__).resolve().parent


def checked(command, log, env=None, pass_fds=()):
    log = Path(log)
    log.parent.mkdir(parents=True, exist_ok=True)
    options = {"pass_fds": pass_fds} if pass_fds else {}
    result = live_output.run(command, log, env=env, **options)
    if result.returncode:
        raise ValueError("stage failed; inspect its retained encrypted evidence")
    return result


def installer_environment():
    return {name: value for name, value in cache.scrubbed_env().items()
            if not gui_diag.SECRET_ENV_NAME.search(name)
            and not name.startswith(("SSH_", "GIT_CONFIG_"))}


def production_build_capability(engine, mode, output):
    """Acquire before credential scrubbing; only an anonymous readonly FD crosses it.

    The receipt is not standalone authority. This trusted parent verifies the
    artifact through the acquiring helper, then ports rechecks its source/bytes.
    Legacy, external-source and non-CI callers keep their original local build.
    """
    if (os.name != "posix" or not Path("/proc/self/fd").is_dir()
            or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("GITHUB_REPOSITORY") != "kolto-labs/ModSync.rs"
            or not os.environ.get("GITHUB_TOKEN")
            or not os.environ.get("CACHE_ENCRYPTION_KEY")):
        return None
    import production_build
    # Actions executes PRs at a merge ref, while the owned producer builds the
    # authenticated same-repository head. Reusable full runs admit valid owned
    # source branches; separate manual producers retain their legacy allowlist.
    source_identity = evidence_receipt.source_identity(os.environ)
    if not production_build.source_admitted({"repository": source_identity["source_repository"],
                                            "ref": source_identity["source_ref"],
                                            "head_sha": source_identity["source_sha"]}):
        return None
    engine = Path(engine).resolve(strict=True)
    if not (engine / ".git").exists():
        return None
    git = subprocess.run(["git", "rev-parse", "HEAD"], cwd=engine,
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                         text=True, check=False, env=installer_environment())
    if git.returncode or git.stdout.strip() != source_identity["source_sha"]:
        return None
    output = Path(output)
    receipt_path = output / "production-build-acquisition.json"
    command = [sys.executable, str(HERE / "production_build.py"), "fetch", "--mode", mode,
               "--source-root", str(engine), "--destination", str(engine / "target/release/modsync"),
               "--wait-seconds", "3300", "--receipt", str(receipt_path)]
    allowed = {"PATH", "HOME", "LANG", "RUNNER_TEMP", "TMPDIR", "TMP", "TEMP", "SYSTEMROOT", "WINDIR",
               "GITHUB_ACTIONS", "GITHUB_REPOSITORY", "GITHUB_REPOSITORY_ID", "GITHUB_REPOSITORY_OWNER_ID",
               "GITHUB_REF", "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_WORKFLOW_REF",
               "GITHUB_WORKFLOW_SHA", "GITHUB_JOB", "GITHUB_EVENT_NAME", "GITHUB_API_URL",
               "GITHUB_TOKEN", "CACHE_ENCRYPTION_KEY"}
    environment = {name: value for name, value in os.environ.items() if name in allowed}
    # Only fetch's source selector changes. Consumer execution/workflow IDs and
    # SHAs remain the original Actions values; producer provenance is checked
    # independently against its own API run, workflow and sealed source recipe.
    environment.update(GITHUB_SHA=source_identity["source_sha"], GITHUB_REF=source_identity["source_ref"])
    result = live_output.run(command, output / "production-build-acquisition-private.log",
                             env=environment)
    if result.returncode == 3:
        return None
    if result.returncode:
        raise ValueError("Verified production build admission failed; inspect encrypted acquisition evidence")
    with receipt_path.open("rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 12 * 1024 * 1024:
            raise ValueError("Invalid verified build acquisition receipt")
        body = source.read(12 * 1024 * 1024 + 1)
    receipt = json.loads(body)
    if (receipt.get("status") != "verified" or receipt.get("head_sha") != git.stdout.strip()
            or receipt.get("repository") != os.environ["GITHUB_REPOSITORY"] or receipt.get("mode") != mode
            or receipt.get("ref") != source_identity["source_ref"]):
        raise ValueError("Verified build acquiring parent identity differs")
    payload = json.dumps({"schema_version": 1, "source_path": str(engine), "receipt": receipt}).encode()
    if len(payload) > 12 * 1024 * 1024:
        raise ValueError("Verified build capability exceeds bound")
    with tempfile.TemporaryFile(dir=output) as staging:
        staging.write(payload)
        staging.flush()
        os.fchmod(staging.fileno(), 0o400)
        descriptor = os.open("/proc/self/fd/" + str(staging.fileno()), os.O_RDONLY | os.O_CLOEXEC)
    return descriptor


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
    execution = evidence_receipt.identity()
    contract.write_json(output, {"schema_version": 1, "engine_sha": actual_engine,
                                "guide_sha": actual_guide, "execution_repository": os.environ.get("GITHUB_REPOSITORY"),
                                "execution_sha": execution["sha"],
                                "execution_source_sha": execution.get("source_sha", execution["sha"]),
                                "execution_source_repository": execution.get("source_repository", execution["repository"]),
                                "execution_event": os.environ.get("GITHUB_EVENT_NAME", ""),
                                "execution_workflow_sha": execution["workflow_sha"],
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


def native_k1_scope(guide_root):
    path = Path(guide_root) / "content/k1/full.md"
    return (not path.is_symlink() and path.is_file()
            and cache.digest(path) == k1_widescreen_inputs.GUIDE_SHA256)


def install_scope(args, game):
    """Acquire a profile subset while the unchanged whole guide stays authoritative."""
    import install_profiles
    scope = install_profiles.plan(getattr(args, "install_profile", "minimal"), game)
    path = Path(args.guides) / "content" / game / "full.md"
    if path.is_symlink() or cache.digest(path) != scope["source_guide_sha256"]:
        raise ValueError("Install profile requires the exact complete original source guide")
    items = cache.discover([str(path)])
    discovered = {item["id"] for item in items}
    selected = set(scope["acquisition_source_ids"])
    if not selected or selected - discovered:
        raise ValueError("Install profile archive closure is empty or outside the original guide")
    excluded_headings = set(scope["excluded_headings"])
    inactive = set(scope["platform_inactive_source_ids"]) - selected
    retained = set(selected)
    for item in items:
        if item["id"] in retained or item["id"] in inactive:
            continue
        steps = cache.referencing_steps(item)
        if not steps or any(step["heading"] not in excluded_headings for step in steps):
            # Preamble/master/setup inputs have no root-component GUID. Preserve
            # them unless every reference belongs to a documented omission.
            retained.add(item["id"])
    scope = {**scope, "archive_source_ids": sorted(retained),
             "archive_omitted_source_ids": sorted(discovered - retained),
             "guide_only_archive_source_ids": sorted(retained - selected),
             "non_acquisition_policy_source_ids": sorted(item["id"] for item in items
                 if item["id"] in retained and cache.uncached_reason(item))}
    return scope


def source_filter(scope):
    return [argument for identity in scope["archive_source_ids"] for argument in ("--source-id", identity)]


DOWNLOAD_MODES = ("cache-first", "sample", "refresh")


def cache_mode(args):
    """The mode every archive is restored with. A sampled run reuses the verified cache."""
    mode = getattr(args, "download_mode", "cache-first")
    if mode not in DOWNLOAD_MODES:
        raise ValueError("unsupported download mode")
    return "cache-first" if mode == "sample" else mode


def sample_seed(environ=None):
    """One draw per workflow run attempt, repeatable from the run's public identity."""
    environ = os.environ if environ is None else environ
    return "%s:%s" % (environ.get("GITHUB_RUN_ID") or "local", environ.get("GITHUB_RUN_ATTEMPT") or "1")


def sample_source(scope, game, seed):
    """The one archive source this run downloads live; None when nothing can be downloaded."""
    candidates = sorted(set(scope["archive_source_ids"]) - set(scope["non_acquisition_policy_source_ids"]))
    if not candidates:
        return None
    draw = int(hashlib.sha256(("%s:%s" % (seed, game)).encode()).hexdigest(), 16)
    return candidates[draw % len(candidates)]


def sample_report(output, game):
    return Path(output) / ("live-sample-%s.json" % game)


def sampled_attempt(args, game, scope):
    """Return the drawn source once its live provider attempt is on record, else raise."""
    chosen = sample_source(scope, game, sample_seed())
    if chosen is None:
        return None
    path = sample_report(args.output, game)
    if path.is_symlink() or path.stat().st_size > 32 * 1024 * 1024:
        raise ValueError("unsafe live sample report")
    report = json.loads(path.read_text())
    entries = report["entries"]
    if (report.get("command") != "restore" or report.get("ok") is not True or len(entries) != 1
            or entries[0].get("id") != chosen or entries[0].get("status") != "restored"):
        raise ValueError("live sample did not restore the drawn source")
    acquisition = entries[0].get("acquisition", {})
    if acquisition.get("download_mode") != "refresh" or acquisition.get("provider_attempted") is not True:
        raise ValueError("live sample lacks a real provider attempt")
    return chosen


def downloader_validation(args, games):
    """Describe real provider attempts separately from verified cache reuse."""
    mode = getattr(args, "download_mode", "cache-first")
    restore_mode = cache_mode(args)
    scopes = {game: install_scope(args, game) for game in games}
    selected = {identity for scope in scopes.values() for identity in scope["archive_source_ids"]}
    attempted, downloaded, fallback, policies, valid, seen = set(), set(), set(), set(), set(), set()
    sampled, expected_samples = set(), 0
    failures = []
    for game, scope in scopes.items():
        path = Path(args.output) / ("verify-%s.json" % game)
        try:
            if path.is_symlink() or path.stat().st_size > 32 * 1024 * 1024:
                raise ValueError("unsafe downloader report")
            report = json.loads(path.read_text())
            entries = report["entries"]
            expected = set(scope["archive_source_ids"])
            if (report.get("command") != "restore" or report.get("ok") is not True
                    or len(entries) != len(expected) or {entry["id"] for entry in entries} != expected):
                raise ValueError("downloader source coverage differs")
            for entry in entries:
                identity = entry["id"]
                seen.add(identity)
                if entry.get("status") == cache.UNCACHED_SKIPPED:
                    if identity not in scope["non_acquisition_policy_source_ids"]:
                        raise ValueError("Required archive cannot be waived as a policy helper")
                    policies.add(identity)
                    continue
                acquisition = entry.get("acquisition", {})
                if entry.get("status") != "restored" or acquisition.get("download_mode") != restore_mode:
                    raise ValueError("downloader input is not verified for the requested mode")
                valid.add(identity)
                if acquisition.get("provider_attempted") is True:
                    attempted.add(identity)
                elif (acquisition.get("provider_outcome") in ("downloaded", "cached-fallback")
                        or acquisition.get("provider_downloaded") is True
                        or acquisition.get("valid_cache_fallback") is True):
                    raise ValueError("Downloader outcome lacks a real provider attempt")
                if (acquisition.get("provider_outcome") == "downloaded"
                        or acquisition.get("provider_downloaded") is True):
                    downloaded.add(identity)
                if (acquisition.get("provider_outcome") == "cached-fallback"
                        or acquisition.get("valid_cache_fallback") is True):
                    fallback.add(identity)
            if mode == "sample" and sample_source(scope, game, sample_seed()) is not None:
                expected_samples += 1
                sampled.add(sampled_attempt(args, game, scope))
        except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError):
            failures.append(game)
    missing = selected - valid - policies
    unattempted = selected - attempted - policies if mode == "refresh" else set()
    passed = (not failures and not missing and not unattempted and seen == selected
              and (mode != "sample" or len(sampled) >= min(1, expected_samples)))
    return {"schema_version": 1, "status": "passed" if passed else "failed",
        "install_profile": getattr(args, "install_profile", "minimal"), "download_mode": mode,
        "live_sample_source_ids": sorted(sampled),
        "selected_source_ids": sorted(selected), "provider_attempted_source_ids": sorted(attempted),
        "valid_cache_fallback_source_ids": sorted(fallback), "downloaded_source_ids": sorted(downloaded),
        "non_acquisition_policy_source_ids": sorted(policies),
        "non_archive_provenance_source_ids": sorted({identity for scope in scopes.values()
            for identity in scope["non_archive_provenance_source_ids"]}),
        "missing_verified_source_ids": sorted(missing), "unattempted_required_source_ids": sorted(unattempted),
        "incomplete_games": failures,
        "claim": "provider attempts and verified cache fallback; cache/artifact reads are not provider downloads"}


def refresh(args, games):
    try:
        refresh_archives(args, games)
    finally:
        validation = downloader_validation(args, games)
        contract.write_json(Path(args.output) / "downloader-validation.json", validation)
        contract.emit_outputs({"download_validation": validation["status"]})
    if validation["status"] != "passed":
        raise ValueError("Downloader validation did not cover the requested profile and mode")


def restore_with_public_supplement(command, log, args, game, destination, report_path, map_path,
                                   source_ids=None):
    """Repair only the pinned public K2 source group after the normal restore."""
    def identity(path):
        try:
            info = Path(path).stat()
            return info.st_ino, info.st_size, info.st_mtime_ns
        except FileNotFoundError:
            return None

    before = (identity(report_path), identity(map_path)) if game == "k2" else None
    failure = None
    try:
        checked(command, log)
    except ValueError as error:
        failure = error
    if (game == "k2" and (source_ids is None or supplemental_public_inputs.SOURCE_ID in source_ids)
            and supplemental_public_inputs.applies(args.guides)):
        # A failed restore may be repaired only from this invocation's newly
        # written records. The helper also rejects every unrelated failure.
        if failure is not None and (identity(report_path) in (None, before[0])
                                    or identity(map_path) in (None, before[1])):
            raise failure
        return supplemental_public_inputs.ensure(args.guides, destination, args.cache,
            report_path, map_path, Path(args.output) / "k2-supplemental-public-inputs.json",
            download_mode=cache_mode(args))
    if failure is not None:
        raise failure
    return None


def refresh_archives(args, games):
    guides(args.guides, games)
    scopes = {game: install_scope(args, game) for game in games}
    fullrun_archive_manifest.recover(args.cache, args.guides, games)
    cache_root = Path(args.cache)
    cache_root.parent.mkdir(parents=True, exist_ok=True)
    evidence = Path(args.output)
    receipts = []
    k1_auxiliary_verified = False
    # Restore verifies every required archive; the cache cap is a storage policy,
    # never permission to count an unfetched OVER_LIMIT object as verified.
    for game in games:
        scope = scopes[game]
        contract.write_json(evidence / ("install-profile-%s.json" % game), scope)
        report_path = evidence / ("verify-%s.json" % game)
        chosen = (sample_source(scope, game, sample_seed())
                  if getattr(args, "download_mode", "cache-first") == "sample" else None)
        if chosen is not None:
            # One live provider download per game and run keeps the downloaders exercised
            # without asking every mod site for every archive. Its verified bytes land in
            # the cache, so the complete restore below reads them like any other archive.
            with tempfile.TemporaryDirectory(prefix="archive-live-sample-", dir=cache_root.parent) as live:
                checked([sys.executable, str(HERE / "cache.py"), "restore", *guides(args.guides, [game]),
                         "--cache-dir", args.cache, "--destination", live,
                         "--report", str(sample_report(evidence, game)), "--download-mode", "refresh",
                         "--source-id", chosen, *uncached_options([game]), *remote_options()],
                        evidence / ("live-sample-%s-private.log" % game))
        with tempfile.TemporaryDirectory(prefix="archive-verification-", dir=cache_root.parent) as temporary:
            map_path = evidence / ("verify-%s-archive-map.json" % game)
            restore_with_public_supplement([sys.executable, str(HERE / "cache.py"), "restore", *guides(args.guides, [game]),
                     "--cache-dir", args.cache, "--destination", temporary, "--report", str(report_path),
                     "--download-mode", cache_mode(args),
                     *source_filter(scope),
                     *(["--archive-map", str(map_path)] if game == "k2" else []),
                     *uncached_options([game]), *remote_options()], evidence / ("verify-%s-private.log" % game),
                     args, game, Path(temporary), report_path, map_path, scope["archive_source_ids"])
            report = json.loads(report_path.read_text())
            entries = report.get("entries")
            expected = set(scope["archive_source_ids"])
            if (report.get("ok") is not True or not isinstance(entries, list) or not entries
                    or len(entries) != len(expected) or {entry.get("id") for entry in entries} != expected
                    or any(entry.get("status") not in ("restored", cache.UNCACHED_SKIPPED) for entry in entries)):
                raise ValueError("Archive verification did not restore every required guide input")
            receipts.append({"game": game, "restored": sum(entry["status"] == "restored" for entry in entries),
                             "policy_skipped": sum(entry["status"] == cache.UNCACHED_SKIPPED for entry in entries),
                             "install_profile": scope["profile"], "selected_owner_count": scope["selected_count"],
                             "original_definition_count": scope["source_definition_count"],
                             "selected_archive_sources": len(expected)})
            if scope["requires_k1_widescreen_inputs"] and native_k1_scope(args.guides):
                auxiliary = k1_widescreen_inputs.ensure(
                    Path(temporary), cache_root, evidence / "k1-native-inputs.json",
                    download_mode=cache_mode(args))
                if auxiliary.get("ok") is not True:
                    raise ValueError("Pinned K1 native inputs unavailable; inspect encrypted acquisition evidence")
                receipts[-1]["native_widescreen_inputs_verified"] = 6
                k1_auxiliary_verified = True
    manifest_path = cache_root / "manifest.json"
    document = json.loads(manifest_path.read_text())
    cache.prune(cache_root, document, 2684354560)
    key = cache_key(manifest_path)
    if k1_auxiliary_verified:
        # An existing immutable cache key cannot acquire new auxiliary objects.
        identity = key + k1_widescreen_inputs.PROFILE_SHA256 + k1_widescreen_inputs.GALAXY_PROFILE_SHA256
        key = "mod-archives-" + hashlib.sha256(identity.encode()).hexdigest()[:16]
    # A partial-profile object set must not occupy an exhaustive immutable cache
    # key merely because both retained the complete original manifest metadata.
    profile_identity = json.dumps({game: {"policy_sha256": scope["policy_sha256"],
        "profile": scope["profile"], "sources": scope["archive_source_ids"]}
        for game, scope in scopes.items()}, sort_keys=True, separators=(",", ":"))
    key = "mod-archives-" + hashlib.sha256((key + profile_identity).encode()).hexdigest()[:16]
    contract.emit_outputs({"archive_cache_key": key})
    contract.write_json(evidence / "cache-receipt.json", {
        "archive_cache_key": key, "games": games, "status": "verified", "verification": receipts,
        "install_profile": getattr(args, "install_profile", "minimal"),
        "download_mode": getattr(args, "download_mode", "cache-first")})


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
    # CLI callers always supply an explicit default profile. Older Python
    # callers without that field retain their unfiltered acquisition API.
    scope = install_scope(args, args.game) if hasattr(args, "install_profile") else None
    if scope is not None:
        contract.write_json(temp / "install-profile-source-scope.json", scope)
    fullrun_archive_manifest.recover(args.cache, args.guides, [args.game])
    options = uncached_options([args.game])
    restore_with_public_supplement([sys.executable, str(HERE / "cache.py"), "restore", *guides(args.guides, [args.game]),
             "--cache-dir", args.cache, "--destination", str(work / "archives"),
             "--download-mode", cache_mode(args),
             *(source_filter(scope) if scope is not None else []),
             "--archive-map", str(temp / "archive-map.json"), "--report", str(temp / "restore.json"),
             *options, *remote_options()], temp / "restore-private.log", args, args.game,
             work / "archives", temp / "restore.json", temp / "archive-map.json", scope["archive_source_ids"] if scope is not None else None)
    if getattr(args, "prune_restored_object_cache", False):
        prune_restored_object_cache(args, work / "archives", temp / "restore.json", temp / "archive-map.json")
    if (scope is None or scope["requires_k1_widescreen_inputs"]) and native_k1_scope(args.guides):
        auxiliary = k1_widescreen_inputs.ensure(
            work / "archives", Path(args.cache), temp / "k1-native-inputs.json",
            download_mode=cache_mode(args))
        if auxiliary.get("ok") is not True:
            raise ValueError("Pinned K1 native inputs unavailable; inspect encrypted acquisition evidence")
    if args.game == "k1" and scope is not None:
        import source_text_diagnostics
        source_text_diagnostics.retain(work / "archives", temp / "source-text-diagnostics")
    build_descriptor = None
    try:
        build_descriptor = production_build_capability(args.engine, args.mode, temp)
        build_arguments = [] if build_descriptor is None else ["--verified-build-fd", str(build_descriptor)]
        backend_arguments = []
        if getattr(args, "patcher_engine", None) is not None:
            backend_arguments += ["--patcher-engine", args.patcher_engine,
                                  "--comparison-output-dir", str(temp / "patcher-comparison")]
        if getattr(args, "patcher", None) is not None:
            backend_arguments += ["--patcher", str(args.patcher)]
        for name in ("execute_request_dir", "execute_xvfb", "execute_wine", "execute_wineserver", "execute_wine_prefix"):
            if getattr(args, name, None) is not None:
                backend_arguments += ["--" + name.replace("_", "-"), str(getattr(args, name))]
        checked([sys.executable, str(HERE / "ports.py"), "--implementation", "rust", "--mode", args.mode, "--complete-guide", "--max-quarantine", "0",
                 *(["--install-profile", scope["profile"]] if scope is not None else []),
                 "--source", args.engine, "--guide", str(Path(args.guides) / "content" / args.game / "full.md"),
                 "--game-dir", args.game_dir, "--archives", str(work / "archives"),
                 "--archive-map", str(temp / "archive-map.json"), "--restore-report", str(temp / "restore.json"),
                 "--work-dir", str(work / "work"), "--report", str(temp / "report.json"), *build_arguments, *backend_arguments], temp / "install-private.log",
                env=installer_environment(), pass_fds=() if build_descriptor is None else (build_descriptor,))
    finally:
        if build_descriptor is not None:
            os.close(build_descriptor)
        retain_port_diagnostics(work / "work", temp / "native-diagnostics")
    digest = hashlib.sha256((Path(args.guides) / "content" / args.game / "full.md").read_bytes()).hexdigest()
    result = contract.require_complete(json.loads((temp / "report.json").read_text()), args.mode, digest,
                                       install_profile=scope["profile"] if scope is not None else None)
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
    # Compress complete raw streams in independently verifiable 16 MiB chunks.
    # Explicit budgets fail the lane; never silently truncate evidence.
    raw_remaining = 16 * 1024 ** 3
    compressed_remaining = 4 * 1024 ** 3
    destination.mkdir(mode=0o700)
    manifest = []
    game_envelopes = []
    for directory in sorted(work.iterdir()):
        if not directory.name.startswith("ports-"):
            continue
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError("unsafe native diagnostics entry")
        paths = []
        for base, folders, files in os.walk(directory, followlinks=False):
            # Never descend into game baselines, builds, settings or archives.
            relative_parts = Path(base).relative_to(directory).parts
            installer_evidence = "windows-execute" in relative_parts
            if installer_evidence:
                # Backend backups can contain licensed game resources. Keep
                # those bodies behind the game key inside diagnostics encryption.
                for name in sorted(name for name in folders if name.endswith("-artifacts")):
                    source = Path(base) / name
                    if source.is_symlink():
                        raise ValueError("unsafe backend artifact directory")
                    total = 0
                    for member in source.rglob("*"):
                        if member.is_symlink() or not (member.is_file() or member.is_dir()):
                            raise ValueError("unsafe backend artifact member")
                        if member.is_file():
                            total += member.stat().st_size
                    raw_remaining -= total
                    if raw_remaining < 0:
                        raise ValueError("backend evidence exceeds explicit 16 GiB raw budget")
                    relative = source.relative_to(work)
                    target = destination / relative.parent / (name + ".seal")
                    target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                    import seal
                    seal.seal(source, target)
                    compressed_remaining -= target.stat().st_size
                    if compressed_remaining < 0:
                        raise ValueError("backend evidence exceeds explicit 4 GiB encrypted budget")
                    game_envelopes.append({"source": str(relative), "path": str(target.relative_to(destination)),
                                           "plaintext_bytes": total, "ciphertext_bytes": target.stat().st_size,
                                           "ciphertext_sha256": cache.digest(target),
                                           "access": "CACHE_ENCRYPTION_KEY"})
                folders[:] = sorted(name for name in folders if name.startswith("packaged-ui-"))
            else:
                folders[:] = sorted(name for name in folders
                                    if name.startswith(("gui-work", "gui-rust-")) or name == "windows-execute")
            if any((Path(base) / name).is_symlink() for name in folders):
                raise ValueError("unsafe native diagnostics directory link")
            paths.extend(Path(base) / name for name in sorted(files))
        for path in paths:
            if path.is_symlink():
                raise ValueError("unsafe native diagnostics entry")
            if path.is_dir():
                continue
            installer_evidence = "windows-execute" in path.relative_to(directory).parts
            if not (installer_evidence or path.suffix == ".log" or path.name.endswith(".command.json") or
                    path.name.startswith("converted") and path.suffix == ".toml" or
                    path.name in allowed or path.name == "command.json" or
                    path.name.startswith("review-query-") and path.suffix == ".json" or
                    path.name == "phase-plan.json"):
                continue
            relative = path.relative_to(work)
            target = destination / relative.parent
            target.mkdir(mode=0o700, parents=True, exist_ok=True)
            descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            checksum = hashlib.sha256()
            size = 0
            chunks = []
            with os.fdopen(descriptor, "rb") as incoming:
                if not stat.S_ISREG(os.fstat(incoming.fileno()).st_mode):
                    raise ValueError("native diagnostics must be regular files")
                while data := incoming.read(16 * 1024 ** 2):
                    size += len(data)
                    raw_remaining -= len(data)
                    if raw_remaining < 0:
                        raise ValueError("complete native evidence exceeds explicit 16 GiB raw budget")
                    checksum.update(data)
                    name = path.name + (".%06d.gz" % len(chunks))
                    archive = target / name
                    with archive.open("xb") as output:
                        os.chmod(archive, 0o600)
                        with gzip.GzipFile(filename="", fileobj=output, mode="wb", compresslevel=1, mtime=0) as zipped:
                            zipped.write(data)
                    compressed_remaining -= archive.stat().st_size
                    if compressed_remaining < 0:
                        raise ValueError("complete native evidence exceeds explicit 4 GiB compressed budget")
                    chunks.append({"path": str(archive.relative_to(destination)),
                                   "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
            manifest.append({"path": str(relative), "bytes": size,
                             "sha256": checksum.hexdigest(), "chunks": chunks})
            # Keep small structured inputs usable by existing encrypted-evidence readers.
            if path.suffix != ".log" and size <= 16 * 1024 ** 2:
                import shutil
                shutil.copyfile(path, target / path.name)
                os.chmod(target / path.name, 0o600)
    contract.write_json(destination / "complete-log-manifest.json",
                        {"schema_version": 1, "files": manifest})
    contract.write_json(destination / "game-output-envelopes.json",
                        {"schema_version": 1, "envelopes": game_envelopes})


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
        if name in ("refresh", "install"):
            child.add_argument("--install-profile", choices=("minimal", "full", "exhaustive"),
                               default="minimal", help="original guide coverage to acquire and install")
            child.add_argument("--download-mode", choices=DOWNLOAD_MODES, default="cache-first",
                               help="reuse verified cache; sample downloads one drawn source live; "
                                    "refresh checks every selected upstream input")
        if name == "seed":
            child.add_argument("--dry-run", action="store_true")
        if name in ("identity", "install"):
            child.add_argument("--engine", required=True)
        if name == "identity":
            child.add_argument("--engine-sha", required=True)
            child.add_argument("--guide-sha", required=True)
        if name == "install":
            child.add_argument("--patcher-engine", choices=("ody-patcher", "holo-patcher", "mod-packaged"))
            child.add_argument("--patcher", type=Path)
            for name in ("execute-request-dir", "execute-xvfb", "execute-wine", "execute-wineserver", "execute-wine-prefix"):
                child.add_argument("--" + name, type=Path)
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
