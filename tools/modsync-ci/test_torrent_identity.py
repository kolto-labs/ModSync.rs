"""First P2P test: a pinned archive version has exactly one creatable BitTorrent identity.

The golden vectors are the info hashes the production CLI rebuilds for these exact
bytes and names (``modsync torrent-seed --authored-magnet``, which reports its own
``rebuilt`` hash). They are recreated here without the CLI. The second P2P test,
``p2p_loopback.py``, compares this module with what the pinned torrent creator mints
in the binary it transfers with.
"""
import hashlib
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib
from contextlib import redirect_stdout

import torrent_identity as identity

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures" / "ci"
MIB = 1024 * 1024


def stored_zip(total, member=b"payload.bin"):
    """A deterministic one-member stored ZIP that is exactly ``total`` bytes long."""
    size = total - (30 + len(member)) - (46 + len(member)) - 22
    assert size > 0
    payload = hashlib.shake_256(b"modsync-p2p-identity-%d" % total).digest(size)
    crc = zlib.crc32(payload)
    local = struct.pack("<4sHHHHHIIIHH", b"PK\x03\x04", 20, 0, 0, 0, 0x21, crc, size, size, len(member), 0) + member
    central = struct.pack("<4sHHHHHHIIIHHHHHII", b"PK\x01\x02", 20, 20, 0, 0, 0, 0x21, crc, size, size,
                          len(member), 0, 0, 0, 0, 0, 0) + member
    end = struct.pack("<4sHHHHIIH", b"PK\x05\x06", 0, 0, 1, 1, len(central), len(local) + size, 0)
    return local + payload + central + end


# (torrent name, archive bytes, archive SHA-256, info hash rebuilt by the production CLI)
VECTORS = (
    ("small.zip", 4096,
     "500dcd5130be2f28c1736d5ee3dc8134cd0a0027366fbf28135999f3437b9a13", "a34d54cfb632bbbccb775347f33abd20deccdc0b"),
    ("Exactly One Piece.zip", 2 * MIB,
     "1b205ff0f6a821840f787ff9a2e5849ed81cead03866628402f50be5aa55c78c", "5e0f030badd37537996d8361edf79843c1d7db32"),
    ("one-piece-plus-one.zip", 2 * MIB + 1,
     "7f880c764694ec564e6905c7c179ef112d3f9936fea79c2b64876efd5bd9c5bb", "f5a7cf1d6a091b383e1b84d10528bdf2a3bf0603"),
    ("exactly_two_pieces.zip", 4 * MIB,
     "5ede203784d9d4a48c16db385de36636db7caa7e91f46f062c48cd00117703c8", "969b7441a518bab160b54b2314199b88413d927e"),
    ("Odd [v1.2] ~ café + more.zip", 5 * MIB + 7,
     "aa95b4296414b8f29d65e6139d4f38af2a865680617b6f58ce70c6f6031f6b86", "df7190fc13c0e3fe9d6b5a89853a4802692504f6"),
)


