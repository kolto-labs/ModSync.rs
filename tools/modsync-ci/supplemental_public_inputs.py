#!/usr/bin/env python3
"""Complete the exact original K2 DS1138 public source group after legacy restore.

Only the pinned Main Menu base plus optional logo are admitted. Cache objects and
restore records are created from verified bytes, never from declared filenames.
"""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
from urllib.parse import parse_qs, urlsplit

import cache
import k1_widescreen_inputs as public

GUIDE_SHA256 = "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f"
AUTHORITY_SHA256 = "92fad6add3641d53416effbf8a1360c51fb9b64fc340273c2baa33b06672ee74"
OWNER = "14d721c3-67f9-885b-9736-04282d41bed0"
URL = "https://deadlystream.com/files/file/1138-tsl-main-menu-model-fix-for-widescreen/"
SOURCE_ID = "e65b3e0b9fc29a47556948301b140691814956b4633722eb1d18d5fa95621ef0"
POLICY = "original-k2-main-menu-public-pinned-pair-v1"
MAX_BODY = 3 * 1024 * 1024
PINS = (
    {"filename": "[TSL]_Main_Menu_Widescreen_Fix_v1.2.7z", "bytes": 2147838,
     "sha256": "ebd3ca1549cf82b961b535ae2180ff78160ceac46cf8d417b42b1143c94c63b5", "active": True},
    {"filename": "Updated_TSLRCM_Logo_v1.8.6.7z", "bytes": 124981,
     "sha256": "7274ad04de521caf4e8819e431a90612325308336136db54f72ed70af93dd272", "active": False},
)


def _require(condition, code):
    public._require(condition, code)


def applies(guide_root):
    path = public._path(Path(guide_root) / "content/k2/full.md")
    try:
        with public._open(path) as source:
            body = source.read(4 * 1024 * 1024 + 1)
    except FileNotFoundError:
        return False
    return len(body) <= 4 * 1024 * 1024 and hashlib.sha256(body).hexdigest() == GUIDE_SHA256


def _authority():
    path = Path(__file__).resolve().parents[2] / "src/archive_generation/source_plan_namespace/supplemental_authority.json"
    with public._open(path) as source:
        body = source.read(2 * 1024 * 1024 + 1)
    _require(hashlib.sha256(body).hexdigest() == AUTHORITY_SHA256, "supplemental-authority-pin-mismatch")
    authority = json.loads(body)
    rows = [row for row in authority["rows"] if row["guid"] == OWNER]
    _require(len(rows) == 1 and rows[0]["game"] == "k2"
             and authority["source_guide_sha256"]["k2"] == GUIDE_SHA256
             and set(rows[0]["raw_component"]["ResourceRegistry"]) == {URL}
             and cache.key_for(URL) == SOURCE_ID, "supplemental-original-source-mismatch")
    expected = [{"filename": pin["filename"], "sha256": pin["sha256"], "source_url": URL,
                 "source_id": SOURCE_ID, "publisher_filename": pin["filename"],
                 "active": pin["active"], "exact_bytes_observed": True} for pin in PINS]
    _require(rows[0]["inputs"] == expected, "supplemental-required-pair-mismatch")
    item = {"id": SOURCE_ID, "provider": "deadlystream", "url": URL}
    _require(cache.multi_selectors(item) == [{"file": pin["filename"], "r": None} for pin in PINS],
             "supplemental-reviewed-selection-mismatch")
    return item


def _json(path):
    with public._open(path) as source:
        body = source.read(32 * 1024 * 1024 + 1)
    _require(len(body) <= 32 * 1024 * 1024, "supplemental-metadata-too-large")
    return json.loads(body)


def _artstore(pin, destination):
    """Use the existing own-repository sealed-object transport, never mod sites."""
    module = cache.artstore_module()
    session = cache.artstore_session() if module is not None else None
    if session is None or getattr(session["client"], "repo", "").casefold() != "kolto-labs/modsync.rs":
        return False
    record = module.core.lookup(session["index"], pin["sha256"])
    if record is None:
        return False
    parts = record["parts"]
    _require(record["size"] == pin["bytes"] <= MAX_BODY and len(parts) <= 4
             and sum(part["stored_size"] for part in parts) <= MAX_BODY + 1024,
             "supplemental-artstore-size-mismatch")
    if not module.fetch(pin["sha256"], destination, client=session["client"],
                        index=session["index"], workdir=destination.parent, workers=1):
        destination.unlink(missing_ok=True)
        return False
    public._identity(destination, pin)
    # import_existing below additionally checks archive CRC before association.
    return True


