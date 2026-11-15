#!/usr/bin/env python3
"""Acquire only the six compiled public K1 native-widescreen inputs.

`remote` enables the optional same-repository Actions artifact read tier. Public
publisher fallback needs no credentials and remains available with remote=False.
Cache-first uses verified cache tiers before publishers; refresh tries publishers
first and falls back to verified cache. cache_only forbids publisher requests.
This does not change the main archive manifest, cache objects, packs or indexes.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tempfile
import time
from types import SimpleNamespace
from urllib.parse import parse_qs, urlsplit

import cache

PROFILE_SHA256 = "5828e338a0f06e65f5ff9b0a019f64221d7090ad40d6362f397f5905609ad2e4"
GALAXY_PROFILE_SHA256 = "76794533372b440b9b804a3afac5a3909e1e323da3479bab165ffdfe664167b7"
GUIDE_SHA256 = "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed"
PLAN_SHA256 = "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74"
PROFILE_ID = "k1-1920x1080-1024-interface-letterbox-off-movies-on-v1"
REPOSITORY = "kolto-labs/ModSync.rs"
MAX_INPUT = 8 * 1024 * 1024
MAX_HTML = 2 * 1024 * 1024
CHUNK = 65536
PUBLISHER_SECONDS = 180
SOURCES = {
    "editable": ("https://deadlystream.com/files/file/1320-kotor-editable-executable/", "1320", None),
    "uniws": ("https://www.wsgf.org/downloads/uniws.zip", None, None),
    "hrmenus": ("https://deadlystream.com/files/file/1159-kotor-high-resolution-menus/", "1159", None),
    "galaxy": ("https://deadlystream.com/files/file/1068-k1-galaxy-map-fix-pack/", "1068", "38716"),
    "fourgb": ("https://ntcore.com/files/4gb_patch.zip", None, None),
    "galaxy-base": ("https://deadlystream.com/files/file/1068-k1-galaxy-map-fix-pack/", "1068", "38715"),
}


class InputError(ValueError):
    """Fixed diagnostic codes only; never embed request URLs or provider errors."""


def _require(condition, code):
    if not condition:
        raise InputError(code)


def _ordinary(info, directory=False):
    _require(not stat.S_ISLNK(info.st_mode) and not (getattr(info, "st_file_attributes", 0) & 0x400), "links-rejected")
    _require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode), "unexpected-file-kind")
    if not directory:
        _require(info.st_nlink == 1, "hard-links-rejected")


def _path(path):
    result = Path(os.path.abspath(os.fspath(path)))
    for item in reversed((result, *result.parents)):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        _ordinary(info, directory=item != result or stat.S_ISDIR(info.st_mode))
    return result


def _directory(path):
    path = _path(path)
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    _ordinary(path.lstat(), directory=True)
    return path


def _flat(name):
    _require(isinstance(name, str) and 0 < len(name) <= 240 and name.isascii()
             and name not in (".", "..") and not name.endswith((".", " "))
             and not any(ord(c) < 32 or ord(c) == 127 or c in '/\\:*?"<>|' for c in name), "unsafe-flat-filename")
    return name


def _open(path):
    path = _path(path)
    before = path.lstat()
    _ordinary(before)
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
    try:
        actual = os.fstat(descriptor)
        _ordinary(actual)
        _require((before.st_dev, before.st_ino) == (actual.st_dev, actual.st_ino), "input-object-changed")
        return os.fdopen(descriptor, "rb")
    except BaseException:
        os.close(descriptor)
        raise


def _signature(prefix, filename):
    extension = Path(filename).suffix.lower()
    return (extension == ".zip" and prefix.startswith((b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"))
            or extension == ".rar" and prefix.startswith(b"Rar!\x1a\x07")
            or extension == ".7z" and prefix.startswith(b"7z\xbc\xaf\x27\x1c"))


def _identity(path, pin, output=None):
    with _open(path) as stream:
        before = os.fstat(stream.fileno())
        _require(before.st_size == pin["bytes"] <= MAX_INPUT, "input-size-mismatch")
        digest, size, prefix = hashlib.sha256(), 0, b""
        while chunk := stream.read(CHUNK):
            size += len(chunk)
            _require(size <= pin["bytes"], "input-grew")
            prefix = (prefix + chunk)[:8] if len(prefix) < 8 else prefix
            digest.update(chunk)
            if output is not None:
                output.write(chunk)
        after = os.fstat(stream.fileno())
        _require((before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                 == (after.st_size, after.st_mtime_ns, after.st_ctime_ns), "input-changed-during-read")
    _require(size == pin["bytes"] and digest.hexdigest() == pin["sha256"], "input-sha256-mismatch")
    _require(_signature(prefix, pin["filename"]), "archive-signature-rejected")


def _present(path, pin):
    try:
        path.lstat()
    except FileNotFoundError:
        return False
    _identity(path, pin)
    return True


def _publish(temporary, target, pin):
    """No-clobber atomic publication: private temp and target share a directory."""
    try:
        os.link(temporary, target, follow_symlinks=False)
    except FileExistsError:
        _identity(target, pin)
    finally:
        temporary.unlink(missing_ok=True)
    _identity(target, pin)


def _copy(source, target, pin):
    _require(source != target, "copy-source-destination-overlap")
    _directory(target.parent)
    descriptor, name = tempfile.mkstemp(prefix=".k1-input-", dir=target.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            _identity(source, pin, output)
            output.flush()
            os.fsync(output.fileno())
        _identity(temporary, pin)
        _publish(temporary, target, pin)
    finally:
        temporary.unlink(missing_ok=True)


def _profile(path, expected):
    with _open(path) as stream:
        body = stream.read(1024 * 1024 + 1)
    _require(len(body) <= 1024 * 1024 and hashlib.sha256(body).hexdigest() == expected, "compiled-profile-pin-mismatch")
    value = json.loads(body)
    _require(value["source_guide_sha256"] == GUIDE_SHA256 and value["source_plan_sha256"] == PLAN_SHA256, "source-context-pin-mismatch")
    return value


def _pins():
    source = Path(__file__).resolve().parents[2] / "src"
    profile = _profile(source / "k1_widescreen_profile.json", PROFILE_SHA256)
    galaxy = _profile(source / "galaxy_map_vanilla.json", GALAXY_PROFILE_SHA256)
    _require(profile["profile"] == PROFILE_ID and len(profile["archives"]) == 5, "compiled-profile-scope-mismatch")
    owner = [item["canonical"] for item in profile["source_components"]
             if item["canonical"]["Guid"] == "6c3c7165-297e-80eb-b9f8-d47506dab2d5"]
    _require(len(owner) == 1 and set(owner[0]["ResourceRegistry"]) == {
        "https://deadlystream.com/files/file/1159-kotor-high-resolution-menus/?tab=comments"}, "hrmenus-original-publisher-link-mismatch")
    pins = [dict(pin) for pin in profile["archives"]] + [{"role": "galaxy-base", **galaxy["archive"]}]
    _require(pins[-1] == {"role": "galaxy-base", "filename": "K1 Galaxy Map Fix Pack.zip",
        "sha256": "ba66f17ae0bb1358b2aead343450613f5dd586c8d076e2a142629e912cf7e6b4", "bytes": 7742433}, "galaxy-base-pin-mismatch")
    _require({pin["role"] for pin in pins} == set(SOURCES) and len({pin["filename"] for pin in pins}) == 6, "input-inventory-mismatch")
    for pin in pins:
        _flat(pin["filename"])
        _require(type(pin["bytes"]) is int and 0 < pin["bytes"] <= MAX_INPUT
                 and cache.SHA256.fullmatch(pin["sha256"]), "input-pin-invalid")
    return pins


def _https(url):
    parsed = urlsplit(cache.checked_url(url))
    _require(parsed.scheme == "https" and parsed.port in (None, 443), "publisher-url-rejected")
    return url


def _stream(response, maximum, deadline):
    count = 0
    reader = getattr(response, "read1", response.read)
    while True:
        _require(time.monotonic() < deadline, "publisher-total-deadline")
        chunk = reader(min(CHUNK, maximum + 1 - count))
        if not chunk:
            return
        count += len(chunk)
        _require(count <= maximum, "publisher-body-too-large")
        yield chunk


class _Page(io.BytesIO):
    def __init__(self, body, url):
        super().__init__(body)
        self.url = url

    def geturl(self):
        return self.url


def _publisher(pin, target):
    page, file_id, row_id = SOURCES[pin["role"]]
    # Resolve the publisher's public landing page first. Download buttons carry
    # session/CSRF state which a guessed ?do=download request does not establish.
    endpoint = page
    deadline = time.monotonic() + PUBLISHER_SECONDS
    selected = False
    landing_retries = 0
    args = SimpleNamespace(timeout=30, retries=2)
    for _ in range(7):
        _require(time.monotonic() < deadline, "publisher-total-deadline")
        _https(endpoint)
        with cache.request(endpoint, args, {"Referer": page} if file_id else {}) as response:
            _https(response.geturl())
            _require(response.code == 200, "publisher-http-status")
            if "html" in response.headers.get("Content-Type", "").lower():
                _require(file_id is not None, "publisher-html-rejected")
                body = b"".join(_stream(response, MAX_HTML, deadline))
                item = {"provider": "deadlystream", "url": page}
                try:
                    rows = cache.page_rows(_Page(body, response.geturl()), item,
                        selections={file_id: [{"file": pin["filename"], "r": row_id}]})
                except cache.LandingVariance:
                    _require(landing_retries < 2, "publisher-landing-unavailable")
                    delay = 4 * 2 ** landing_retries
                    landing_retries += 1
                    _require(time.monotonic() + delay < deadline, "publisher-total-deadline")
                    time.sleep(delay)
                    continue
                _require(len(rows) == 1, "publisher-row-count")
                row = rows[0]
                _require(row["name"] in (None, pin["filename"]), "publisher-row-name")
                if row["name"] is not None:
                    _require(row_id is None or row["r"] == row_id, "publisher-row-identity")
                    selected = True
                parsed = urlsplit(_https(row["url"]))
                match = cache.FILE_ID.match(parsed.path)
                _require(parsed.hostname in ("deadlystream.com", "www.deadlystream.com")
                         and match is not None and match[1] == file_id
                         and parse_qs(parsed.query).get("do") == ["download"], "publisher-file-identity")
                _require(endpoint != row["url"], "publisher-resolution-cycle")
                endpoint = row["url"]
                continue
            disposition = response.headers.get("Content-Disposition")
            if disposition:
                message = cache.EmailMessage()
                message["Content-Disposition"] = disposition
                name = message.get_filename()
                _require(name == pin["filename"], "publisher-disposition-name")
                selected = True
            _require(file_id is None or selected, "publisher-unreviewed-name")
            length = response.headers.get("Content-Length")
            _require(length is None or length.isdigit() and int(length) == pin["bytes"], "publisher-announced-size")
            descriptor, name = tempfile.mkstemp(prefix=".k1-publisher-", dir=target.parent)
            temporary = Path(name)
            try:
                with os.fdopen(descriptor, "wb") as output:
                    for chunk in _stream(response, pin["bytes"], deadline):
                        output.write(chunk)
                    output.flush()
                    os.fsync(output.fileno())
                _identity(temporary, pin)
                _publish(temporary, target, pin)
                return
            finally:
                temporary.unlink(missing_ok=True)
    raise InputError("publisher-resolution-limit")


def _remote_context():
    """Only this job's existing same-repository GitHub token, never a PAT override."""
    if os.environ.get("GITHUB_REPOSITORY", "").casefold() != REPOSITORY.casefold():
        return None
    import artstore
    import artstore_client
    environment = {"GITHUB_REPOSITORY": REPOSITORY,
        "GITHUB_TOKEN": os.environ.get("GITHUB_TOKEN", ""), "GITHUB_API_URL": "https://api.github.com"}
    client = artstore_client.from_environment(environment, timeout=30, max_retries=2, max_wait=4, max_pages=8)
    if client is None:
        return None
    return artstore, client, artstore.load_index(client, keep=1)