class Creation(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)

    def write(self, name, data):
        path = self.root / name
        path.write_bytes(data)
        return path

    def test_recreates_the_identities_the_production_cli_rebuilds(self):
        for name, total, sha256, info_hash in VECTORS:
            with self.subTest(name=name):
                created = identity.create(self.write("bytes", stored_zip(total)), name)
                self.assertEqual((created["bytes"], created["sha256"], created["info_hash"], created["name"]),
                                 (total, sha256, info_hash, name))
                self.assertEqual(identity.parse_magnet(created["magnet"]), (info_hash, name))

    def test_same_pinned_version_always_yields_the_same_info_hash(self):
        data = stored_zip(3 * MIB + 5)
        first = identity.create(self.write("a", data), "Mod 1.0.zip")
        second = identity.create(self.write("b", data), "Mod 1.0.zip")
        self.assertEqual(first, second)
        # The name is part of the identity: the same bytes under another name are another torrent.
        renamed = identity.create(self.write("c", data), "Mod 1.1.zip")
        self.assertEqual(renamed["sha256"], first["sha256"])
        self.assertNotEqual(renamed["info_hash"], first["info_hash"])
        changed = identity.create(self.write("d", data[:-1] + bytes([data[-1] ^ 1])), "Mod 1.0.zip")
        self.assertNotEqual((changed["sha256"], changed["info_hash"]), (first["sha256"], first["info_hash"]))

    def test_piece_layout_follows_the_pinned_creator(self):
        def info(total):
            created = identity.create(self.write("bytes", b"\x5a" * total), "n")
            return created["info_hash"]
        def expected(total, pieces):
            body = b"d6:lengthi%de4:name1:n12:piece lengthi2097152e6:pieces%d:" % (total, len(pieces)) + pieces + b"e"
            return hashlib.sha1(body).hexdigest()
        full, empty = hashlib.sha1(b"\x5a" * (2 * MIB)).digest(), hashlib.sha1(b"").digest()
        self.assertEqual(info(1), expected(1, hashlib.sha1(b"\x5a").digest()))
        # A file ending on a piece boundary carries one extra checksum of no bytes.
        self.assertEqual(info(2 * MIB), expected(2 * MIB, full + empty))
        self.assertEqual(info(2 * MIB + 1), expected(2 * MIB + 1, full + hashlib.sha1(b"\x5a").digest()))

    def test_refuses_what_the_engine_refuses(self):
        path = self.write("bytes", b"x")
        for name in ("", ".", "..", "a/b.zip", "a\\b.zip", "c:evil.zip", "trailing.", "trailing ", "what?.zip",
                     "star*.zip", "CON.zip", "com1", "LPT².7z", "n" * 256, 'quote".zip', "pipe|.zip"):
            with self.subTest(name=name):
                self.assertFalse(identity.safe_name(name))
                with self.assertRaises(identity.IdentityError):
                    identity.create(path, name)
        for name in ("COM10.zip", "com1x.zip", "Mod [v2] (final).7z", "café.rar", "n" * 255):
            self.assertTrue(identity.safe_name(name), name)
        with self.assertRaises(identity.IdentityError):
            identity.create(self.write("empty", b""), "empty.zip")

    def test_magnet_has_one_canonical_text(self):
        info_hash = "0123456789abcdef0123456789abcdef01234567"
        self.assertEqual(identity.magnet(info_hash, "A b~[1]+é_-.zip"),
                         "magnet:?xt=urn%3Abtih%3A" + info_hash + "&dn=A+b%7E%5B1%5D%2B%C3%A9_-.zip")
        with self.assertRaises(identity.IdentityError):
            identity.magnet(info_hash.upper(), "a.zip")
        self.assertEqual(identity.parse_magnet("magnet:?xt=urn:btih:" + info_hash.upper()), (info_hash, None))
        for bad in ("http://x", "magnet:?dn=a", "magnet:?xt=urn:btih:abc",
                    "magnet:?xt=urn:btih:%s&xt=urn:btih:%s" % (info_hash, "f" * 40)):
            with self.assertRaises(identity.IdentityError):
                identity.parse_magnet(bad)


class Pins(unittest.TestCase):
    def entry(self, name, data):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bytes"
            path.write_bytes(data)
            return identity.create(path, name)

    def document(self, **archives):
        return {"schema": 1, "piece_length": identity.PIECE_LENGTH, "archives": archives}

    def test_pin_rules(self):
        one, two = self.entry("One.zip", b"one"), self.entry("Two.zip", b"two")
        archives = {"{archives}/One.zip": one["sha256"], "{archives}/Two.zip": two["sha256"]}
        self.assertEqual(identity.validate_pins(self.document(), archives), sorted(archives))
        good = {"{archives}/One.zip": one}
        self.assertEqual(identity.validate_pins(self.document(**good), archives), ["{archives}/Two.zip"])
        nameless = dict(one, magnet="magnet:?xt=urn%3Abtih%3A" + one["info_hash"])
        with self.assertRaisesRegex(identity.IdentityError, r"no name \(dn\)"):
            identity.validate_pins(self.document(**{"{archives}/One.zip": nameless}), archives)
        tracker = dict(one, magnet=one["magnet"] + "&tr=http%3A%2F%2F192.0.2.1%2Fannounce")
        other_name = dict(one, name="Other.zip", magnet=identity.magnet(one["info_hash"], "Other.zip"))
        other_bytes = dict(one, sha256=two["sha256"])
        other_hash = dict(one, info_hash=two["info_hash"])
        shared = {"{archives}/One.zip": one, "{archives}/Two.zip": dict(two, info_hash=one["info_hash"],
                  magnet=identity.magnet(one["info_hash"], "Two.zip"))}
        for label, broken in (("tracker", {"{archives}/One.zip": tracker}), ("name", {"{archives}/One.zip": other_name}),
                              ("bytes", {"{archives}/One.zip": other_bytes}), ("hash", {"{archives}/One.zip": other_hash}),
                              ("unknown", {"{archives}/Three.zip": one}), ("shared", shared),
                              ("extra", {"{archives}/One.zip": dict(one, peers=[])}),
                              ("size", {"{archives}/One.zip": dict(one, bytes=0)})):
            with self.subTest(label=label), self.assertRaises(identity.IdentityError):
                identity.validate_pins(self.document(**broken), archives)
        for broken in ({"schema": 2, "piece_length": identity.PIECE_LENGTH, "archives": {}},
                       {"schema": 1, "piece_length": 262144, "archives": {}}, {"schema": 1, "archives": {}}, []):
            with self.assertRaises(identity.IdentityError):
                identity.validate_pins(broken, archives)

    def test_committed_pins_agree_with_the_manual_plans(self):
        for game in ("k1", "k2"):
            with self.subTest(game=game):
                plan = json.loads((FIXTURES / ("%s-manual-plan.json" % game)).read_text(encoding="utf-8"))
                archives = identity.plan_archives(plan)
                pins = json.loads((FIXTURES / ("%s-p2p-identity.json" % game)).read_text(encoding="utf-8"))
                identity.validate_pins(pins, archives)
                # Every pinned version must be nameable as a torrent before any bytes are seen.
                self.assertEqual([key for key in archives if not identity.safe_name(key[len(identity.PREFIX):])], [])


