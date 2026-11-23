"""Exact source-guide cache selections; these are not operator seed pins."""
import copy
import hashlib
import json
from pathlib import Path

GUIDE_SHA256 = "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f"
PLAN_SHA256 = "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258"
SUPPLEMENTAL_SHA256 = "92fad6add3641d53416effbf8a1360c51fb9b64fc340273c2baa33b06672ee74"
PROVENANCE = "reviewed-original-guide-artifact"
GUIDES = {
    "k1": ("44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
           "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74"),
    "k2": (GUIDE_SHA256, PLAN_SHA256),
}
SPECS = {
    "07d93c7cf8ada06f7edc81d429ff76d10fb51eaa270de815ddcb30d5a264d8a8": {
        "url": "https://deadlystream.com/files/file/2184-trask-ulgo-without-tutorials/",
        "guid": "14d77038-49c0-882b-83a1-0a424efa0f60", "index": 124, "game": "k1",
        "source_kind": "original-pinned-plan", "step_id": "step-125",
        "heading": "### Tutorial Remover",
        "objects": [{"archive_name": "Trask Ulgo Without Tutorials.7z",
                     "sha256": "b9e4b5f4934594400cbc4873cbba12b78444174d5239b92f0049d746f31207cc",
                     "size": 453590, "extension": "7z"}],
    },
    "b5e8bd00abb1552134512314ddb129643e2462f64ba6f09ef966d3207ebbb8ea": {
        "url": "https://deadlystream.com/files/file/2577-improved-ls-enlightenment-ds-corruption-feats/",
        "guid": "e015887a-be0f-86d1-830e-92d82c8fc46c", "index": 135, "game": "k2",
        "source_kind": "original-pinned-plan", "step_id": "step-136",
        "heading": "### Improved Enlightenment/Corruption",
        "objects": [{"archive_name": "Improved Enlightenment and Corruption Feats.zip",
                     "sha256": "0c6db43f218d6d5bf73b1be8e3f7dc469254f3168920b6af6fa8513313617597",
                     "size": 8802, "extension": "zip"}],
    },
    "e65b3e0b9fc29a47556948301b140691814956b4633722eb1d18d5fa95621ef0": {
        "url": "https://deadlystream.com/files/file/1138-tsl-main-menu-model-fix-for-widescreen/",
        "guid": "14d721c3-67f9-885b-9736-04282d41bed0", "index": 145, "game": "k2",
        "source_kind": "authored-supplemental", "step_id": None,
        "objects": [{"archive_name": "[TSL]_Main_Menu_Widescreen_Fix_v1.2.7z",
                     "sha256": "ebd3ca1549cf82b961b535ae2180ff78160ceac46cf8d417b42b1143c94c63b5",
                     "size": 2147838, "extension": "7z"},
                    {"archive_name": "Updated_TSLRCM_Logo_v1.8.6.7z",
                     "sha256": "7274ad04de521caf4e8819e431a90612325308336136db54f72ed70af93dd272",
                     "size": 124981, "extension": "7z"}],
    },
}


def _objects(entry):
    if not isinstance(entry, dict):
        return []
    return entry.get("objects") or ([entry] if entry.get("sha256") else [])


