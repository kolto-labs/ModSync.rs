#!/usr/bin/env python3
"""Create the one BitTorrent v1 identity that belongs to a pinned archive version.

A pinned version is its archive bytes (SHA-256) plus its file name. This module
derives the info dictionary, info hash and magnet for that pair with the same
rules as ``src/p2p.rs::rebuild_single_info``: one file, 2 MiB pieces, bencode keys
``length``/``name``/``piece length``/``pieces`` and the final-piece behaviour of
the pinned librqbit 9.0.1 creator. The result is a pure function of
(bytes, name), so the same pinned version always yields the same info hash.

No network access, no torrent session and no external program is used.

Pin files (``fixtures/ci/{k1,k2}-p2p-identity.json``) sit beside the manual plans
because the plan parser rejects unknown keys. Shape::

    {"schema": 1, "piece_length": 2097152,
     "archives": {"{archives}/Name.zip": {"sha256": "<hex>", "bytes": 123,
                  "name": "Name.zip", "info_hash": "<40 hex>",
                  "magnet": "magnet:?xt=urn%3Abtih%3A<40 hex>&dn=Name.zip"}}}
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import urllib.parse

PIECE_LENGTH = 2 * 1024 * 1024
MAX_INFO = 32 * 1024
SCHEMA = 1
PREFIX = "{archives}/"
SHA256 = re.compile(r"[0-9a-f]{64}")
INFO_HASH = re.compile(r"[0-9a-f]{40}")
_REFUSED = set(':\0<>|"*?/\\')
_DEVICES = {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"}
_FORM_SAFE = frozenset(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789*-._")


class IdentityError(ValueError):
    """The pair (bytes, name) cannot carry a ModSync torrent identity."""


def _reserved(name):
    stem = name.split(".")[0].rstrip(" ").upper()
    stem = "".join(chr(ord(char) - 32) if "a" <= char <= "z" else char for char in stem)
    if stem in _DEVICES:
        return True
    for prefix in ("COM", "LPT"):
        rest = stem[len(prefix):] if stem.startswith(prefix) else ""
        if len(rest) == 1 and rest in "0123456789¹²³":
            return True
    return False


def safe_name(name):
    """Mirror of ``p2p.rs::safe_file_name``: one literal, portable file name."""
    return (isinstance(name, str) and name not in ("", ".", "..")
            and len(name.encode("utf-8", "surrogatepass")) <= 255
            and not _REFUSED.intersection(name) and not name.endswith((" ", "."))
            and not _reserved(name))


def _encode(text):
    """``application/x-www-form-urlencoded`` exactly as the engine's URL library writes it."""
    return "".join(chr(byte) if byte in _FORM_SAFE else "+" if byte == 0x20 else "%%%02X" % byte
                   for byte in text.encode("utf-8"))


def magnet(info_hash, name):
    """The canonical magnet for a pinned version: the info hash and its name, nothing else."""
    if INFO_HASH.fullmatch(info_hash or "") is None or not safe_name(name):
        raise IdentityError("a magnet needs a lowercase 40-digit info hash and a safe name")
    return "magnet:?xt=" + _encode("urn:btih:" + info_hash) + "&dn=" + _encode(name)


def parse_magnet(uri):
    """Return (info_hash, name or None). Refuses conflicting or missing BTv1 hashes."""
    if not isinstance(uri, str) or not uri.startswith("magnet:?"):
        raise IdentityError("a magnet URI is required")
    info_hash, name = None, None
    for key, value in urllib.parse.parse_qsl(uri[len("magnet:?"):], keep_blank_values=True):
        if key == "xt" and value.startswith("urn:btih:"):
            found = value[len("urn:btih:"):].lower()
            if INFO_HASH.fullmatch(found) is None or info_hash not in (None, found):
                raise IdentityError("the magnet needs exactly one 40-digit hex BTv1 info hash")
            info_hash = found
        elif key == "dn" and value:
            name = value
    if info_hash is None:
        raise IdentityError("the magnet needs a BTv1 info hash")
    return info_hash, name


