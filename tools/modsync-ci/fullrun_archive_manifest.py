#!/usr/bin/env python3
"""Recover an authenticated URL-to-object association from this repository's artifacts.

The GitHub artifact digest binds the transfer receipt; the legacy envelope MAC
binds the private manifest and source provenance. Neither replaces checking the
reviewed guide and current object pins, or verifying the actual archive bytes.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import time
import zipfile

import artstore
import artstore_client
import cache
import seal

ARTIFACT_NAME = "mod-archive-manifest-sealed"
SOURCE_REPOSITORY = "kolto-labs/odyssey-ci"
SOURCE_REPOSITORY_ID = 1427366340
MAX_MANIFEST_BYTES = 32 * 1024 * 1024
SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")


def matches(pattern, value):
    return isinstance(value, str) and pattern.fullmatch(value) is not None


def positive_id(value):
    return isinstance(value, (int, str)) and not isinstance(value, bool) and bool(
        re.fullmatch(r"[1-9][0-9]*", str(value)))


def read_json(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ValueError("Invalid manifest metadata file")
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("Invalid manifest metadata document")
    return value


def verify_provenance(inner, receipt, artifact, repository, guide_root, games):
    source = receipt.get("source_artifact") or {}
    if not isinstance(source, dict):
        raise ValueError("Invalid archive manifest source artifact")
    source_run = source.get("workflow_run") or {}
    target_run = artifact.get("workflow_run") or {}
    if not all(isinstance(value, dict) for value in (source, source_run, target_run)):
        raise ValueError("Invalid archive manifest artifact identities")
    source_attempt = receipt.get("source_attempt", source_run.get("run_attempt"))
    if (type(inner.get("schema_version")) is not int or inner["schema_version"] != 1
            or type(receipt.get("schema_version")) is not int or receipt["schema_version"] != 1
            or receipt.get("source_repository") != SOURCE_REPOSITORY
            or inner.get("source_repository") != SOURCE_REPOSITORY
            or source.get("name") != ARTIFACT_NAME or not positive_id(source.get("id"))
            or not matches(DIGEST, source.get("digest"))
            or source_run.get("repository_id") != SOURCE_REPOSITORY_ID
            or source_run.get("head_repository_id") != SOURCE_REPOSITORY_ID
            or not matches(SHA, source_run.get("head_sha"))
            or inner.get("source_sha") != source_run.get("head_sha")
            or not positive_id(source_run.get("id"))
            or str(inner.get("source_run_id")) != str(source_run.get("id"))
            or not positive_id(source_attempt)
            or str(inner.get("source_run_attempt")) != str(source_attempt)
            or receipt.get("target_repository") != repository
            or not matches(SHA, target_run.get("head_sha"))
            or receipt.get("target_sha") != target_run.get("head_sha")
            or not positive_id(target_run.get("id"))
            or str(receipt.get("target_run_id")) != str(target_run.get("id"))
            or not positive_id(target_run.get("repository_id"))
            or target_run.get("head_repository_id") != target_run.get("repository_id")
            or (os.environ.get("GITHUB_REPOSITORY_ID") and str(target_run.get("repository_id"))
                != os.environ["GITHUB_REPOSITORY_ID"])):
        raise ValueError("Archive manifest provenance does not match artifact identities")
    config = read_json(Path(__file__).resolve().parents[2] / ".github/fullrun.json")
    guide_sha = subprocess.check_output(
        ["git", "-C", str(guide_root), "rev-parse", "HEAD"], text=True).strip()
    hashes = inner.get("guide_sha256")
    if (inner.get("guide_repository") != config["guide_repository"]
            or not SHA.fullmatch(guide_sha) or inner.get("guide_sha") != guide_sha
            or not isinstance(hashes, dict)):
        raise ValueError("Archive manifest guide identity does not match checkout")
    for game in games:
        path = Path(guide_root) / "content" / game / "full.md"
        if path.is_symlink() or not path.is_file() or hashes.get(game) != hashlib.sha256(path.read_bytes()).hexdigest():
            raise ValueError("Archive manifest guide bytes do not match checkout")


def validate_manifest(document, guide_root, games):
    if type(document.get("schema")) is not int or document["schema"] != 1 or not isinstance(document.get("entries"), dict):
        raise ValueError("Invalid archive manifest schema")
    items = {item["id"]: item for item in cache.discover(
        [str(Path(guide_root) / "content" / game / "full.md") for game in games])}
    pins = cache.load_pins()
    original_entries = cache.original_guide_inputs.selected_entries(
        list(items.values()),
        [str(Path(guide_root) / "content" / game / "full.md") for game in games], document["entries"])
    selected = {}
    for identity, entry in document["entries"].items():
        if not isinstance(identity, str) or not cache.SHA256.fullmatch(identity) or not isinstance(entry, dict):
            raise ValueError("Invalid archive manifest association")
        objects = cache.entry_objects(entry)
        if not objects:
            raise ValueError("Archive manifest association has no objects")
        for stored in objects:
            if (not isinstance(stored.get("sha256"), str) or not cache.SHA256.fullmatch(stored["sha256"])
                    or not isinstance(stored.get("size"), int) or isinstance(stored["size"], bool)
                    or not 0 < stored["size"] <= cache.MAX_LINK_BYTES
                    or stored.get("extension") not in cache.PIN_EXTENSIONS
                    or not isinstance(stored.get("archive_name"), str)
                    or cache.safe_archive_name(stored["archive_name"], stored["extension"]) != stored["archive_name"]):
                raise ValueError("Invalid archive manifest object")
        if "objects" in entry and (not isinstance(entry.get("size"), int)
                                   or isinstance(entry["size"], bool)
                                   or entry["size"] != sum(stored["size"] for stored in objects)):
            raise ValueError("Archive manifest bundle size mismatch")
        if identity not in items:
            continue
        item = items[identity]
        original_descriptor = original_entries.get(identity)
        legacy_menu = (original_descriptor is not None
                       and cache.original_guide_inputs.authenticated_legacy_menu_association(
                           item, entry, original_descriptor))
        if (cache.key_for(item["url"]) != identity
                or "url" in entry and entry["url"] not in (item["url"], cache.clean_url(item["url"]))
                or not cache.objects_match_selection(item, entry) and not legacy_menu):
            raise ValueError("Archive association does not match current guide URL selection")
        if identity in pins:
            fields = ("sha256", "size", "extension", "archive_name")
            actual = sorted(tuple(stored[key] for key in fields) for stored in objects)
            expected = sorted(tuple(stored[key] for key in fields) for stored in pins[identity]["objects"])
            if actual != expected or pins[identity]["url"] != item["url"]:
                raise ValueError("Archive association conflicts with current reviewed pins")
        # The destination is a reusable Actions cache. Copy only reviewed object
        # inventory, never source endpoints, P2P trackers, headers or arbitrary
        # provider metadata. Reconstruct selection labels from the current guide.
        selectors = cache.multi_selectors(item) or []
        safe_objects = []
        for stored in objects:
            safe = {field: stored[field] for field in ("sha256", "size", "extension", "archive_name")}
            for selector in selectors:
                if stored.get("listed_name") == selector["file"] and (
                        selector["r"] is None or stored.get("row") == selector["r"]):
                    safe.update(listed_name=selector["file"], row=selector["r"])
            safe_objects.append(safe)
        metadata = {"url": cache.clean_url(item["url"]), "checked_at": 0, "metadata_unknown": True,
                    "status": "pinned" if identity in pins else "stale-unknown",
                    "evidence": "operator-seed" if identity in pins else "legacy-manifest-association"}
        selected[identity] = ({**metadata, "objects": safe_objects, "size": sum(each["size"] for each in safe_objects)}
                              if "objects" in entry else {**metadata, **safe_objects[0]})
        if legacy_menu:
            # recover() already authenticated the sealed source manifest and
            # provenance. Preserve that association while scheduling the exact
            # missing compiled inputs; no body has been restored by this step.
            selected[identity] = {**original_descriptor, "status": "required-input",
                                  "legacy_manifest_association": {
                                      "evidence": "authenticated-legacy-manifest-association",
                                      "association_id": identity,
                                      "entry_sha256": hashlib.sha256(json.dumps(
                                          entry, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
                                      "objects": safe_objects}}

    if not selected:
        raise ValueError("Archive manifest has no current guide associations")
    return {"schema": 1, "entries": selected}


def recover(cache_root, guide_root, games):
    """Recover only a missing manifest; absent optional artifacts leave normal acquisition available."""
    cache_root = Path(cache_root)
    destination = cache_root / "manifest.json"
    if destination.is_symlink():
        raise ValueError("Archive manifest must not be a symlink")
    if destination.exists():
        return False
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    # The optional legacy store may use a different narrowly scoped token.
    # Manifest provenance always belongs to this repository; never send that
    # legacy credential here or redirect the manifest lookup to its namespace.
    store = os.environ.get("ARTSTORE_REPOSITORY") or repository
    token = os.environ.get("GITHUB_TOKEN") or (
        os.environ.get("ARTSTORE_TOKEN", "") if store == repository else "")
    if not repository or not token:
        return False
    client = artstore_client.ArtifactsClient(repository, token)
    artifacts = [item for item in client.list_artifacts(name=ARTIFACT_NAME)
                 if item.get("name") == ARTIFACT_NAME and not artstore._is_dead(item, time.time())]
    if not artifacts:
        return False
    artifact = max(artifacts, key=lambda item: (item.get("created_at") or "", item.get("id") or 0))
    if (not matches(DIGEST, artifact.get("digest"))
            or type(artifact.get("size_in_bytes")) is not int
            or not 0 < artifact["size_in_bytes"] <= MAX_MANIFEST_BYTES):
        raise ValueError("Archive manifest artifact digest or size is invalid")
    temporary_root = os.environ.get("RUNNER_TEMP")
    if not temporary_root:
        raise ValueError("RUNNER_TEMP is required for private manifest recovery")
    with tempfile.TemporaryDirectory(prefix="archive-manifest-", dir=temporary_root) as temporary:
        temporary = Path(temporary)
        archive_path = temporary / "artifact.zip"
        client.download_zip(artifact["id"], archive_path, expect_digest=artifact["digest"])
        if archive_path.stat().st_size != artifact["size_in_bytes"]:
            raise ValueError("Archive manifest artifact size mismatch")
        with zipfile.ZipFile(archive_path) as archive:
            members = archive.infolist()
            if len(members) != 2 or {member.filename for member in members} != {"manifest.seal", "transfer-provenance.json"}:
                raise ValueError("Unexpected archive manifest artifact members")
            for member in members:
                mode = member.external_attr >> 16
                if (member.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG)
                        or member.flag_bits & 1 or not 0 < member.file_size <= MAX_MANIFEST_BYTES):
                    raise ValueError("Unsafe archive manifest artifact member")
                with archive.open(member) as incoming, (temporary / member.filename).open("xb") as outgoing:
                    shutil.copyfileobj(incoming, outgoing)
        private = temporary / "private"
        seal.unseal(temporary / "manifest.seal", private)
        if {path.name for path in private.iterdir()} != {"manifest.json", "provenance.json"}:
            raise ValueError("Unexpected decrypted manifest members")
        inner = read_json(private / "provenance.json")
        manifest = read_json(private / "manifest.json")
        if inner.get("manifest_sha256") != hashlib.sha256((private / "manifest.json").read_bytes()).hexdigest():
            raise ValueError("Private manifest content digest does not match provenance")
        verify_provenance(inner, read_json(temporary / "transfer-provenance.json"),
                          artifact, repository, guide_root, games)
        document = validate_manifest(manifest, guide_root, games)
        cache_root.mkdir(parents=True, exist_ok=True)
        # Link a completely written private file: no replacement of a concurrent cache writer.
        staged = cache_root / (".manifest-%s.json" % os.urandom(12).hex())
        try:
            with staged.open("x", encoding="utf-8") as stream:
                os.chmod(staged, 0o600)
                json.dump(document, stream, sort_keys=True)
                stream.flush()
                os.fsync(stream.fileno())
            os.link(staged, destination)
        finally:
            staged.unlink(missing_ok=True)
    return True
