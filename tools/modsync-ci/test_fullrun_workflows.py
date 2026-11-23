"""Validate actual owned workflow graph and pins, not copies of YAML strings."""
import json
import contextlib
import io
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest import mock
import yaml
import fullrun_contract

ROOT = Path(__file__).resolve().parents[2]

class WorkflowGraph(unittest.TestCase):
    def test_every_local_workflow_action_and_python_entrypoint_exists(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            document = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            if not path.name.startswith(("fullrun", "cache-")):
                continue
            for job in document.get("jobs", {}).values():
                uses = job.get("uses", "")
                if uses:
                    self.assertTrue(uses.startswith("./.github/workflows/"), "CI runtime must remain repository owned")
                    self.assertTrue((ROOT / uses[2:]).is_file(), (path.name, uses))
                for step in job.get("steps", []):
                    action = step.get("uses", "")
                    if action.startswith("./"):
                        self.assertTrue((ROOT / action[2:] / "action.yml").is_file())
                    for tool in re.findall(r"tools/modsync-ci/[A-Za-z0-9_-]+\.py", step.get("run", "")):
                        self.assertTrue((ROOT / tool).is_file(), tool)
    def test_matrix_has_no_fail_fast_and_honest_final_collector(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(main["jobs"]["install"]["strategy"]["fail-fast"], "false")
        self.assertEqual(main["jobs"]["install"]["strategy"]["matrix"]["patcher_backend"],
                         ["ody-patcher", "holo-patcher", "mod-packaged"])
        gate = main["jobs"]["result"]
        self.assertIn("always()", gate["if"])
        self.assertEqual(set(gate["needs"]), {"plan","toolkit","production","games","archives","install","comparison","seed","retain_inputs","p2p"})
        collector = next(step for step in gate["steps"] if "NEEDS_JSON" in step.get("env", {}))
        self.assertIn("REQUIRE_COMPARISON", collector["env"])
        self.assertIn("REQUIRE_PRODUCTION", collector["env"])
        producer = main["jobs"]["production"]
        self.assertEqual(producer["uses"], "./.github/workflows/production-build.yml")
        self.assertEqual(producer["with"]["modes"], "${{ needs.plan.outputs.modes }}")
        self.assertIn("!= 'quick'", producer["if"])
        self.assertIn("production", main["jobs"]["install"]["needs"])
    def test_install_admits_skipped_producers_but_not_cancelled_or_invalid_plans(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        expression = main["jobs"]["install"]["if"][3:-2].strip()
        for operation, plan, production, cancelled, expected in (("install","success","success",False,True),
                                                    ("full","success","success",False,True),
                                                    ("install","failure","success",False,False),
                                                    ("install","success","failure",False,False),
                                                    ("install","success","skipped",False,False),
                                                    ("install","success","success",True,False)):
            value = expression.replace("needs.plan.outputs.operation", repr(operation)).replace("needs.plan.result", repr(plan))
            value = value.replace("needs.production.result", repr(production))
            for name, result in (("success",False),("failure",False),("cancelled",cancelled)):
                value = value.replace(name+"()", repr(result))
            value = value.replace("&&", " and ").replace("||", " or ").replace("!", " not ")
            self.assertEqual(eval(value, {"__builtins__": {}}), expected)
    def test_pins_are_valid_and_variants_bind_their_own_subject(self):
        config = json.loads((ROOT / ".github/fullrun.json").read_text())
        self.assertRegex(config["counterpart_sha"], r"^[0-9a-f]{40}$")
        self.assertIn(config["role"], ("engine", "guides"))
    def test_event_defaults_bind_trusted_pr_heads_and_keep_pushes_quick(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        step = next(s for s in main["jobs"]["plan"]["steps"] if s.get("id") == "request")
        defaults = json.loads(main["env"]["FULLRUN_DEFAULTS"])
        for event, requested, write, base, expected, profile, download, writer in (
                ("push", "", False, "feature", "check", "quick", "cache-first", False),
                ("pull_request", "", True, "feature", "full", "full", "cache-first", True),
                ("pull_request", "", True, "main", "full", "exhaustive", "refresh", True),
                ("workflow_dispatch", "full", False, "feature", "full", "minimal", "cache-first", False),
                ("workflow_dispatch", "games", True, "feature", "games", "minimal", "cache-first", True),
                ("workflow_dispatch", "install", True, "feature", "install", "minimal", "cache-first", False),
                ("workflow_dispatch", "check", True, "feature", "check", "minimal", "cache-first", False),
                ("workflow_dispatch", "retain-inputs", True, "feature", "retain-inputs", "minimal", "cache-first", False)):
            repository = "kolto-labs/ModSync.rs"
            context = {"github.event_name": event, "github.repository": repository,
                       "github.event.pull_request.head.repo.full_name": repository,
                       "inputs.operation": requested, "inputs.write_cache": write,
                       "inputs.install_profile": "", "inputs.download_mode": "",
                       "inputs.counterpart_sha": "", "inputs.game": "", "inputs.mode": ""}
            env = {"FULLRUN_DEFAULTS": json.dumps(defaults)}
            for key, expression in step["env"].items():
                value = re.sub(r"\b(?:github|inputs|secrets)(?:\.[A-Za-z_]+)+",
                               lambda match: repr(context[match[0]]), expression[3:-2])
                value = re.sub(r"\bfalse\b", "False", value).replace("&&", " and ").replace("||", " or ")
                result = eval(value, {"__builtins__": {}})
                env[key] = str(result).lower() if type(result) is bool else result
            with self.subTest(event=event, operation=requested, base=base), tempfile.TemporaryDirectory() as root:
                payload = Path(root) / "event.json"
                payload.write_text(json.dumps({"pull_request": {"head": {"sha": "b" * 40,
                    "ref": "feature", "repo": {"full_name": repository}}, "base": {"ref": base}}}
                    if event == "pull_request" else {}))
                env.update(GITHUB_REPOSITORY=repository, GITHUB_SHA="a" * 40,
                           GITHUB_EVENT_NAME=event, GITHUB_EVENT_PATH=str(payload),
                           GITHUB_OUTPUT=str(Path(root) / "outputs"))
                output = Path(root) / "plan.json"
                with mock.patch.dict(os.environ, env, clear=True), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(fullrun_contract.main(["plan", "--config", str(ROOT / ".github/fullrun.json"),
                                                           "--output", str(output)]), 0)
                plan = json.loads(output.read_text())
                self.assertEqual((plan["operation"], plan["cache_writer"], plan["install_profile"],
                                  plan["download_mode"]), (expected, writer, profile, download))
                self.assertEqual(plan["engine_sha"], ("b" if event == "pull_request" else "a") * 40)
                self.assertEqual(plan["release_candidate"], base == "main")
                for key in ("CACHE_ENCRYPTION_KEY", "DIAGNOSTICS_ENCRYPTION_KEY"):
                    self.assertNotIn(key, env, "planning must not gate independent stages on private keys")

    def test_release_policy_cannot_be_downgraded_and_forks_cannot_acquire(self):
        config = json.loads((ROOT / ".github/fullrun.json").read_text())
        for pull in ({"base": {"ref": "main"}}, {"head": {"ref": "release/v1"}},
                     {"labels": [{"name": "release-candidate"}]}):
            policy = fullrun_contract.workflow_policy("pull_request", {"pull_request": pull},
                                                     requested_profile="minimal", requested_download="cache-first")
            self.assertEqual((policy["install_profile"], policy["download_mode"]), ("exhaustive", "refresh"))
        with self.assertRaises(ValueError):
            fullrun_contract.plan(config, config["repository"], "a" * 40, operation="full",
                                  event="pull_request", trusted_pull_request=False)

    def test_quick_push_uses_one_runner_and_new_runs_cancel_prior_same_branch(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(main["concurrency"]["cancel-in-progress"], "true")
        self.assertIn("github.head_ref || github.ref_name", main["concurrency"]["group"])
        self.assertIn("!= 'quick'", main["jobs"]["toolkit"]["if"])
        self.assertIn("!= 'quick'", main["jobs"]["result"]["if"])
        # Narrow non-CI push triggers: site deploy from main and version-tag releases.
        allowed_push = {
            "pages.yml": {"branches": ["main"]},
            "release.yml": {"tags": ["v[0-9]+.[0-9]+.[0-9]+"]},
        }
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            document = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
            if path.name == "fullrun.yml":
                continue
            push = document.get("on", {}).get("push")
            if path.name in allowed_push:
                for key, value in allowed_push[path.name].items():
                    self.assertEqual(push.get(key), value, path.name)
                self.assertNotIn("branches-ignore", push, path.name)
            else:
                self.assertIsNone(push, path.name)
    def test_pr_reads_pinned_k1_auxiliary_cache_without_enabling_publication(self):
        import fullrun_stage
        archive = yaml.load((ROOT / ".github/workflows/cache-archives.yml").read_text(), Loader=yaml.BaseLoader)
        steps = archive["jobs"]["archives"]["steps"]
        key = next(step for step in steps if step.get("id") == "k1-public-key")
        script = "\n".join(key["run"].splitlines()[1:-1])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "outputs"
            env = {"GITHUB_REPOSITORY": "kolto-labs/ModSync.rs", "GITHUB_REF": "refs/pull/95/merge",
                   "REQUESTED_GAMES": '["k1","k2"]', "GITHUB_OUTPUT": str(path)}
            with mock.patch.dict(os.environ, env, clear=True), mock.patch.object(fullrun_stage, "native_k1_scope", return_value=True):
                exec(compile(script, "<archive cache identity>", "exec"), {})
            self.assertIn("applicable=true", path.read_text())
            self.assertIn("key=k1-widescreen-public-v1-", path.read_text())
        ready = next(step for step in steps if step.get("id") == "k1-public-ready")
        self.assertIn("startsWith(github.ref, 'refs/heads/')", ready["if"])
        lane = yaml.load((ROOT / ".github/workflows/fullrun-lane.yml").read_text(), Loader=yaml.BaseLoader)
        restored = [step for step in lane["jobs"]["install"]["steps"]
                    if step.get("name") == "Restore exact K1 auxiliary cache independently of its producer"]
        self.assertEqual(len(restored), 1)
        self.assertIn("actions/cache/restore@", restored[0]["uses"])

    def test_retention_is_reusable_before_default_branch_registration_and_manual_only(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        retention = main["jobs"]["retain_inputs"]
        self.assertIn("github.event_name == 'workflow_dispatch'", retention["if"])
        self.assertEqual(retention["uses"], "./.github/workflows/retain-k1-public-inputs.yml")
        document = yaml.load((ROOT / retention["uses"][2:]).read_text(), Loader=yaml.BaseLoader)
        self.assertIn("workflow_call", document["on"])
        self.assertEqual(document["on"]["workflow_call"]["secrets"]["CACHE_ENCRYPTION_KEY"]["required"], "true")
        self.assertIn("'retention'", main["concurrency"]["group"])
        config = json.loads((ROOT / ".github/fullrun.json").read_text())
        with self.assertRaises(ValueError):
            fullrun_contract.plan(config, config["repository"], "a" * 40, operation="retain-inputs", event="push")
        self.assertEqual(fullrun_contract.aggregate("retain-inputs", {name: {"result": "success"}
            for name in ("plan", "toolkit", "retain_inputs")})["status"], "passed")
        with self.assertRaises(ValueError):
            fullrun_contract.aggregate("retain-inputs", {name: {"result": "success"} for name in ("plan", "toolkit")})

    def test_each_private_stage_checks_its_own_credentials_before_any_acquisition(self):
        for filename, job, stage, keys in (
                ("cache-game.yml", "game", "games", {"CACHE_ENCRYPTION_KEY"}),
                ("cache-archives.yml", "archives", "archives", {"DIAGNOSTICS_ENCRYPTION_KEY"}),
                ("fullrun-lane.yml", "install", "install", {"CACHE_ENCRYPTION_KEY", "DIAGNOSTICS_ENCRYPTION_KEY"}),
                ("cache-seed.yml", "seed", "seed", {"DIAGNOSTICS_ENCRYPTION_KEY"})):
            with self.subTest(stage=stage):
                document = yaml.load((ROOT / ".github/workflows" / filename).read_text(), Loader=yaml.BaseLoader)
                checkout, gate = document["jobs"][job]["steps"][:2]
                self.assertTrue(checkout["uses"].startswith("actions/checkout@"))
                self.assertEqual(gate["run"], "python tools/modsync-ci/fullrun_contract.py check-credentials --stage " + stage)
                self.assertEqual(set(gate["env"]), keys)
                for key in keys:
                    self.assertEqual(gate["env"][key], "${{ secrets." + key + " }}")
    def test_synthetic_cli_and_gui_workflows_are_removed(self):
        for path in (ROOT / ".github/workflows").glob("synthetic-*.yml"):
            self.assertFalse(path.name.startswith(("synthetic-full-cli-", "synthetic-gui-")), path.name)
    def test_seed_digest_can_be_a_variable_but_urls_and_keys_stay_private(self):
        doc = yaml.load((ROOT / ".github/workflows/cache-game.yml").read_text(), Loader=yaml.BaseLoader)
        seed = next(step for step in doc["jobs"]["game"]["steps"] if step.get("id") == "seed")
        for game in ("k1", "k2"):
            for item in ("URL", "KEY", "SHA256"):
                variable = f"GAME_SEED_{item}_{game.upper()}"
                expression = seed["env"][variable]
                self.assertIn(f"matrix.game == '{game}'", expression)
                self.assertIn(f"secrets.{variable}", expression)
                self.assertEqual(f"vars.{variable}" in expression, item == "SHA256")
        self.assertIn("A pinned SHA-256 is required", seed["run"])
    def test_archive_provider_credentials_are_only_scoped_to_refresh(self):
        doc = yaml.load((ROOT / ".github/workflows/cache-archives.yml").read_text(), Loader=yaml.BaseLoader)
        archive_job = doc["jobs"]["archives"]
        self.assertFalse(archive_job.get("env"))
        for step in archive_job["steps"]:
            keys = set(step.get("env", {}))
            self.assertEqual("ARTSTORE_TOKEN" in keys, step.get("id") == "refresh")
            self.assertEqual("RCLONE_CONFIG_MODCACHE_SECRET_ACCESS_KEY" in keys, step.get("id") == "refresh")
        for file, job in (("cache-archives.yml", "archives"), ("fullrun-lane.yml", "install"),
                          ("cache-seed.yml", "seed")):
            doc = yaml.load((ROOT / ".github/workflows" / file).read_text(), Loader=yaml.BaseLoader)
            step, = (step for step in doc["jobs"][job]["steps"]
                     if "RCLONE_CONFIG_MODCACHE_ENDPOINT" in step.get("env", {}))
            self.assertEqual(step["env"]["RCLONE_CONFIG_MODCACHE_ENDPOINT"],
                             "${{ secrets.MOD_CACHE_S3_ENDPOINT || vars.MOD_CACHE_S3_ENDPOINT }}")
    def test_evidence_upload_requires_a_successful_new_seal(self):
        action = yaml.load((ROOT / ".github/actions/retain-fullrun-evidence/action.yml").read_text(), Loader=yaml.BaseLoader)
        seal, upload = action["runs"]["steps"]
        self.assertEqual(seal["id"], "seal")
        self.assertIn("mktemp -d", seal["run"])
        self.assertIn("steps.seal.outcome == 'success'", upload["if"])
        self.assertEqual(upload["with"]["path"], "${{ steps.seal.outputs.path }}")
    def test_no_test_target_compilation_was_added_to_fullrun_workflows(self):
        for path in (ROOT / ".github/workflows").glob("fullrun*.yml"):
            text = path.read_text()
            self.assertNotRegex(text, r"cargo(?: \+\S+)? (?:test|nextest)\b|--all-targets")

if __name__ == "__main__":
    unittest.main()