def create(path, name):
    """Hash ``path`` once and return its identity under ``name``.

    Returns ``{"sha256", "bytes", "name", "info_hash", "magnet"}``. Raises
    ``IdentityError`` when the engine would refuse to mint or rebuild this torrent.
    """
    if not safe_name(name):
        raise IdentityError("unsafe torrent name")
    encoded_name = name.encode("utf-8")
    content, piece = hashlib.sha256(), hashlib.sha1()
    pieces, length, remaining = bytearray(), 0, PIECE_LENGTH
    with open(path, "rb") as stream:
        while True:
            chunk = stream.read(min(remaining, 1 << 20))
            if not chunk:
                break
            content.update(chunk)
            piece.update(chunk)
            length += len(chunk)
            remaining -= len(chunk)
            if remaining == 0:
                pieces += piece.digest()
                piece, remaining = hashlib.sha1(), PIECE_LENGTH
                if len(pieces) > MAX_INFO:
                    raise IdentityError("torrent info exceeds the size limit")
    if length == 0:
        raise IdentityError("an empty file has no torrent identity")
    # The pinned creator closes with one more checksum whenever bytes were read,
    # so a file ending exactly on a piece boundary carries the SHA-1 of nothing.
    pieces += piece.digest()
    info = (b"d6:lengthi%de4:name%d:%s12:piece lengthi%de6:pieces%d:"
            % (length, len(encoded_name), encoded_name, PIECE_LENGTH, len(pieces))) + bytes(pieces) + b"e"
    if len(info) > MAX_INFO:
        raise IdentityError("torrent info exceeds the size limit")
    info_hash = hashlib.sha1(info).hexdigest()
    return {"sha256": content.hexdigest(), "bytes": length, "name": name,
            "info_hash": info_hash, "magnet": magnet(info_hash, name)}


def plan_archives(plan):
    """``{plan path: sha256}`` for every archive a manual plan pins."""
    archives = plan.get("archive_sha256")
    if not isinstance(archives, dict) or not archives:
        raise IdentityError("the plan pins no archives")
    for key, checksum in archives.items():
        if (not isinstance(key, str) or not key.startswith(PREFIX) or not isinstance(checksum, str)
                or SHA256.fullmatch(checksum) is None):
            raise IdentityError("the plan has a malformed archive pin")
    return archives


def validate_pins(document, archives):
    """Check a pin file against a plan's archives without any archive bytes.

    Every pinned entry must name a plan archive with the same SHA-256, carry a safe
    name equal to the plan's file name, and a magnet that states that name (``dn``)
    and that info hash and nothing else. One info hash never serves two contents.
    Returns the list of plan paths that are not pinned yet.
    """
    if (not isinstance(document, dict) or document.get("schema") != SCHEMA
            or document.get("piece_length") != PIECE_LENGTH or not isinstance(document.get("archives"), dict)
            or set(document) != {"schema", "piece_length", "archives"}):
        raise IdentityError("unsupported P2P identity pin file")
    owners = {}
    for key, entry in document["archives"].items():
        if key not in archives:
            raise IdentityError("P2P identity for an archive the plan does not pin: %s" % key)
        if not isinstance(entry, dict) or set(entry) != {"sha256", "bytes", "name", "info_hash", "magnet"}:
            raise IdentityError("malformed P2P identity: %s" % key)
        name, info_hash = entry["name"], entry["info_hash"]
        if entry["sha256"] != archives[key]:
            raise IdentityError("P2P identity is for other bytes than the plan pins: %s" % key)
        if name != key[len(PREFIX):] or not safe_name(name):
            raise IdentityError("P2P identity name must be the plan's safe file name: %s" % key)
        if (not isinstance(entry["bytes"], int) or isinstance(entry["bytes"], bool) or entry["bytes"] <= 0
                or not isinstance(info_hash, str) or INFO_HASH.fullmatch(info_hash) is None):
            raise IdentityError("malformed P2P identity: %s" % key)
        stated_hash, stated_name = parse_magnet(entry["magnet"])
        if stated_name is None:
            raise IdentityError("pinned magnet has no name (dn): %s" % key)
        if stated_hash != info_hash or stated_name != name or entry["magnet"] != magnet(info_hash, name):
            raise IdentityError("pinned magnet does not match its identity: %s" % key)
        if owners.setdefault(info_hash, entry["sha256"]) != entry["sha256"]:
            raise IdentityError("one info hash is pinned for two different contents: %s" % key)
    return sorted(set(archives) - set(document["archives"]))


