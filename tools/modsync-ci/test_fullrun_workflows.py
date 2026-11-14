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
        gate = main["jobs"]["result"]
        self.assertIn("always()", gate["if"])
        self.assertEqual(set(gate["needs"]), {"plan","toolkit","games","archives","install","seed","p2p"})
    def test_install_admits_skipped_producers_but_not_cancelled_or_invalid_plans(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        expression = main["jobs"]["install"]["if"][3:-2].strip()
        for operation, plan, cancelled, expected in (("install","success",False,True),
                                                    ("full","success",False,True),
                                                    ("install","failure",False,False),
                                                    ("install","success",True,False)):
            value = expression.replace("needs.plan.outputs.operation", repr(operation)).replace("needs.plan.result", repr(plan))
            for name, result in (("success",False),("failure",False),("cancelled",cancelled)):
                value = value.replace(name+"()", repr(result))
            value = value.replace("&&", " and ").replace("||", " or ").replace("!", " not ")
            self.assertEqual(eval(value, {"__builtins__": {}}), expected)
    def test_pins_are_valid_and_variants_bind_their_own_subject(self):
        config = json.loads((ROOT / ".github/fullrun.json").read_text())
        self.assertRegex(config["counterpart_sha"], r"^[0-9a-f]{40}$")
        self.assertIn(config["role"], ("engine", "guides"))
    def test_actual_event_expressions_admit_full_pushes_and_keep_prs_credential_free(self):
        main = yaml.load((ROOT / ".github/workflows/fullrun.yml").read_text(), Loader=yaml.BaseLoader)
        step = next(s for s in main["jobs"]["plan"]["steps"] if s.get("id") == "request")
        for event, operation, write, expected, writer in (
                ("push", "", False, "full", True),
                ("pull_request", "full", True, "check", False),
                ("pull_request_target", "full", True, "check", False),
                ("workflow_run", "full", True, "check", False),
                ("workflow_dispatch", "full", False, "full", False),
                ("workflow_dispatch", "games", True, "games", True),
                ("workflow_dispatch", "install", True, "install", False),
                ("workflow_dispatch", "check", True, "check", False)):
            context = {"github.event_name": event, "inputs.operation": operation,
                       "inputs.write_cache": write, "inputs.counterpart_sha": "",
                       "inputs.game": "", "inputs.mode": "",
                       "secrets.CACHE_ENCRYPTION_KEY": "test-cache-key",
                       "secrets.DIAGNOSTICS_ENCRYPTION_KEY": "test-evidence-key"}
            env = {}
            for key, expression in step["env"].items():
                value = re.sub(r"\b(?:github|inputs|secrets)\.[A-Za-z_]+",
                               lambda match: repr(context[match[0]]), expression[3:-2])
                value = re.sub(r"\bfalse\b", "False", value).replace("&&", " and ").replace("||", " or ")
                result = eval(value, {"__builtins__": {}})
                env[key] = str(result).lower() if type(result) is bool else result
            with self.subTest(event=event, operation=operation, write=write), tempfile.TemporaryDirectory() as root:
                env.update(GITHUB_REPOSITORY="kolto-labs/ModSync.rs", GITHUB_SHA="a" * 40,
                           GITHUB_EVENT_NAME=event, GITHUB_OUTPUT=str(Path(root) / "outputs"))
                output = Path(root) / "plan.json"
                with mock.patch.dict(os.environ, env), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(fullrun_contract.main(["plan", "--config", str(ROOT / ".github/fullrun.json"),
                                                           "--output", str(output)]), 0)
                plan = json.loads(output.read_text())
                self.assertEqual((plan["operation"], plan["cache_writer"]), (expected, writer))
                for key in ("CACHE_ENCRYPTION_KEY", "DIAGNOSTICS_ENCRYPTION_KEY"):
                    self.assertEqual(bool(env[key]), event in ("push", "workflow_dispatch"))
                if event == "push":
                    for key in ("CACHE_ENCRYPTION_KEY", "DIAGNOSTICS_ENCRYPTION_KEY"):
                        missing = dict(env, **{key: ""})
                        rejected = Path(root) / (key + ".json")
                        with mock.patch.dict(os.environ, missing), contextlib.redirect_stderr(io.StringIO()):
                            self.assertEqual(fullrun_contract.main(["plan", "--config", str(ROOT / ".github/fullrun.json"),
                                                                   "--output", str(rejected)]), 1)
                        self.assertFalse(rejected.exists(), "missing keys must not downgrade full to check")
    def test_synthetic_cli_and_gui_workflows_are_removed(self):
        for path in (ROOT / ".github/workflows").glob("synthetic-*.yml"):
            self.assertFalse(path.name.startswith(("synthetic-full-cli-", "synthetic-gui-")), path.name)
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