def _remote_fetch(context, pin, target, scratch):
    if context is None:
        return False
    artstore, client, index = context
    record = artstore.core.lookup(index, pin["sha256"])
    if record is None:
        return False
    parts = record["parts"]
    _require(record["size"] == pin["bytes"] and 0 < len(parts) <= 8
             and sum(part["stored_size"] for part in parts) <= pin["bytes"] + 4096 * len(parts), "remote-object-bound-mismatch")
    with tempfile.TemporaryDirectory(prefix=".k1-artstore-", dir=scratch) as directory:
        path = Path(directory) / pin["sha256"]
        if not artstore.fetch(pin["sha256"], path, client=client, index=index, workdir=directory, workers=1):
            return False
        _identity(path, pin)
        _copy(path, target, pin)
        return True


def _report(path, report):
    body = (json.dumps(report, indent=2, sort_keys=True) + "\n").encode()
    _require(len(body) <= 128 * 1024, "report-size-bound")
    _directory(path.parent)
    _path(path)
    descriptor, name = tempfile.mkstemp(prefix=".k1-report-", dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(body)
            output.flush()
            os.fsync(output.fileno())
        _path(path)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def ensure(archives_root: Path, cache_root: Path, report_path: Path, *, remote=True,
           cache_only=False, download_mode='cache-first') -> dict:
    """Return a bounded report; callers must require report['ok'] before install.

    Existing wrong files are fatal and remain untouched. No payload is accepted
    from a mere listing, response metadata, or an unverified cached object.
    """
    _require(download_mode in ('cache-first', 'refresh'), "invalid-download-mode")
    _require(not (cache_only and download_mode == 'refresh'), "refresh-cache-only-conflict")
    archives, root, report_path = _path(archives_root), _path(cache_root), _path(report_path)
    _require(not (archives.is_relative_to(root) or root.is_relative_to(archives)), "archive-cache-roots-overlap")
    _require(not any(report_path.is_relative_to(path) or path.is_relative_to(report_path) for path in (archives, root)), "report-payload-path-overlap")
    report = {"schema_version": 1, "kind": "k1-native-widescreen-public-input-acquisition",
        "profile_id": PROFILE_ID, "profile_sha256": PROFILE_SHA256,
        "galaxy_profile_sha256": GALAXY_PROFILE_SHA256, "source_guide_sha256": GUIDE_SHA256,
        "source_plan_sha256": PLAN_SHA256, "download_mode": download_mode,
        "cache_only": bool(cache_only), "ok": False, "entries": []}
    context, remote_attempted = None, False
    try:
        pins = _pins()
        archives = _directory(archives)
        objects = _directory(root / "k1-widescreen-inputs" / "objects")
        def cached(pin, destination, auxiliary):
            nonlocal context, remote_attempted
            if _present(destination, pin):
                return "verified-destination"
            if _present(auxiliary, pin):
                _copy(auxiliary, destination, pin)
                return "auxiliary-cache"
            ordinary = root / "objects" / pin["sha256"]
            if _present(ordinary, pin):
                _copy(ordinary, destination, pin)
                return "existing-cache-object"
            if remote and not remote_attempted:
                remote_attempted = True
                try:
                    context = _remote_context()
                except Exception:
                    context = None
            try:
                found = _remote_fetch(context, pin, auxiliary, objects)
            except Exception:
                found = False
            if found:
                _copy(auxiliary, destination, pin)
                return "same-repository-artstore-read"
            return None

        for pin in pins:
            entry = {**pin, "status": "pending", "publisher": SOURCES[pin["role"]][0]}
            report["entries"].append(entry)
            destination, auxiliary = archives / pin["filename"], objects / pin["sha256"]
            source_class = None
            if download_mode == 'refresh':
                # Existing incorrect payloads stay fatal; never replace them even
                # when a publisher would supply a valid pinned body.
                _present(destination, pin)
                _present(auxiliary, pin)
                try:
                    with tempfile.TemporaryDirectory(prefix=".k1-refresh-", dir=objects) as temporary:
                        refreshed = Path(temporary) / pin["filename"]
                        _publisher(pin, refreshed)
                        _identity(refreshed, pin)
                        _copy(refreshed, auxiliary, pin)
                    _copy(auxiliary, destination, pin)
                    source_class = "verified-public-publisher-refresh"
                    entry["publisher_refresh"] = "verified"
                except Exception as error:
                    entry["publisher_refresh"] = "failed-cache-fallback"
                    entry["publisher_refresh_failure"] = (str(error) if isinstance(error, InputError)
                        else type(error).__name__[:96])
            if source_class is None:
                source_class = cached(pin, destination, auxiliary)
                if source_class is not None and download_mode == 'refresh':
                    source_class = "refresh-fallback-" + source_class
            if source_class is None:
                _require(not cache_only, "required-input-not-in-cache")
                _require(download_mode != 'refresh', "refresh-and-cache-unavailable")
                _publisher(pin, auxiliary)
                _copy(auxiliary, destination, pin)
                source_class = "verified-public-publisher"
            entry.update(status="verified", source_class=source_class)
            if not _present(auxiliary, pin):
                _copy(destination, auxiliary, pin)
            _identity(destination, pin)
        report["ok"] = len(report["entries"]) == 6 and all(entry["status"] == "verified" for entry in report["entries"])
    except Exception as error:
        report["failure_code"] = str(error) if isinstance(error, InputError) else "input-acquisition-failed"
        # Exception messages can contain capability URLs; the class and bounded
        # HTTP status distinguish provider failures without exposing those URLs.
        report["failure_category"] = type(error).__name__[:96]
        status = getattr(error, "code", None)
        if isinstance(error, ValueError):
            # cache.request deliberately reduces HTTP failures to this fixed
            # format, retaining neither the response body nor request URL.
            match = re.fullmatch(r"HTTP ([1-5][0-9]{2})", str(error))
            if match:
                status = int(match[1])
        cause = getattr(error, "cause", None)
        if isinstance(cause, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,95}", cause):
            report["failure_network_cause"] = cause
        if type(status) is int and 100 <= status <= 599:
            report["failure_http_status"] = status
        if report["entries"]:
            report["entries"][-1]["status"] = "failed"
    _report(report_path, report)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--no-remote", action="store_true", help="disable optional Actions artifact reads; public fallback remains available")
    parser.add_argument("--cache-only", action="store_true", help="reject misses after verified cache/artifact tiers; never contact publishers")
    parser.add_argument("--download-mode", choices=("cache-first", "refresh"), default="cache-first")
    args = parser.parse_args(argv)
    try:
        report = ensure(args.archives, args.cache, args.report, remote=not args.no_remote,
                        cache_only=args.cache_only, download_mode=args.download_mode)
    except Exception:
        print("K1 auxiliary acquisition rejected unsafe paths or report destination")
        return 1
    print(json.dumps({"ok": report["ok"], "verified": sum(entry["status"] == "verified" for entry in report["entries"]),
        "failure_code": report.get("failure_code")}, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
