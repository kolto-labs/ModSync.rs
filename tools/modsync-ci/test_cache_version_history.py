"""DeadlyStream re-uploads: a download that differs from its manual plan pin walks the version history."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("cache", Path(__file__).with_name("cache.py"))
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)

PAGE = "https://deadlystream.com/files/file/990001-example-npc-diversity/"
NAME = "Example NPC Diversity.7z"
CURRENT = b"7z-current-v1.2" * 8
V11 = b"7z-older-v1.1" * 8
V10 = b"7z-oldest-v1.0" * 8
V09 = b"7z-ancient-v0.9" * 8


def sha(data):
    return hashlib.sha256(data).hexdigest()


def item():
    kind, direct = cache.provider(PAGE)
    return {"id": cache.key_for(PAGE), "url": PAGE, "provider": kind, "direct": direct, "guides": []}


def args(**extra):
    return types.SimpleNamespace(retries=0, timeout=5, max_bytes=10 ** 6, download_timeout=60,
                                 integrity_timeout=5, **extra)


def download(version=None):
    query = "do=download&version=%d&csrfKey=key" % version if version else "do=download&csrfKey=key"
    return PAGE + "?" + query


FILE_PAGE = ("<html><body><a href='%s'>Download this file</a><ul>"
             "<li><a href='?changelog=0'>1.2</a></li>"
             "<li><a href='%s?changelog=4943'>1.1</a></li>"
             "<li><a href='%s?changelog=4000'> 1.0 </a></li>"
             "<li><a href='?changelog=3000'>0.9</a></li>"
             "<li><a href='https://example.com/files/file/990001-x/?changelog=9999'>evil</a></li>"
             "</ul></body></html>") % (download().replace("&", "&amp;"), PAGE, PAGE)


def changelog(version):
    return "<html><body><a href='%s'>Download</a></body></html>" % download(version).replace("&", "&amp;")


class Response:
    def __init__(self, url, body, content_type, disposition=None):
        self.url, self.body, self.code = url, body, 200
        self.headers = {"Content-Type": content_type, "Content-Length": str(len(body))}
        if disposition:
            self.headers["Content-Disposition"] = disposition

    def geturl(self):
        return self.url

    def read(self, size=-1):
        chunk, self.body = (self.body, b"") if size < 0 else (self.body[:size], self.body[size:])
        return chunk

    def close(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class Site:
    """Routes request() by exact URL to an HTML page or an archive body; records every URL asked for."""

    def __init__(self, archives, names=None):
        self.pages = {PAGE: FILE_PAGE, PAGE + "?changelog=4943": changelog(4943),
                      PAGE + "?changelog=4000": changelog(4000), PAGE + "?changelog=3000": changelog(3000)}
        self.archives = archives
        self.names = names or {}
        self.urls = []

    def __call__(self, url, request_args, headers=None):
        self.urls.append(url)
        if url in self.pages:
            return Response(url, self.pages[url].encode(), "text/html; charset=utf-8")
        if url in self.archives:
            return Response(url, self.archives[url], "application/x-7z-compressed",
                            'attachment; filename="%s"' % self.names.get(url, NAME))
        raise ValueError("HTTP 404")


ARCHIVES = {download(): CURRENT, download(4943): V11, download(4000): V10, download(3000): V09}
RENAMED = "Example NPC Diversity 1.2.7z"


class RenamedUploadTests(unittest.TestCase):
    """An author re-uploads under a new file name, which a pin keyed by file name cannot see."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "objects").mkdir()
        self.site = Site(ARCHIVES, {download(): RENAMED})

    def fetch(self, pins):
        with mock.patch.dict(cache.PLAN_PIN_STATE, {"pins": pins}), \
                mock.patch.object(cache, "request", self.site), \
                mock.patch.object(cache, "validate", return_value="7z"), \
                mock.patch.object(cache, "load_selections", return_value={}), \
                mock.patch.object(cache, "warn") as self.warn, \
                mock.patch.object(cache.time, "sleep"):
            return cache.fetch_item(item(), {}, self.root, args())

    def history_asked(self):
        return [url for url in self.site.urls if "changelog=" in url]

    def test_name_comparison_ignores_version_and_extension_only(self):
        same = ("PartySwap 1.5.7z", "PartySwap 1.6.1.7z", "partyswap_v2.zip", "PartySwap.rar")
        self.assertEqual({cache.stem_letters(name) for name in same}, {"partyswap"})
        self.assertNotEqual(cache.stem_letters("PartySwap HD 1.5.7z"), "partyswap")
        self.assertEqual(cache.stem_letters("1.5.7z"), "")

    def test_the_pinned_version_is_recovered_under_its_pinned_name(self):
        result = self.fetch({NAME: frozenset({sha(V10)})})
        self.assertEqual((result["sha256"], result["archive_name"]), (sha(V10), NAME))
        self.assertEqual(result["pinned_version"], {"changelog": 4000, "label": "1.0"})
        self.assertEqual(result["version_history"][0]["file"], RENAMED)
        self.assertEqual(sorted(path.name for path in (self.root / "objects").iterdir()), [sha(V10)])

    def test_a_pin_missing_from_history_keeps_the_current_upload_and_says_so(self):
        result = self.fetch({NAME: frozenset({"ab" * 32})})
        self.assertEqual((result["sha256"], result["archive_name"]), (sha(CURRENT), RENAMED))
        self.assertEqual(result["pin_not_in_history"], NAME)
        self.assertEqual([each.get("sha256") for each in result["version_history"]],
                         [sha(CURRENT), sha(V11), sha(V10), sha(V09)])
        self.assertIn(NAME, self.warn.call_args.args[0])

    def test_no_history_walk_without_exactly_one_unsatisfied_candidate(self):
        (self.root / "held").mkdir()
        cases = {
            "another mod": {"Other Mod.7z": frozenset({sha(V10)})},
            "two candidates": {NAME: frozenset({sha(V10)}), "Example NPC Diversity 0.9.zip": frozenset({sha(V09)})},
            "current bytes are themselves pinned": {NAME: frozenset({sha(V10)}), "Alias.7z": frozenset({sha(CURRENT)})},
            "no pins": {},
        }
        for label, pins in cases.items():
            with self.subTest(label=label):
                self.site.urls.clear()
                result = self.fetch(pins)
                self.assertEqual((result["sha256"], result["archive_name"]), (sha(CURRENT), RENAMED))
                self.assertEqual(self.history_asked(), [])
                self.assertNotIn("pin_not_in_history", result)
        # The pinned bytes are already in the cache: nothing to recover.
        (self.root / "objects" / sha(V10)).write_bytes(V10)
        self.site.urls.clear()
        result = self.fetch({NAME: frozenset({sha(V10)})})
        self.assertEqual((result["sha256"], self.history_asked()), (sha(CURRENT), []))


class VersionHistoryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "objects").mkdir()

    def fetch(self, pins, site, call=None):
        with mock.patch.dict(cache.PLAN_PIN_STATE, {"pins": pins}), \
                mock.patch.object(cache, "request", site), \
                mock.patch.object(cache, "validate", return_value="7z"), \
                mock.patch.object(cache, "load_selections", return_value={}), \
                mock.patch.object(cache.time, "sleep"):
            return (call or (lambda: cache.fetch_item(item(), {}, self.root, args())))()

    def stored(self):
        return sorted(path.name for path in (self.root / "objects").iterdir())

    def test_pinned_old_version_is_recovered_from_history(self):
        site = Site(ARCHIVES)
        result = self.fetch({NAME: frozenset({sha(V10)})}, site)
        self.assertEqual(result["sha256"], sha(V10))
        self.assertEqual(result["archive_name"], NAME)
        self.assertEqual((self.root / "objects" / sha(V10)).read_bytes(), V10)
        self.assertEqual(self.stored(), [sha(V10)], "only the pinned bytes are stored")
        self.assertEqual(result["pinned_version"], {"changelog": 4000, "label": "1.0"})
        history = [(each["changelog"], each.get("label"), each.get("sha256"), each.get("size"))
                   for each in result["version_history"]]
        self.assertEqual(history, [(0, "current", sha(CURRENT), len(CURRENT)), (4943, "1.1", sha(V11), len(V11)),
                                   (4000, "1.0", sha(V10), len(V10))])
        self.assertTrue(all(each["file"] == NAME for each in result["version_history"]))
        self.assertNotIn(PAGE + "?changelog=3000", site.urls, "the walk stops at the first match")
        self.assertNotIn(download(3000), site.urls)
        self.assertFalse(any("example.com" in url for url in site.urls))

    def test_pinned_version_reaches_the_acquisition_receipt(self):
        site = Site(ARCHIVES)
        receipt, source = self.fetch({NAME: frozenset({sha(V11)})}, site, lambda: cache.acquire(
            item(), {}, self.root, args(download_mode="refresh"), {}))
        self.assertEqual(source, "website")
        self.assertEqual(receipt["sha256"], sha(V11))
        self.assertEqual(receipt["pinned_version"]["changelog"], 4943)
        self.assertEqual([each["changelog"] for each in receipt["version_history"]], [0, 4943])

    def test_no_matching_version_fails_loudly_and_never_uses_unpinned_bytes(self):
        site = Site(ARCHIVES)
        pin = "ab" * 32
        with self.assertRaises(cache.PinnedVersionMissing) as caught:
            self.fetch({NAME: frozenset({pin})}, site)
        message = str(caught.exception)
        self.assertTrue(message.startswith(cache.PINNED_MISSING), message)
        self.assertIn(NAME, message)
        self.assertIn(pin, message)
        self.assertEqual(self.stored(), [], "no unpinned object is stored")
        self.assertEqual([(each["changelog"], each["sha256"]) for each in caught.exception.version_history],
                         [(0, sha(CURRENT)), (4943, sha(V11)), (4000, sha(V10)), (3000, sha(V09))])

    def test_no_matching_version_fails_the_acquisition_with_its_history(self):
        site = Site(ARCHIVES)
        with self.assertRaises(cache.AcquisitionFailure) as caught:
            self.fetch({NAME: frozenset({"cd" * 32})}, site, lambda: cache.acquire(
                item(), {}, self.root, args(download_mode="refresh"), {}))
        self.assertTrue(str(caught.exception).startswith(cache.PINNED_MISSING))
        self.assertEqual(len(caught.exception.version_history), 4)
        self.assertEqual(self.stored(), [])

    def test_version_errors_are_recorded_and_the_walk_continues(self):
        site = Site({download(): CURRENT, download(4000): V10})
        site.pages[PAGE + "?changelog=4943"] = "<html><body>no download here</body></html>"
        result = self.fetch({NAME: frozenset({sha(V10)})}, site)
        self.assertEqual(result["sha256"], sha(V10))
        failed = [each for each in result["version_history"] if each["changelog"] == 4943]
        self.assertEqual(failed, [{"changelog": 4943, "label": "1.1",
                                   "error": "changelog page has no download link for its version"}])

    def test_unpinned_file_is_accepted_without_reading_the_history(self):
        site = Site(ARCHIVES)
        result = self.fetch({"Some Other Archive.7z": frozenset({sha(V10)})}, site)
        self.assertEqual(result["sha256"], sha(CURRENT))
        self.assertEqual(result["archive_name"], NAME)
        self.assertNotIn("version_history", result)
        self.assertFalse(any("changelog" in url for url in site.urls), site.urls)
        self.assertEqual(self.stored(), [sha(CURRENT)])

    def test_matching_current_download_is_accepted_without_reading_the_history(self):
        site = Site(ARCHIVES)
        result = self.fetch({NAME: frozenset({sha(CURRENT)})}, site)
        self.assertEqual(result["sha256"], sha(CURRENT))
        self.assertFalse(any("changelog" in url for url in site.urls), site.urls)

    def test_cached_object_with_other_bytes_than_the_pin_is_not_a_cache_hit(self):
        (self.root / "objects" / sha(CURRENT)).write_bytes(CURRENT)
        entry = {"sha256": sha(CURRENT), "size": len(CURRENT), "extension": "7z", "archive_name": NAME}
        check = lambda: cache.verified_selection(item(), entry, self.root, args(), {})
        self.assertFalse(self.fetch({NAME: frozenset({sha(V10)})}, Site({}), check))
        self.assertTrue(self.fetch({NAME: frozenset({sha(CURRENT)})}, Site({}), check))
        self.assertTrue(self.fetch({}, Site({}), check))


class PlanPinLoadingTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def plan(self, name, value):
        path = self.directory / name
        path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        return path

    def test_repository_plans_pin_the_drifted_archives(self):
        with mock.patch.dict(cache.PLAN_PIN_STATE, {"pins": None}):
            pins = cache.load_plan_pins()
        self.assertIn("f5da0ee1823feeba8e881dd5626361f6bd4d0e5e6d8c0764ca45e5c6c6f69fc4",
                      pins["K1 Twi_lek Male NPC Diversity.7z"])
        self.assertIn("TSL Twi_lek Male NPC Diversity.7z", pins)

    def test_plans_merge_by_file_name(self):
        one = self.plan("a.json", {"archive_sha256": {"{archives}/x.7z": "1" * 64}})
        two = self.plan("b.json", {"archive_sha256": {"{archives}/x.7z": "2" * 64, "{archives}/y.zip": "3" * 64}})
        self.assertEqual(cache.load_plan_pins([one, two]),
                         {"x.7z": frozenset({"1" * 64, "2" * 64}), "y.zip": frozenset({"3" * 64})})

    def test_unreadable_or_malformed_plan_is_an_error_not_an_empty_mapping(self):
        cases = [self.plan("broken.json", "{not json"),
                 self.plan("missing-table.json", {"steps": []}),
                 self.plan("bad-sha.json", {"archive_sha256": {"{archives}/x.7z": "nothex"}}),
                 self.plan("bad-key.json", {"archive_sha256": {"x.7z": "1" * 64}}),
                 self.directory / "absent.json"]
        for path in cases:
            with self.subTest(path=path.name), self.assertRaises(ValueError) as caught:
                cache.load_plan_pins([path])
            self.assertIn("manual plan pins", str(caught.exception))

    def test_other_providers_are_never_plan_checked(self):
        other = {"provider": "http", "url": "https://example.com/x.7z"}
        with mock.patch.dict(cache.PLAN_PIN_STATE, {"pins": {NAME: frozenset({"1" * 64})}}):
            self.assertIsNone(cache.plan_pin_for(other, NAME))
            self.assertEqual(cache.plan_pin_for(item(), NAME), frozenset({"1" * 64}))


if __name__ == "__main__":
    unittest.main()