class Survey(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)
        self.objects = self.root / "objects"
        self.objects.mkdir()
        self.archives = {}
        for name, data in (("Present.zip", b"present"), ("Second.7z", b"second"), ("Absent.rar", b"absent")):
            checksum = hashlib.sha256(data).hexdigest()
            self.archives["{archives}/" + name] = checksum
            if name != "Absent.rar":
                (self.objects / checksum).write_bytes(data)
        self.plan = self.root / "plan.json"
        self.plan.write_text(json.dumps({"archive_sha256": self.archives}))
        self.pins = self.root / "pins.json"
        self.pins.write_text(json.dumps({"schema": 1, "piece_length": identity.PIECE_LENGTH, "archives": {}}))
        self.receipt = self.root / "out" / "receipt.json"

    def run_survey(self, *extra):
        with redirect_stdout(io.StringIO()):
            code = identity.main(["survey", "--plan", str(self.plan), "--pins", str(self.pins),
                                  "--objects", str(self.objects), "--receipt", str(self.receipt), *extra])
        return code, json.loads(self.receipt.read_text())

    def test_creates_every_available_version_and_pins_it_once(self):
        code, receipt = self.run_survey()
        self.assertEqual((code, receipt["status"]), (0, "passed"))
        self.assertEqual(receipt["counts"], {"bytes-unavailable": 1, "created-unpinned": 2})
        self.assertEqual(json.loads(self.pins.read_text())["archives"], {})
        code, receipt = self.run_survey("--require-bytes")
        self.assertEqual((code, receipt["status"]), (1, "failed"))
        code, _ = self.run_survey("--write-pins")
        self.assertEqual(code, 0)
        pinned = json.loads(self.pins.read_text())["archives"]
        self.assertEqual(sorted(pinned), ["{archives}/Present.zip", "{archives}/Second.7z"])
        code, receipt = self.run_survey("--write-pins")
        self.assertEqual(receipt["counts"], {"bytes-unavailable": 1, "created-matches-pin": 2})
        self.assertEqual(json.loads(self.pins.read_text())["archives"], pinned)

    def test_a_pin_that_differs_from_the_bytes_fails_and_is_never_replaced(self):
        self.run_survey("--write-pins")
        document = json.loads(self.pins.read_text())
        entry = document["archives"]["{archives}/Present.zip"]
        entry["info_hash"] = "0" * 40
        entry["magnet"] = identity.magnet("0" * 40, "Present.zip")
        self.pins.write_text(json.dumps(document))
        code, receipt = self.run_survey("--write-pins")
        self.assertEqual((code, receipt["problems"]), (1, ["{archives}/Present.zip"]))
        self.assertEqual(receipt["archives"]["{archives}/Present.zip"]["status"], "pin-mismatch")
        self.assertEqual(json.loads(self.pins.read_text()), document)

    def test_wrong_object_bytes_are_never_given_an_identity(self):
        (self.objects / self.archives["{archives}/Present.zip"]).write_bytes(b"tampered")
        code, receipt = self.run_survey()
        record = receipt["archives"]["{archives}/Present.zip"]
        self.assertEqual((code, record["status"]), (0, "bytes-unverified"))
        self.assertNotIn("info_hash", record)

    def test_io_failures_are_reported_instead_of_raised(self):
        from contextlib import redirect_stderr
        from unittest import mock
        with mock.patch.object(identity, "create", side_effect=PermissionError("denied")):
            code, receipt = self.run_survey()
        record = receipt["archives"]["{archives}/Present.zip"]
        self.assertEqual((code, record["status"], record["reason"]), (0, "bytes-unverified", "PermissionError"))
        arguments = ["survey", "--objects", str(self.objects), "--receipt", str(self.receipt)]
        self.pins.write_text("{not json")
        with redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(identity.main([*arguments, "--plan", str(self.plan), "--pins", str(self.pins)]), 1)
            self.assertEqual(identity.main([*arguments, "--plan", str(self.root / "absent"), "--pins", str(self.pins)]), 1)
        self.assertEqual(errors.getvalue().count("unusable plan or pin file"), 2)


if __name__ == "__main__":
    unittest.main()
