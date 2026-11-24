"""The sample download mode: one drawn archive source per run is downloaded live, the rest come from cache."""
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

STAGE_FIELDS = {"non_archive_provenance_source_ids": [], "profile": "minimal", "selected_count": 1,
                "source_definition_count": 1, "requires_k1_widescreen_inputs": False}
SCOPE = {"archive_source_ids": ["a", "b", "c", "helper"], "non_acquisition_policy_source_ids": ["helper"],
         **STAGE_FIELDS}
OTHER_SCOPE = {"archive_source_ids": ["x", "y"], "non_acquisition_policy_source_ids": [], **STAGE_FIELDS}
SCOPES = {"k1": SCOPE, "k2": OTHER_SCOPE}
RUN = {"GITHUB_RUN_ID": "4242", "GITHUB_RUN_ATTEMPT": "1"}
SEED = "4242:1"  # what sample_seed derives from RUN
GAMES = ["k1", "k2"]


def restored(identity, mode, attempted=False):
    return {"id": identity, "status": "restored",
            "acquisition": {"download_mode": mode, "provider_attempted": attempted}}


def scope_of(args, game):
    return SCOPES[game]


class Draw(unittest.TestCase):
    def test_one_draw_per_run_across_all_games_and_never_a_policy_helper(self):
        self.assertEqual(fullrun_stage.sample_seed(RUN), SEED)
        self.assertEqual(fullrun_stage.sample_seed({}), "local:1")
        first = fullrun_stage.sample_draw(SCOPES, SEED)
        self.assertEqual(first, fullrun_stage.sample_draw(SCOPES, SEED))
        self.assertEqual(len(first), 2)
        drawn = {fullrun_stage.sample_draw(SCOPES, "%d:1" % run) for run in range(400)}
        self.assertEqual(drawn, {("k1", "a"), ("k1", "b"), ("k1", "c"), ("k2", "x"), ("k2", "y")})
        self.assertIn(first, drawn)
        # A different attempt of the same run draws again.
        self.assertEqual(fullrun_stage.sample_seed({"GITHUB_RUN_ID": "4242", "GITHUB_RUN_ATTEMPT": "2"}), "4242:2")

    def test_nothing_is_drawn_when_no_source_can_be_downloaded(self):
        helper_only = {"archive_source_ids": ["helper"], "non_acquisition_policy_source_ids": ["helper"]}
        empty = {"archive_source_ids": [], "non_acquisition_policy_source_ids": []}
        self.assertIsNone(fullrun_stage.sample_draw({"k1": helper_only, "k2": empty}, SEED))
        self.assertIsNone(fullrun_stage.sample_draw({}, SEED))
        solo = {"k1": {"archive_source_ids": ["solo"], "non_acquisition_policy_source_ids": []}}
        self.assertEqual({fullrun_stage.sample_draw(solo, "%d:1" % run) for run in range(20)}, {("k1", "solo")})

    def test_only_the_three_modes_exist_and_sample_restores_from_cache(self):
        modes = {mode: fullrun_stage.cache_mode(types.SimpleNamespace(download_mode=mode))
                 for mode in fullrun_stage.DOWNLOAD_MODES}
        self.assertEqual(modes, {"cache-first": "cache-first", "sample": "cache-first", "refresh": "refresh"})
        with self.assertRaises(ValueError):
            fullrun_stage.cache_mode(types.SimpleNamespace(download_mode="everything"))
        # Older callers pass no mode at all and keep the cache-first behaviour.
        self.assertEqual(fullrun_stage.cache_mode(types.SimpleNamespace()), "cache-first")