def _publisher(pin, destination):
    """Resolve only this public DS file group with bounded CSRF/landing hops."""
    endpoint, selected = URL, False
    deadline = time.monotonic() + 180
    args = SimpleNamespace(timeout=30, retries=2)
    landing_retries = 0
    for _ in range(7):
        _require(time.monotonic() < deadline, "supplemental-publisher-deadline")
        public._https(endpoint)
        with cache.request(endpoint, args, {"Referer": URL}) as response:
            public._https(response.geturl())
            _require(response.code == 200, "supplemental-publisher-status")
            if "html" in response.headers.get("Content-Type", "").lower():
                body = b"".join(public._stream(response, public.MAX_HTML, deadline))
                try:
                    rows = cache.page_rows(public._Page(body, response.geturl()),
                        {"provider": "deadlystream", "url": URL},
                        selections={"1138": [{"file": pin["filename"], "r": None}]})
                except cache.LandingVariance:
                    _require(landing_retries < 2, "supplemental-publisher-landing")
                    delay = 4 * 2 ** landing_retries
                    landing_retries += 1
                    _require(time.monotonic() + delay < deadline, "supplemental-publisher-deadline")
                    time.sleep(delay)
                    continue
                _require(len(rows) == 1 and rows[0]["name"] in (None, pin["filename"]),
                         "supplemental-publisher-row")
                selected |= rows[0]["name"] == pin["filename"]
                parsed = urlsplit(public._https(rows[0]["url"]))
                match = cache.FILE_ID.match(parsed.path)
                _require(parsed.hostname in ("deadlystream.com", "www.deadlystream.com")
                         and match is not None and match[1] == "1138"
                         and parse_qs(parsed.query).get("do") == ["download"]
                         and endpoint != rows[0]["url"], "supplemental-publisher-identity")
                endpoint = rows[0]["url"]
                continue
            disposition = response.headers.get("Content-Disposition")
            if disposition:
                message = cache.EmailMessage()
                message["Content-Disposition"] = disposition
                _require(message.get_filename() == pin["filename"], "supplemental-publisher-filename")
                selected = True
            _require(selected, "supplemental-publisher-unreviewed-name")
            length = response.headers.get("Content-Length")
            _require(length is None or length.isdigit() and int(length) == pin["bytes"],
                     "supplemental-publisher-size")
            descriptor, name = tempfile.mkstemp(prefix=".supplemental-public-", dir=destination.parent)
            temporary = Path(name)
            try:
                with os.fdopen(descriptor, "wb") as output:
                    for chunk in public._stream(response, min(MAX_BODY, pin["bytes"]), deadline):
                        output.write(chunk)
                    output.flush()
                    os.fsync(output.fileno())
                public._identity(temporary, pin)
                public._publish(temporary, destination, pin)
                return
            finally:
                temporary.unlink(missing_ok=True)
    raise public.InputError("supplemental-publisher-resolution-limit")


def _canonical(entry):
    objects = cache.entry_objects(entry)
    expected = {(pin["filename"], pin["sha256"], pin["bytes"]) for pin in PINS}
    return (len(objects) == 2 and "objects" in entry
            and entry.get("size") == sum(pin["bytes"] for pin in PINS)
            and {(obj.get("archive_name"), obj.get("sha256"), obj.get("size")) for obj in objects} == expected
            and all(obj.get("extension") == "7z" and obj.get("listed_name") == obj.get("archive_name")
                    and obj.get("row") is None for obj in objects))