def survey(archives, objects, pins):
    """Create the identity of every plan archive whose verified bytes are in ``objects``.

    ``objects`` is a content-addressed directory (``<sha256>`` regular files).
    Returns ``(entries, problems)``. A problem is a version whose identity cannot be
    created, or whose created identity differs from its pin.
    """
    entries, problems = {}, []
    pinned = pins.get("archives", {}) if pins else {}
    for key in sorted(archives):
        checksum, name = archives[key], key[len(PREFIX):]
        record = {"sha256": checksum, "name": name}
        source = Path(objects) / checksum
        if not safe_name(name):
            record["status"] = "unsafe-name"
            problems.append(key)
        elif source.is_symlink() or not source.is_file():
            record["status"] = "bytes-unavailable"
        else:
            try:
                identity = create(source, name)
            except IdentityError as error:
                record.update(status="not-creatable", reason=str(error))
                problems.append(key)
            except OSError as error:
                # Unreadable bytes are not evidence about the version; report, do not guess.
                record.update(status="bytes-unverified", reason=type(error).__name__)
            else:
                if identity["sha256"] != checksum:
                    record["status"] = "bytes-unverified"
                else:
                    record.update(identity)
                    if key not in pinned:
                        record["status"] = "created-unpinned"
                    elif pinned[key] == identity:
                        record["status"] = "created-matches-pin"
                    else:
                        record["status"] = "pin-mismatch"
                        problems.append(key)
        entries[key] = record
    return entries, problems


def _load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def _pin_document(entries):
    return {"schema": SCHEMA, "piece_length": PIECE_LENGTH,
            "archives": {key: {field: entries[key][field] for field in ("sha256", "bytes", "name", "info_hash", "magnet")}
                         for key in sorted(entries) if entries[key]["status"].startswith("created")}}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    one = commands.add_parser("create", help="print the identity of one archive file")
    one.add_argument("archive", type=Path)
    one.add_argument("--name", help="torrent name; defaults to the file name")
    many = commands.add_parser("survey", help="create identities for a plan's pinned archives from a hash-object cache")
    many.add_argument("--plan", type=Path, required=True)
    many.add_argument("--pins", type=Path, required=True)
    many.add_argument("--objects", type=Path, required=True)
    many.add_argument("--receipt", type=Path, required=True)
    many.add_argument("--require-bytes", action="store_true",
                      help="also fail when any pinned archive has no verified bytes in the cache")
    many.add_argument("--write-pins", action="store_true",
                      help="record newly created identities in the pin file; an existing pin is never replaced")
    args = parser.parse_args(argv)
    if args.command == "create":
        try:
            print(json.dumps(create(args.archive, args.name or args.archive.name), sort_keys=True))
        except (IdentityError, OSError) as error:
            print("torrent_identity: %s" % error, file=sys.stderr)
            return 1
        return 0
    try:
        archives = plan_archives(_load(args.plan))
        pins = _load(args.pins)
        validate_pins(pins, archives)
    except (IdentityError, OSError, ValueError) as error:
        print("torrent_identity: unusable plan or pin file: %s" % error, file=sys.stderr)
        return 1
    entries, problems = survey(archives, args.objects, pins)
    counts = {}
    for record in entries.values():
        counts[record["status"]] = counts.get(record["status"], 0) + 1
    missing = [key for key, record in entries.items() if record["status"] in ("bytes-unavailable", "bytes-unverified")]
    failed = bool(problems) or (args.require_bytes and bool(missing))
    receipt = {"schema": SCHEMA, "proof": "p2p-identity-creation", "piece_length": PIECE_LENGTH,
               "status": "failed" if failed else "passed", "plan": args.plan.name, "pinned_archives": len(archives),
               "counts": dict(sorted(counts.items())), "problems": problems, "archives": entries}
    try:
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        args.receipt.write_text(json.dumps(receipt, indent=1, sort_keys=True) + "\n", encoding="utf-8")
        if args.write_pins and not problems:
            merged = dict(_pin_document(entries)["archives"])
            merged.update(pins["archives"])
            document = {"schema": SCHEMA, "piece_length": PIECE_LENGTH,
                        "archives": {key: merged[key] for key in sorted(merged)}}
            validate_pins(document, archives)
            args.pins.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")
    except OSError as error:
        print("torrent_identity: cannot write the receipt or pin file: %s" % type(error).__name__, file=sys.stderr)
        return 1
    except IdentityError as error:
        # The merged pins failed their own rules: nothing was written to the pin file.
        print("torrent_identity: new pins refused: %s" % error, file=sys.stderr)
        return 1
    print(json.dumps({"status": receipt["status"], "plan": receipt["plan"], "pinned_archives": len(archives),
                      "counts": receipt["counts"], "problems": problems}, sort_keys=True))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