class Validation(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.output = Path(directory.name)
        self.args = types.SimpleNamespace(output=str(self.output), download_mode="sample", install_profile="exhaustive")
        for patcher in (mock.patch.object(fullrun_stage, "install_scope", side_effect=scope_of),
                        mock.patch.dict(os.environ, RUN)):
            patcher.start()
            self.addCleanup(patcher.stop)
        self.game, self.chosen = fullrun_stage.sample_draw(SCOPES, SEED)
        for game, scope in SCOPES.items():
            entries = [restored(identity, "cache-first") for identity in scope["archive_source_ids"]
                       if identity not in scope["non_acquisition_policy_source_ids"]]
            entries += [{"id": identity, "status": cache.UNCACHED_SKIPPED}
                        for identity in scope["non_acquisition_policy_source_ids"]]
            (self.output / ("verify-%s.json" % game)).write_text(
                json.dumps({"command": "restore", "ok": True, "entries": entries}))

    def sample(self, entries, ok=True, game=None):
        fullrun_stage.sample_report(self.output, game or self.game).write_text(
            json.dumps({"command": "restore", "ok": ok, "entries": entries}))

    def test_a_recorded_live_attempt_on_the_drawn_source_passes(self):
        self.sample([restored(self.chosen, "refresh", attempted=True)])
        result = fullrun_stage.downloader_validation(self.args, GAMES)
        self.assertEqual((result["status"], result["live_sample_source_ids"], result["download_mode"]),
                         ("passed", [self.chosen], "sample"))
        self.assertEqual(result["unattempted_required_source_ids"], [])

    def test_a_missing_or_weaker_sample_fails(self):
        other = next(identity for identity in SCOPES[self.game]["archive_source_ids"] if identity != self.chosen)
        other_game = next(game for game in GAMES if game != self.game)
        self.assertEqual(fullrun_stage.downloader_validation(self.args, GAMES)["status"], "failed")
        for label, entries, ok in (("cache only", [restored(self.chosen, "refresh")], True),
                                   ("wrong mode", [restored(self.chosen, "cache-first", attempted=True)], True),
                                   ("other source", [restored(other, "refresh", attempted=True)], True),
                                   ("not ok", [restored(self.chosen, "refresh", attempted=True)], False),
                                   ("empty", [], True)):
            with self.subTest(label=label):
                self.sample(entries, ok)
                result = fullrun_stage.downloader_validation(self.args, GAMES)
                self.assertEqual((result["status"], result["incomplete_games"]), ("failed", [self.game]))
        # A report filed under the game that was not drawn does not count.
        fullrun_stage.sample_report(self.output, self.game).unlink()
        self.sample([restored(self.chosen, "refresh", attempted=True)], game=other_game)
        self.assertEqual(fullrun_stage.downloader_validation(self.args, GAMES)["status"], "failed")

    def test_the_other_modes_do_not_need_a_sample(self):
        self.args.download_mode = "cache-first"
        self.assertEqual(fullrun_stage.downloader_validation(self.args, GAMES)["status"], "passed")
        self.args.download_mode = "refresh"
        # Refresh still demands a provider attempt for every source, which cached entries lack.
        self.assertEqual(fullrun_stage.downloader_validation(self.args, GAMES)["status"], "failed")


class Stage(unittest.TestCase):
    def patched(self, root, mode, checked_effect=None):
        args = types.SimpleNamespace(guides=str(root / "guides"), cache=str(root / "cache"),
                                     output=str(root / "evidence"), download_mode=mode, install_profile="minimal")
        patches = [mock.patch.object(fullrun_stage, "install_scope", side_effect=scope_of),
                   mock.patch.object(fullrun_stage, "guides", return_value=["guide.md"]),
                   mock.patch.object(fullrun_stage.fullrun_archive_manifest, "recover"),
                   mock.patch.object(fullrun_stage, "uncached_options", return_value=[]),
                   mock.patch.object(fullrun_stage, "remote_options", return_value=[]),
                   mock.patch.dict(os.environ, RUN)]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        checked = mock.patch.object(fullrun_stage, "checked", side_effect=checked_effect).start()
        self.addCleanup(mock.patch.stopall)
        return args, checked

    def test_exactly_one_source_is_downloaded_live_for_the_whole_run(self):
        game, chosen = fullrun_stage.sample_draw(SCOPES, SEED)
        with tempfile.TemporaryDirectory() as temporary:
            args, checked = self.patched(Path(temporary), "sample")
            seen = []

            def record(command, log, call_args, restored_game, *rest):
                seen.append((restored_game, checked.call_count))
                if len(seen) == len(GAMES):
                    raise KeyboardInterrupt
                Path(call_args.output, "verify-%s.json" % restored_game).write_text(json.dumps(
                    {"ok": True, "entries": [{"id": identity, "status": "restored"}
                                             for identity in SCOPES[restored_game]["archive_source_ids"]]}))

            with mock.patch.object(fullrun_stage, "restore_with_public_supplement", side_effect=record), \
                    mock.patch.object(fullrun_stage, "native_k1_scope", return_value=False), \
                    self.assertRaises(KeyboardInterrupt):
                fullrun_stage.refresh_archives(args, GAMES)
            self.assertEqual(checked.call_count, 1)
            command = checked.call_args.args[0]
            self.assertEqual(command[command.index("--download-mode") + 1], "refresh")
            self.assertEqual([command[index + 1] for index, word in enumerate(command) if word == "--source-id"],
                             [chosen])
            # The live download happened immediately before the drawn game's complete restore.
            self.assertEqual(dict(seen)[game], 1)
            self.assertEqual([count for restored_game, count in seen if GAMES.index(restored_game) < GAMES.index(game)],
                             [0] * GAMES.index(game))

    def test_a_failed_live_download_fails_the_stage_and_is_reported(self):
        failure = ValueError("live download command failed")
        game, _ = fullrun_stage.sample_draw(SCOPES, SEED)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args, _ = self.patched(root, "sample", checked_effect=failure)
            with mock.patch.object(fullrun_stage.contract, "emit_outputs") as outputs, \
                    mock.patch.object(fullrun_stage, "restore_with_public_supplement") as complete, \
                    self.assertRaises(ValueError) as raised:
                fullrun_stage.refresh(args, [game])
            # The failure is the download's own, nothing was restored after it, and the
            # stage still wrote and published a failed validation.
            self.assertIs(raised.exception, failure)
            complete.assert_not_called()
            report = json.loads((root / "evidence" / "downloader-validation.json").read_text())
            self.assertEqual(report["status"], "failed")
            self.assertIn(game, report["incomplete_games"])
            outputs.assert_called_once_with({"download_validation": "failed"})

    def test_other_modes_make_no_separate_live_download(self):
        for mode in ("cache-first", "refresh"):
            with tempfile.TemporaryDirectory() as temporary:
                args, checked = self.patched(Path(temporary), mode)
                with mock.patch.object(fullrun_stage, "restore_with_public_supplement",
                                       side_effect=ValueError("stop")) as complete, self.assertRaises(ValueError):
                    fullrun_stage.refresh_archives(args, GAMES)
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