def ensure(guide_root, archives_root, cache_root, restore_report_path, archive_map_path,
           report_path, *, public_inputs=None, download_mode="cache-first"):
    """Complete one physical source group; leave every other association intact.

    public_inputs is an optional explicit directory of the same pinned bytes.
    Cache-first uses verified destination/objects and the encrypted artifact
    store before publisher fallback. Refresh attempts the publisher first and
    retains verified cached inputs if that attempt fails or changes version.
    """
    if not applies(guide_root):
        return {"ok": True, "applicable": False}
    _require(download_mode in ("cache-first", "refresh"), "supplemental-download-mode")
    item = _authority()
    archives, root = public._directory(archives_root), public._directory(cache_root)
    report_path = public._path(report_path)
    restore_report_path, archive_map_path = public._path(restore_report_path), public._path(archive_map_path)
    _require(not (archives.is_relative_to(root) or root.is_relative_to(archives)), "supplemental-roots-overlap")
    for metadata in (report_path, restore_report_path, archive_map_path):
        _require(not any(metadata.is_relative_to(payload) or payload.is_relative_to(metadata)
                         for payload in (archives, root)), "supplemental-report-payload-overlap")
    report = {"schema_version": 1, "policy": POLICY, "applicable": True, "ok": False,
              "source_id": SOURCE_ID, "download_mode": download_mode, "source_guide_sha256": GUIDE_SHA256,
              "supplemental_authority_sha256": AUTHORITY_SHA256, "entries": []}
    try:
        restored, mapping = _json(restore_report_path), _json(archive_map_path)
        _require(isinstance(restored, dict) and restored.get("command") == "restore"
                 and isinstance(restored.get("entries"), list) and isinstance(mapping, dict),
                 "supplemental-restore-schema")
        owners = [entry for entry in restored["entries"] if entry.get("id") == SOURCE_ID]
        _require(len(owners) == 1 and owners[0].get("url") == URL
                 and all(entry.get("status") in ("restored", cache.UNCACHED_SKIPPED)
                         for entry in restored["entries"] if entry.get("id") != SOURCE_ID),
                 "supplemental-unrelated-restore-failure")
        manifest_path = root / "manifest.json"
        document = _json(manifest_path)
        _require(document.get("schema") == 1 and isinstance(document.get("entries"), dict),
                 "supplemental-manifest-schema")
        old = document["entries"].get(SOURCE_ID)
        with tempfile.TemporaryDirectory(prefix="supplemental-public-inputs-", dir=root.parent) as temporary:
            stage = Path(temporary)
            for pin in PINS:
                destination = stage / pin["filename"]
                origin, refresh_failure = None, None
                if download_mode == "refresh":
                    try:
                        _publisher(pin, destination)
                        origin = "verified-public-publisher"
                    except Exception as error:
                        destination.unlink(missing_ok=True)
                        refresh_failure = (str(error) if isinstance(error, public.InputError)
                                           else "publisher-unavailable")
                if origin is None:
                    candidates = [(archives / pin["filename"], "verified-restored-input"),
                                  (root / "objects" / pin["sha256"], "verified-cache-object")]
                    if public_inputs is not None:
                        candidates.append((public._path(public_inputs) / pin["filename"],
                                           "verified-operator-input"))
                    for source, candidate_origin in candidates:
                        if public._present(source, pin):
                            public._copy(source, destination, pin)
                            origin = candidate_origin
                            break
                if origin is None and _artstore(pin, destination):
                    origin = "verified-encrypted-artstore"
                if origin is None:
                    # Refresh already attempted this source; do not retry it or
                    # silently skip a required input when every cache tier misses.
                    _require(download_mode != "refresh", "supplemental-refresh-and-cache-unavailable")
                    _publisher(pin, destination)
                    origin = "verified-public-publisher"
                public._identity(destination, pin)
                entry = {**pin, "source_class": origin, "status": "verified",
                         "refresh_attempted": download_mode == "refresh"}
                if refresh_failure is not None:
                    entry["refresh_fallback_reason"] = refresh_failure
                report["entries"].append(entry)
            enriched = not _canonical(old)
            if enriched:
                updated = copy.deepcopy(document)
                previous = updated.setdefault("supplemental_public_inputs", {})
                _require(isinstance(previous, dict), "supplemental-provenance-schema")
                _require(SOURCE_ID not in previous or isinstance(previous[SOURCE_ID], dict)
                         and previous[SOURCE_ID].get("policy") == POLICY,
                         "supplemental-provenance-policy-mismatch")
                previous.setdefault(SOURCE_ID, {"policy": POLICY, "source_guide_sha256": GUIDE_SHA256,
                    "authority_sha256": AUTHORITY_SHA256, "previous_association": old,
                    "public_pins": [{key: pin[key] for key in ("filename", "sha256", "bytes")} for pin in PINS]})
                updated["entries"].pop(SOURCE_ID, None)
                args = SimpleNamespace(apply=True, max_bytes=MAX_BODY, integrity_timeout=30,
                    import_manifest=updated["entries"], import_manifest_document=updated)
                resources = {URL: {"Files": {pin["filename"]: {} for pin in PINS},
                                   "HandlerMetadata": {"PrimaryUrl": URL}}}
                imported = cache.import_existing(item, resources, stage, root, args,
                    expected_sha256={pin["filename"]: pin["sha256"] for pin in PINS})
                _require(imported.get("status") == "pinned" and _canonical(updated["entries"].get(SOURCE_ID)),
                         "supplemental-canonical-import-failed")
                updated["entries"][SOURCE_ID]["evidence"] = "verified-pinned-source-supplement"
                for obj in updated["entries"][SOURCE_ID]["objects"]:
                    obj["evidence"] = "verified-pinned-source-supplement"
                cache.atomic_json(manifest_path, updated)
                document = updated
            else:
                # Rehydrate pruned objects from verified public bytes without
                # replacing the existing canonical association or its provenance.
                args = SimpleNamespace(apply=True, max_bytes=MAX_BODY, integrity_timeout=30,
                    import_manifest=document["entries"], import_manifest_document=document)
                resources = {URL: {"Files": {pin["filename"]: {} for pin in PINS},
                                   "HandlerMetadata": {"PrimaryUrl": URL}}}
                imported = cache.import_existing(item, resources, stage, root, args,
                    expected_sha256={pin["filename"]: pin["sha256"] for pin in PINS})
                _require(imported.get("status") == "already-present", "supplemental-object-rehydration-failed")
            objects = cache.entry_objects(document["entries"][SOURCE_ID])
            names = {path.name.casefold(): "existing-unrelated-input" for path in archives.iterdir()
                     if path.name not in {pin["filename"] for pin in PINS}}
            copied, taken, created = [], set(), []
            try:
                for obj in objects:
                    name = cache.restore_object(item, obj, root,
                        SimpleNamespace(destination=archives, integrity_timeout=30), names, taken, created)
                    copied.append({"file": name, "sha256": obj["sha256"]})
            except Exception:
                for path in created:
                    path.unlink(missing_ok=True)
                raise
            replacement = {"id": SOURCE_ID, "url": URL, "provider": "deadlystream", "status": "restored",
                "source": "verified-pinned-supplement", "pinned": True, "metadata_unknown": True,
                "files": copied, "public_supplement_policy": POLICY}
            prior_acquisition = owners[0].get("acquisition", {})
            attempted = (prior_acquisition.get("provider_attempted") is True
                         or any(entry["refresh_attempted"]
                                or entry["source_class"] == "verified-public-publisher"
                                for entry in report["entries"]))
            fallback = (prior_acquisition.get("provider_outcome") == "cached-fallback"
                        or prior_acquisition.get("valid_cache_fallback") is True
                        or (prior_acquisition.get("provider_attempted") is True
                            and prior_acquisition.get("provider_outcome") != "downloaded"
                            and any(entry["source_class"] != "verified-public-publisher"
                                    for entry in report["entries"]))
                        or any("refresh_fallback_reason" in entry for entry in report["entries"]))
            downloaded = (prior_acquisition.get("provider_outcome") == "downloaded"
                          or prior_acquisition.get("provider_downloaded") is True
                          or any(entry["source_class"] == "verified-public-publisher"
                                 for entry in report["entries"]))
            replacement["acquisition"] = {"download_mode": download_mode,
                "provider_attempted": attempted,
                "provider_outcome": "cached-fallback" if fallback else "downloaded" if downloaded else "cache-hit",
                "provider_downloaded": downloaded, "valid_cache_fallback": fallback,
                "input_acquisition": report["entries"]}
            # Preserve the original legacy record rather than relabel its single
            # observed logo download as a two-file publisher restore.
            replacement["previous_restore_entry"] = owners[0].get("previous_restore_entry", owners[0])
            restored["entries"] = [replacement if entry.get("id") == SOURCE_ID else entry
                                   for entry in restored["entries"]]
            restored["ok"] = True
            sources = [entry["source"] for entry in restored["entries"] if "source" in entry]
            restored["sources"] = {name: sources.count(name) for name in sorted(set(sources))}
            mapping[cache.link_key(URL)] = [entry["file"] for entry in copied]
            cache.atomic_json(restore_report_path, restored)
            cache.atomic_json(archive_map_path, mapping)
            report.update(ok=True, manifest_enriched=enriched, restored_files=copied)
    except Exception as error:
        report["failure_code"] = str(error) if isinstance(error, public.InputError) else "supplemental-public-input-failed"
        report["failure_category"] = type(error).__name__[:96]
    public._directory(report_path.parent)
    cache.atomic_json(report_path, report)
    _require(report["ok"], report.get("failure_code", "supplemental-public-input-failed"))
    return report
