import hashlib
import unittest

import patcher_comparison as pc


MODE = {"readonly": False, "unix": 0o644}


def tree(**files):
    return {"rootMode": MODE, "entries": {
        name: {"kind": "file", "mode": MODE, "sha256": hashlib.sha256(body.encode()).hexdigest(),
               "size": len(body)} for name, body in files.items()}}


PATCHED = "aaaa-patched"
PLAIN = "bbbb-plain"
OPERATIONS = [[PATCHED, 0]]


def lanes(overrides=None):
    """Six cells with identical boundaries; overrides[(backend, mode)][key] replaces one tree."""
    base = {"sessionBaseline": tree(),
            f"component/{PATCHED}/1": tree(**{"override/a.2da": "patched"}),
            f"component/{PLAIN}/1": tree(**{"override/a.2da": "patched", "override/b.tga": "plain"})}
    cells = {}
    for backend in pc.BACKENDS:
        for mode in pc.MODES:
            cells[(backend, mode)] = {key: dict(base, **(overrides or {}).get((backend, mode), {}))[key]
                                      for key in base}
    return cells


def run(cells, operations=OPERATIONS):
    return pc.judge(cells, operations, lambda handle: handle)


class Pairs(unittest.TestCase):
    def test_every_lane_is_held_against_the_packaged_lane_of_its_route(self):
        self.assertEqual(pc.comparison_pairs(), [
            (("mod-packaged", "cli"), ("ody-patcher", "cli")),
            (("mod-packaged", "cli"), ("holo-patcher", "cli")),
            (("mod-packaged", "gui"), ("ody-patcher", "gui")),
            (("mod-packaged", "gui"), ("holo-patcher", "gui")),
            (("mod-packaged", "cli"), ("mod-packaged", "gui")),
        ])

    def test_only_preparation_and_patcher_components_are_compared(self):
        keys = ["sessionBaseline", "preparation", f"component/{PATCHED.upper()}/1", f"component/{PLAIN}/1"]
        self.assertEqual(pc.patcher_boundaries(keys, OPERATIONS),
                         ["preparation", f"component/{PATCHED.upper()}/1"])


class Judge(unittest.TestCase):
    def test_identical_lanes_pass(self):
        comparisons, wrong = run(lanes())
        self.assertEqual(wrong, [])
        self.assertEqual([b["key"] for b in comparisons[0]["boundaries"]],
                         [f"component/{PATCHED}/1", "final"])

    def test_a_lane_that_differs_from_the_packaged_patcher_is_named_wrong(self):
        bad = {("holo-patcher", "gui"): {f"component/{PATCHED}/1": tree(**{"override/a.2da": "drift"}),
                                         f"component/{PLAIN}/1": tree(**{"override/a.2da": "drift",
                                                                         "override/b.tga": "plain"})}}
        _, wrong = run(lanes(bad))
        self.assertEqual(len(wrong), 1)
        self.assertEqual(wrong[0]["lane"], ["holo-patcher", "gui"])
        self.assertEqual(wrong[0]["reference"], ["mod-packaged", "gui"])
        self.assertEqual(wrong[0]["first_differing_boundary"], f"component/{PATCHED}/1")

    def test_when_the_packaged_lane_differs_every_other_lane_on_its_route_is_flagged(self):
        # The reference is never blamed: the lanes compared with it are.
        bad = {("mod-packaged", "cli"): {f"component/{PATCHED}/1": tree(**{"override/a.2da": "other"})}}
        _, wrong = run(lanes(bad))
        self.assertEqual(sorted(tuple(w["lane"]) for w in wrong),
                         [("holo-patcher", "cli"), ("mod-packaged", "gui"), ("ody-patcher", "cli")])

    def test_a_difference_inside_a_component_without_a_patcher_is_not_compared_on_its_own(self):
        # Only the plain component's own boundary differs, and the final tree is the
        # last boundary, so the final comparison still catches it.
        bad = {("ody-patcher", "cli"): {f"component/{PLAIN}/1": tree(**{"override/a.2da": "patched",
                                                                        "override/b.tga": "other"})}}
        comparisons, wrong = run(lanes(bad))
        self.assertEqual([w["first_differing_boundary"] for w in wrong], ["final"])
        keys = [b["key"] for b in comparisons[0]["boundaries"]]
        self.assertNotIn(f"component/{PLAIN}/1", keys)

    def test_a_guide_without_patcher_operations_compares_only_the_final_tree(self):
        comparisons, wrong = run(lanes(), operations=[])
        self.assertEqual(wrong, [])
        self.assertEqual([b["key"] for b in comparisons[0]["boundaries"]], ["final"])


if __name__ == "__main__":
    unittest.main()
