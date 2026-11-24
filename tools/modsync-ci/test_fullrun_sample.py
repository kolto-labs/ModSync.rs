"""The sample download mode: one drawn archive source per game is downloaded live, the rest come from cache."""
import json
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest import mock

import cache
import fullrun_contract
import fullrun_stage

SCOPE = {"archive_source_ids": ["a", "b", "c", "helper"], "non_acquisition_policy_source_ids": ["helper"],
         "non_archive_provenance_source_ids": []}
RUN = {"GITHUB_RUN_ID": "4242", "GITHUB_RUN_ATTEMPT": "1"}
SEED = "4242:1"  # what sample_seed derives from RUN


def restored(identity, mode, attempted=False):
    return {"id": identity, "status": "restored",
            "acquisition": {"download_mode": mode, "provider_attempted": attempted}}


class Draw(unittest.TestCase):
    def test_draw_is_repeatable_per_run_and_never_a_policy_helper(self):
        seed = fullrun_stage.sample_seed(RUN)
        self.assertEqual(seed, SEED)
        first = fullrun_stage.sample_source(SCOPE, "k1", seed)
        self.assertEqual(first, fullrun_stage.sample_source(SCOPE, "k1", seed))
        drawn = {fullrun_stage.sample_source(SCOPE, "k1", "%d:1" % run) for run in range(200)}
        self.assertEqual(drawn, {"a", "b", "c"})
        self.assertIsNone(fullrun_stage.sample_source(
            {"archive_source_ids": ["helper"], "non_acquisition_policy_source_ids": ["helper"]}, "k1", seed))
        self.assertEqual(fullrun_stage.sample_seed({}), "local:1")

    def test_only_the_three_modes_exist_and_sample_restores_from_cache(self):
        modes = {mode: fullrun_stage.cache_mode(types.SimpleNamespace(download_mode=mode))
                 for mode in fullrun_stage.DOWNLOAD_MODES}
        self.assertEqual(modes, {"cache-first": "cache-first", "sample": "cache-first", "refresh": "refresh"})
        with self.assertRaises(ValueError):
            fullrun_stage.cache_mode(types.SimpleNamespace(download_mode="everything"))
        # Older callers pass no mode at all and keep the cache-first behaviour.
        self.assertEqual(fullrun_stage.cache_mode(types.SimpleNamespace()), "cache-first")

    def test_nothing_is_drawn_when_no_source_can_be_downloaded(self):
        nothing = {"archive_source_ids": [], "non_acquisition_policy_source_ids": []}
        self.assertIsNone(fullrun_stage.sample_source(nothing, "k1", SEED))
        only = {"archive_source_ids": ["solo"], "non_acquisition_policy_source_ids": []}
        self.assertEqual({fullrun_stage.sample_source(only, game, "%d:1" % run)
                          for game in ("k1", "k2") for run in range(20)}, {"solo"})