def selected_entries(items, guides, manifest_entries):
    """Override these sources only when their complete original guide is supplied."""
    scoped = {game: set() for game in GUIDES}
    for guide in guides:
        path = Path(guide)
        if path.is_file() and not path.is_symlink() and path.stat().st_size <= 2 * 1024 * 1024:
            checksum = hashlib.sha256(path.read_bytes()).hexdigest()
            for game, (guide_sha, _) in GUIDES.items():
                if checksum == guide_sha:
                    scoped[game].add(str(guide))
    if not any(scoped.values()):
        return {}
    repo = Path(__file__).resolve().parents[2]
    for game, paths in scoped.items():
        if not paths:
            continue
        guide_sha, plan_sha = GUIDES[game]
        body = (repo / "tools/modsync-ci/fixtures/ci" / (game + "-manual-plan.json")).read_bytes()
        if hashlib.sha256(body).hexdigest() != plan_sha:
            raise ValueError("original guide input plan identity changed")
        plan = json.loads(body)
        for spec in SPECS.values():
            if spec["game"] != game or spec["source_kind"] != "original-pinned-plan":
                continue
            pin = spec["objects"][0]
            step = plan["steps"][spec["index"]]
            if (plan.get("guide_sha256") != guide_sha or step.get("id") != spec["step_id"]
                    or step.get("source_heading") != spec["heading"]
                    or step["operations"][0] != {"type": "extract", "source": "{archives}/" + pin["archive_name"],
                                                 "destination": "{extract}/" + spec["step_id"] + "/1"}
                    or plan["archive_sha256"].get("{archives}/" + pin["archive_name"]) != pin["sha256"]):
                raise ValueError("original guide input source program changed")
    if scoped["k2"]:
        supplemental_path = repo / "src/archive_generation/source_plan_namespace/supplemental_authority.json"
        supplemental_body = supplemental_path.read_bytes()
        if hashlib.sha256(supplemental_body).hexdigest() != SUPPLEMENTAL_SHA256:
            raise ValueError("supplemental guide input identity changed")
        supplemental = json.loads(supplemental_body)
        menu = next(spec for spec in SPECS.values() if spec["source_kind"] == "authored-supplemental")
        row = next(row for row in supplemental["rows"] if row["guid"] == menu["guid"])
        if (row["game"] != "k2" or row["index"] != menu["index"]
                or row["source_kind"] != "authored-supplemental"
                or {p["filename"]: p["sha256"] for p in row["inputs"]}
                != {p["archive_name"]: p["sha256"] for p in menu["objects"]}):
            raise ValueError("supplemental guide input source pins changed")
    result = {}
    for item in items:
        spec = SPECS.get(item.get("id"))
        if (spec is None or item.get("url") != spec["url"] or item.get("provider") != "deadlystream"
                or not scoped[spec["game"]].intersection(map(str, item.get("guides", [])))):
            continue
        objects = [{**pin, "status": "required-input", "checked_at": 0, "metadata_unknown": True,
                    "evidence": PROVENANCE,
                    **({"listed_name": pin["archive_name"], "row": None} if len(spec["objects"]) > 1 else {})}
                   for pin in spec["objects"]]
        entry = objects[0] if len(objects) == 1 else {
            "objects": objects, "size": sum(pin["size"] for pin in objects),
            "status": "required-input", "checked_at": 0, "metadata_unknown": True, "evidence": PROVENANCE}
        old = manifest_entries.get(item["id"]) or {}
        if not isinstance(old, dict):
            old = {}
        # Cache both publisher inputs; the compiled supplemental install choice
        # independently keeps the optional logo inactive. No original step is invented.
        entry.update(provenance=PROVENANCE,
                     source_association={"id": item["id"], "url": spec["url"], "provider": "deadlystream"},
                     reviewed_original_guide_input={
                         "guide_sha256": GUIDES[spec["game"]][0], "plan_sha256": GUIDES[spec["game"]][1],
                         "component_guid": spec["guid"], "component_index": spec["index"],
                         "source_kind": spec["source_kind"], "step_id": spec["step_id"]},
                     previous_manifest_objects=[
                         {key: stored[key] for key in ("archive_name", "sha256", "size") if key in stored}
                         for stored in _objects(old)])
        if isinstance(old.get("legacy_manifest_association"), dict):
            entry["legacy_manifest_association"] = copy.deepcopy(old["legacy_manifest_association"])
        if old.get("evidence") == PROVENANCE and isinstance(old.get("previous_manifest_objects"), list):
            entry["previous_manifest_objects"] = copy.deepcopy(old["previous_manifest_objects"])
        result[item["id"]] = entry
    return result


def required_matches(item, selected, candidate):
    """A newer, different feature set cannot replace the selected exact source inputs."""
    if selected.get("evidence") != PROVENANCE:
        return True
    spec = SPECS.get(item.get("id"))
    if spec is None or item.get("provider") != "deadlystream" or item.get("url") != spec["url"]:
        return False
    objects = _objects(candidate)
    if (not isinstance(objects, list) or any(not isinstance(p, dict)
            or not isinstance(p.get("archive_name"), str) or not isinstance(p.get("sha256"), str)
            or not isinstance(p.get("size"), int) or isinstance(p.get("size"), bool)
            or not isinstance(p.get("extension"), str) for p in objects)):
        return False
    return (len(objects) == len(spec["objects"])
            and sorted((p["archive_name"], p["sha256"], p["size"], p["extension"]) for p in objects)
            == sorted((p["archive_name"], p["sha256"], p["size"], p["extension"])
                      for p in spec["objects"]))


def retain_provenance(item, selected, candidate):
    if selected.get("evidence") != PROVENANCE:
        return candidate
    if not required_matches(item, selected, candidate):
        raise ValueError("provider input differs from the required original guide version")
    result = copy.deepcopy(candidate)
    for key in ("source_association", "reviewed_original_guide_input", "previous_manifest_objects", "provenance"):
        result[key] = copy.deepcopy(selected[key])
    if isinstance(selected.get("legacy_manifest_association"), dict):
        result["legacy_manifest_association"] = copy.deepcopy(selected["legacy_manifest_association"])
    result["evidence"] = PROVENANCE
    return result


def authenticated_legacy_menu_association(item, entry, selected):
    """Only the authenticated, exact logo-only legacy descriptor may be completed.

    Authentication remains the archive-manifest recovery caller's responsibility.
    This does not assert that either desired body has been fetched or verified.
    """
    identity = "e65b3e0b9fc29a47556948301b140691814956b4633722eb1d18d5fa95621ef0"
    spec = SPECS[identity]
    if (item.get("id") != identity or item.get("url") != spec["url"]
            or item.get("provider") != "deadlystream" or selected.get("evidence") != PROVENANCE
            or not required_matches(item, selected, selected)):
        return False
    objects = _objects(entry)
    return (len(objects) == 1 and isinstance(objects[0], dict)
            and all(objects[0].get(key) == value for key, value in spec["objects"][1].items()))