class Validation(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.output = Path(directory.name)
        self.args = types.SimpleNamespace(output=str(self.output), download_mode="sample", install_profile="exhaustive")
        for patcher in (mock.patch.object(fullrun_stage, "install_scope", return_value=SCOPE),
                        mock.patch.dict(os.environ, RUN)):
            patcher.start()
            self.addCleanup(patcher.stop)
        self.chosen = fullrun_stage.sample_source(SCOPE, "k1", SEED)
        entries = [restored(identity, "cache-first") for identity in ("a", "b", "c")]
        entries.append({"id": "helper", "status": cache.UNCACHED_SKIPPED})
        (self.output / "verify-k1.json").write_text(json.dumps({"command": "restore", "ok": True, "entries": entries}))

    def sample(self, entries, ok=True):
        fullrun_stage.sample_report(self.output, "k1").write_text(
            json.dumps({"command": "restore", "ok": ok, "entries": entries}))

    def test_a_recorded_live_attempt_on_the_drawn_source_passes(self):
        self.sample([restored(self.chosen, "refresh", attempted=True)])
        result = fullrun_stage.downloader_validation(self.args, ["k1"])
        self.assertEqual((result["status"], result["live_sample_source_ids"], result["download_mode"]),
                         ("passed", [self.chosen], "sample"))
        self.assertEqual(result["unattempted_required_source_ids"], [])

    def test_a_missing_or_weaker_sample_fails(self):
        other = next(identity for identity in ("a", "b", "c") if identity != self.chosen)
        self.assertEqual(fullrun_stage.downloader_validation(self.args, ["k1"])["status"], "failed")
        for label, entries, ok in (("cache only", [restored(self.chosen, "refresh")], True),
                                   ("wrong mode", [restored(self.chosen, "cache-first", attempted=True)], True),
                                   ("other source", [restored(other, "refresh", attempted=True)], True),
                                   ("not ok", [restored(self.chosen, "refresh", attempted=True)], False),
                                   ("empty", [], True)):
            with self.subTest(label=label):
                self.sample(entries, ok)
                result = fullrun_stage.downloader_validation(self.args, ["k1"])
                self.assertEqual((result["status"], result["incomplete_games"]), ("failed", ["k1"]))

    def test_the_other_modes_do_not_need_a_sample(self):
        self.args.download_mode = "cache-first"
        self.assertEqual(fullrun_stage.downloader_validation(self.args, ["k1"])["status"], "passed")
        self.args.download_mode = "refresh"
        # Refresh still demands a provider attempt for every source, which cached entries lack.
        self.assertEqual(fullrun_stage.downloader_validation(self.args, ["k1"])["status"], "failed")


class Stage(unittest.TestCase):
    def run_stage(self, mode):
        class Reached(Exception):
            pass

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = types.SimpleNamespace(guides=str(root / "guides"), cache=str(root / "cache"),
                                         output=str(root / "evidence"), download_mode=mode, install_profile="minimal")
            with mock.patch.object(fullrun_stage, "install_scope", return_value=SCOPE), \
                    mock.patch.object(fullrun_stage, "guides", return_value=["guide.md"]), \
                    mock.patch.object(fullrun_stage.fullrun_archive_manifest, "recover"), \
                    mock.patch.object(fullrun_stage, "uncached_options", return_value=[]), \
                    mock.patch.object(fullrun_stage, "remote_options", return_value=[]), \
                    mock.patch.object(fullrun_stage, "checked") as checked, \
                    mock.patch.object(fullrun_stage, "restore_with_public_supplement", side_effect=Reached) as complete, \
                    mock.patch.dict(os.environ, RUN), self.assertRaises(Reached):
                fullrun_stage.refresh_archives(args, ["k1"])
            return checked, complete

    def test_sample_downloads_only_the_drawn_source_live_then_restores_all_from_cache(self):
        checked, complete = self.run_stage("sample")
        command = checked.call_args.args[0]
        chosen = fullrun_stage.sample_source(SCOPE, "k1", SEED)
        self.assertEqual(checked.call_count, 1)
        self.assertEqual(command[command.index("--download-mode") + 1], "refresh")
        self.assertEqual([command[index + 1] for index, word in enumerate(command) if word == "--source-id"], [chosen])
        everything = complete.call_args.args[0]
        self.assertEqual(everything[everything.index("--download-mode") + 1], "cache-first")
        self.assertEqual(everything.count("--source-id"), len(SCOPE["archive_source_ids"]))

    def test_a_failed_live_download_fails_the_stage_and_is_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = types.SimpleNamespace(guides=str(root / "guides"), cache=str(root / "cache"),
                                         output=str(root / "evidence"), download_mode="sample",
                                         install_profile="minimal")
            failure = ValueError("live download command failed")
            with mock.patch.object(fullrun_stage, "install_scope", return_value=SCOPE), \
                    mock.patch.object(fullrun_stage, "guides", return_value=["guide.md"]), \
                    mock.patch.object(fullrun_stage.fullrun_archive_manifest, "recover"), \
                    mock.patch.object(fullrun_stage, "uncached_options", return_value=[]), \
                    mock.patch.object(fullrun_stage, "remote_options", return_value=[]), \
                    mock.patch.object(fullrun_stage.contract, "emit_outputs") as outputs, \
                    mock.patch.object(fullrun_stage, "checked", side_effect=failure), \
                    mock.patch.object(fullrun_stage, "restore_with_public_supplement") as complete, \
                    mock.patch.dict(os.environ, RUN), self.assertRaises(ValueError) as raised:
                fullrun_stage.refresh(args, ["k1"])
            # The failure is the download's own, nothing was restored after it, and the
            # stage still wrote and published a failed validation.
            self.assertIs(raised.exception, failure)
            complete.assert_not_called()
            report = json.loads((root / "evidence" / "downloader-validation.json").read_text())
            self.assertEqual((report["status"], report["incomplete_games"]), ("failed", ["k1"]))
            outputs.assert_called_once_with({"download_validation": "failed"})

    def test_other_modes_make_no_separate_live_download(self):
        for mode in ("cache-first", "refresh"):
            checked, complete = self.run_stage(mode)
            everything = complete.call_args.args[0]
            self.assertEqual((checked.call_count, everything[everything.index("--download-mode") + 1]), (0, mode))


class Policy(unittest.TestCase):
    def test_release_candidates_sample_by_default_and_may_still_refresh(self):
        payload = {"pull_request": {"base": {"ref": "main"}, "head": {"ref": "feature"}, "labels": []}}
        policy = fullrun_contract.workflow_policy("pull_request", payload)
        self.assertEqual((policy["download_mode"], policy["release_candidate"]), ("sample", True))
        refreshed = fullrun_contract.workflow_policy("pull_request", payload, {"release_download_mode": "refresh"})
        self.assertEqual(refreshed["download_mode"], "refresh")
        with self.assertRaises(ValueError):
            fullrun_contract.workflow_policy("pull_request", payload, {"release_download_mode": "cache-first"})
        manual = fullrun_contract.workflow_policy("workflow_dispatch", None, None, "minimal", "sample")
        self.assertEqual(manual["download_mode"], "sample")


if __name__ == "__main__":
    unittest.main()
