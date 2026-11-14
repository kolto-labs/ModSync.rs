#!/usr/bin/env python3
"""Strict admission of the pinned guide's documented Linux installation choices.

This projects a private converted input; it never changes source guides or archive bytes.
The receipt establishes a recipe, not installation success or cross-lane parity.
"""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib
import uuid

from cache import key_for, link_key

GUIDE_SHA = "67dc5fd8da6abde324c51d32eee3d04687070dd6"
RECIPE = "documented-linux-native-v1"
CLAIM = "complete documented Linux installation choices"
HELPER_SECTION = "Optional Install Assistant - Quicker TSLPatching"
HELPER_HEADING = "Quicker TSLPatching"
HELPER_URL = "https://www.nexusmods.com/kotor/mods/1214?tab=description"
CONFIG = {
    "k1": {
        "guide_sha256": "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
        "source_count": 197,
        "section": "Optional Widescreen & Extended Widescreen Support Mods",
        "inactive_heading": "Remastered Cutscenes",
        "inactive_url": "https://www.nexusmods.com/kotor/mods/1306",
        "selected_heading": "Cutscenes Rescaled",
        "selected_url": "https://deadlystream.com/files/file/2380-k1-cutscenes-rescaled/",
        "files": {"k1rs_30fps_1920x1080.7z": None},
    },
    "k2": {
        "guide_sha256": "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
        "source_count": 151,
        "section": "Enhanced Widescreen Support Mods",
        "inactive_heading": "K2 Cutscenes Rescaled",
        "inactive_url": "https://deadlystream.com/files/file/2503-k2-cutscenes-rescaled/",
        "selected_heading": "K2 Remastered Cutscenes",
        "selected_url": "https://www.nexusmods.com/kotor2/mods/1066/",
        "files": {
            "1920x1080 TSLRCM Patch-1066-1-2-1575040419.zip":
                ("56e87873606765efced68a213dc006ababc93fdfbba86b8918cef38827bf4149", 1095362625),
            "Resolution 1920x1080-1066-1-2-1575040400.zip":
                ("295a42a537f9cb19e329a8bac458720d0858ced929243d361995ffe3aa0cea77", 2331082636),
        },
    },
}
RESTORE_SOURCES = {"local", "stale-local", "artstore", "recovered", "website", "revalidated"}
SHA256 = re.compile(r"[0-9a-f]{64}")


def digest(path):
    checksum = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def identity(value):
    result = str(uuid.UUID(str(value)))
    if result != value:
        raise ValueError("guide selection requires canonical component GUIDs")
    return result


def rules(game):
    cfg = CONFIG[game]
    return [
        {"rule": "linux_unsupported_helper", "section": HELPER_SECTION,
         "heading": HELPER_HEADING, "url": HELPER_URL},
        {"rule": "mutually_exclusive_cutscene_alternative", "section": cfg["section"],
         "heading": cfg["inactive_heading"], "url": cfg["inactive_url"]},
    ]


def walk(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def project_text(text, inactive_guids):
    """Remove only whole canonical root blocks, then compare the complete parsed model."""
    original = tomllib.loads(text)
    keys = [key for key in ("thisMod", "Components") if key in original]
    if len(keys) != 1 or not isinstance(original[keys[0]], list):
        raise ValueError("guide selection root component table ambiguous")
    key = keys[0]
    components = original[key]
    ids = [identity(component.get("Guid")) for component in components]
    inactive = {identity(each) for each in inactive_guids}
    if len(ids) != len(set(ids)) or len(inactive) != 1 or not inactive <= set(ids):
        raise ValueError("guide selection inactive partition invalid")
    forbidden = set()
    for component in components:
        if component["Guid"] in inactive:
            forbidden.update(str(node["Guid"]).lower() for node in walk(component) if "Guid" in node)
            forbidden.update(str(node[field]).lower() for node in walk(component)
                             for field in ("Name", "Heading") if field in node)
    retained = [component for component in components if component["Guid"] not in inactive]
    for component in retained:
        for node in walk(component):
            dependencies = node.get("Dependencies", [])
            if isinstance(dependencies, str):
                dependencies = [dependencies]
            if not isinstance(dependencies, list):
                raise ValueError("guide selection dependency representation unsupported")
            for dependency in dependencies:
                try:
                    reference = str(uuid.UUID(str(dependency)))
                except ValueError:
                    reference = str(dependency).lower()
                if reference in forbidden:
                    raise ValueError("active guide component or option depends on an inactive choice")
    lines = text.splitlines(keepends=True)
    root = re.compile(r"^\[\[" + re.escape(key) + r"\]\]\s*(?:#.*)?$")
    table = re.compile(r"^\[\[?([A-Za-z_][A-Za-z0-9_.]*)\]\]?\s*(?:#.*)?$")
    starts = [index for index, line in enumerate(lines) if root.fullmatch(line.strip())]
    if len(starts) != len(components):
        raise ValueError("guide selection requires canonical root blocks")
    remove = set()
    for position, start in enumerate(starts):
        if ids[position] not in inactive:
            continue
        end = starts[position + 1] if position + 1 < len(starts) else len(lines)
        for index in range(start + 1, end):
            header = table.fullmatch(lines[index].strip())
            if header and not header[1].startswith(key + "."):
                end = index
                break
        remove.update(range(start, end))
    projected = "".join(line for index, line in enumerate(lines) if index not in remove)
    expected = copy.deepcopy(original)
    expected[key] = retained
    if tomllib.loads(projected) != expected:
        raise ValueError("guide selection projection changed retained or global data")
    return projected


def verify_alternative(game, archives, restore_report, archive_map, pins_path):
    """Rehash the exact restored alternative objects; a listing is insufficient."""
    cfg = CONFIG[game]
    for path in (restore_report, archive_map, pins_path):
        if Path(path).is_symlink() or not Path(path).is_file():
            raise ValueError("guide selection archive evidence is missing or symlinked")
    report = json.loads(Path(restore_report).read_text())
    mapping = json.loads(Path(archive_map).read_text())
    url = cfg["selected_url"]
    entries = report.get("entries")
    if report.get("ok") is not True or not isinstance(entries, list):
        raise ValueError("guide selection requires successful archive restore")
    entries = [entry for entry in entries if entry.get("id") == key_for(url)]
    if (len(entries) != 1 or entries[0].get("status") != "restored"
            or entries[0].get("source") not in RESTORE_SOURCES
            or link_key(entries[0].get("url", "")) != link_key(url)):
        raise ValueError("selected guide alternative was not restored with known provenance")
    entry = entries[0]
    files = entry.get("files", [{"file": entry.get("file"), "sha256": entry.get("sha256")}])
    if (not isinstance(files, list) or len(files) != len(cfg["files"])
            or any(not isinstance(item, dict) for item in files)
            or {item.get("file") for item in files} != set(cfg["files"])):
        raise ValueError("selected guide alternative does not contain exactly the reviewed files")
    names = mapping.get(link_key(url)) if isinstance(mapping, dict) else None
    if isinstance(names, str):
        names = [names]
    if not isinstance(names, list) or sorted(names) != sorted(cfg["files"]):
        raise ValueError("selected guide alternative archive map mismatch")
    if game == "k2":
        pin = json.loads(Path(pins_path).read_text()).get("entries", {}).get(key_for(url), {})
        actual_pins = {item.get("archive_name"): (item.get("sha256"), item.get("size"))
                       for item in pin.get("objects", [])}
        if pin.get("url") != url or actual_pins != cfg["files"]:
            raise ValueError("selected guide alternative seed pins changed")
    archives = Path(archives)
    if archives.is_symlink() or not archives.is_dir():
        raise ValueError("guide selection requires a real archive directory")
    archives = archives.resolve(strict=True)
    objects = []
    for item in sorted(files, key=lambda item: item["file"]):
        name = item["file"]
        if Path(name).name != name or name in ("", ".", ".."):
            raise ValueError("selected guide alternative path is unsafe")
        path = archives / name
        if path.is_symlink() or not path.is_file() or path.resolve().parent != archives:
            raise ValueError("selected guide alternative actual archive is missing")
        checksum, size = digest(path), path.stat().st_size
        if not SHA256.fullmatch(str(item.get("sha256", ""))) or checksum != item["sha256"] or size <= 0:
            raise ValueError("selected guide alternative actual bytes do not match restore report")
        pin = cfg["files"][name]
        if pin is not None and (checksum, size) != pin:
            raise ValueError("selected guide alternative actual bytes do not match seed pin")
        if "size" in item and item["size"] != size:
            raise ValueError("selected guide alternative restore size mismatch")
        objects.append({"file": name, "sha256": checksum, "size": size})
    return {"url": url, "restore_entry_id": key_for(url), "source": "verified-restore-report",
            "acquisition_source": entry["source"], "objects": objects}


def project(guide, converted, archives, restore_report, archive_map, pins_path, source_steps, skip_records):
    guide, converted = Path(guide), Path(converted)
    game = guide.parent.name
    if game not in CONFIG or digest(guide) != CONFIG[game]["guide_sha256"]:
        raise ValueError("guide selection requires the exact reviewed guide body")
    revision = subprocess.check_output(["git", "-C", str(guide.parents[2]), "rev-parse", "HEAD"], text=True).strip()
    if revision != GUIDE_SHA:
        raise ValueError("guide selection requires the exact reviewed guide revision")
    original_bytes = converted.read_bytes()
    document = tomllib.loads(original_bytes.decode("utf-8"))
    components = document.get("thisMod", document.get("Components"))
    cfg = CONFIG[game]
    if not isinstance(components, list) or len(components) != cfg["source_count"] or len(source_steps) != len(components):
        raise ValueError("guide selection original source coverage mismatch")
    component_map = []
    for component, (section, heading) in zip(components, source_steps):
        if not isinstance(component, dict) or component.get("Heading", component.get("Name")) != heading:
            raise ValueError("guide selection original heading coverage mismatch")
        component_map.append({"component_guid": identity(component.get("Guid")), "section": section, "heading": heading})
    if len({item["component_guid"] for item in component_map}) != len(component_map):
        raise ValueError("guide selection original component identities are duplicate")
    inactive = []
    for rule in rules(game):
        matches = [item for item in component_map if (item["section"], item["heading"]) == (rule["section"], rule["heading"])]
        guide_only = rule["rule"] == "linux_unsupported_helper"
        if (guide_only and matches) or (not guide_only and len(matches) != 1):
            raise ValueError("guide selection inactive heading mapping is ambiguous")
        inactive.append({**rule, "component_guid": None if guide_only else matches[0]["component_guid"],
                         "scope": "guide-only-not-converted" if guide_only else "converted-component",
                         "evidence": "guide-sha256:" + cfg["guide_sha256"]})
    inactive_ids = {item["component_guid"] for item in inactive if item["component_guid"] is not None}
    retained_skips = []
    removed = set()
    for record in skip_records:
        if record.get("kind") == "dependent_of_deselected" and record.get("caused_by") in inactive_ids:
            raise ValueError("guide selection cannot hide cascaded dependent omissions")
        matches = [item for item in inactive if (item["section"], item["heading"]) ==
                   (record.get("section"), record.get("heading"))]
        if not matches:
            if record.get("component_guid") in inactive_ids:
                raise ValueError("guide selection inactive skip heading mismatch")
            retained_skips.append(record)
            continue
        rule = matches[0]
        if (record.get("source") not in ("restore_report", "archive_unavailable")
                or record.get("severity") != "warning" or record.get("kind") or record.get("caused_by")
                or (record.get("section"), record.get("heading")) != (rule["section"], rule["heading"])
                or {link_key(url) for url in record.get("urls", [])} != {link_key(rule["url"])}
                or record.get("component_guid") != rule["component_guid"]
                or rule["rule"] in removed):
            raise ValueError("guide selection refuses an unexpected inactive skip record")
        removed.add(rule["rule"])
    if removed != {item["rule"] for item in inactive}:
        raise ValueError("guide selection requires exactly the two known inactive restore records")
    selected = verify_alternative(game, archives, restore_report, archive_map, pins_path)
    matches = [item for item in component_map if (item["section"], item["heading"]) == (cfg["section"], cfg["selected_heading"])]
    if len(matches) != 1 or matches[0]["component_guid"] in inactive_ids:
        raise ValueError("guide selection chosen alternative heading mapping is ambiguous")
    selected["component_guid"] = matches[0]["component_guid"]
    selected["section"], selected["heading"] = cfg["section"], cfg["selected_heading"]
    if any(record.get("component_guid") == selected["component_guid"] for record in retained_skips):
        raise ValueError("guide selection chosen alternative is still omitted")
    projected = project_text(original_bytes.decode("utf-8"), inactive_ids).encode("utf-8")
    original_ids = [item["component_guid"] for item in component_map]
    receipt = {"schema_version": 1, "recipe": RECIPE, "claim": CLAIM, "game": game,
               "source_guide_sha": GUIDE_SHA, "source_guide_sha256": cfg["guide_sha256"],
               "original_input_sha256": hashlib.sha256(original_bytes).hexdigest(),
               "admitted_input_sha256": hashlib.sha256(projected).hexdigest(),
               "restore_report_sha256": digest(restore_report), "archive_map_sha256": digest(archive_map),
               "original_component_count": len(original_ids), "admitted_component_count": len(original_ids) - 1,
               "inactive_component_count": 1, "inactive_guide_choice_count": 2,
               "original_guids": original_ids, "admitted_guids": [item for item in original_ids if item not in inactive_ids],
               "original_component_map": component_map, "inactive": inactive, "selected_alternative": selected}
    validate_receipt(receipt)
    if converted.read_bytes() != original_bytes:
        raise ValueError("guide selection input changed during admission")
    converted.write_bytes(projected)
    return receipt, retained_skips


def validate_receipt(receipt):
    """Validate the closed recipe partition and its provenance/checksum evidence."""
    if not isinstance(receipt, dict) or receipt.get("game") not in CONFIG:
        raise ValueError("guide selection receipt game is invalid")
    cfg = CONFIG[receipt["game"]]
    if (type(receipt.get("schema_version")) is not int
            or any(type(receipt.get(field)) is not int for field in (
                "original_component_count", "admitted_component_count", "inactive_component_count", "inactive_guide_choice_count"))
            or receipt.get("schema_version") != 1 or receipt.get("recipe") != RECIPE or receipt.get("claim") != CLAIM
            or receipt.get("source_guide_sha") != GUIDE_SHA or receipt.get("source_guide_sha256") != cfg["guide_sha256"]
            or receipt.get("original_component_count") != cfg["source_count"]
            or receipt.get("admitted_component_count") != cfg["source_count"] - 1
            or receipt.get("inactive_component_count") != 1 or receipt.get("inactive_guide_choice_count") != 2):
        raise ValueError("guide selection recipe identity or counts mismatch")
    for name in ("original_input_sha256", "admitted_input_sha256", "restore_report_sha256", "archive_map_sha256"):
        if not isinstance(receipt.get(name), str) or SHA256.fullmatch(receipt[name]) is None:
            raise ValueError("guide selection source checksum evidence is invalid")
    original, admitted = receipt.get("original_guids"), receipt.get("admitted_guids")
    for values, count in ((original, cfg["source_count"]), (admitted, cfg["source_count"] - 1)):
        if not isinstance(values, list) or len(values) != count or len(set(values)) != count:
            raise ValueError("guide selection component partition has missing or duplicate identities")
        for value in values:
            identity(value)
    component_map = receipt.get("original_component_map")
    if (not isinstance(component_map, list) or len(component_map) != len(original)
            or any(not isinstance(item, dict) for item in component_map)
            or [item.get("component_guid") for item in component_map] != original):
        raise ValueError("guide selection original heading-to-GUID evidence is invalid")
    inactive = receipt.get("inactive")
    if not isinstance(inactive, list) or len(inactive) != 2:
        raise ValueError("guide selection must have exactly two inactive rules")
    for item, rule in zip(inactive, rules(receipt["game"])):
        if not isinstance(item, dict) or any(item.get(key) != value for key, value in rule.items()):
            raise ValueError("guide selection inactive rule is not allowed")
        matches = [row for row in component_map if (row.get("section"), row.get("heading")) == (rule["section"], rule["heading"])]
        guide_only = rule["rule"] == "linux_unsupported_helper"
        if (item.get("evidence") != "guide-sha256:" + cfg["guide_sha256"]
                or (guide_only and (matches or item.get("component_guid") is not None
                                    or item.get("scope") != "guide-only-not-converted"))
                or (not guide_only and (len(matches) != 1
                    or item.get("component_guid") != matches[0]["component_guid"]
                    or item.get("scope") != "converted-component"))):
            raise ValueError("guide selection inactive rule heading evidence mismatch")
    inactive_ids = {item["component_guid"] for item in inactive if item["component_guid"] is not None}
    if len(inactive_ids) != 1 or admitted != [value for value in original if value not in inactive_ids]:
        raise ValueError("guide selection partition is not exhaustive, disjoint and ordered")
    selected = receipt.get("selected_alternative")
    if (not isinstance(selected, dict) or selected.get("url") != cfg["selected_url"]
            or selected.get("source") != "verified-restore-report" or selected.get("acquisition_source") not in RESTORE_SOURCES
            or selected.get("restore_entry_id") != key_for(cfg["selected_url"])
            or selected.get("component_guid") not in admitted
            or (selected.get("section"), selected.get("heading")) != (cfg["section"], cfg["selected_heading"])):
        raise ValueError("guide selection selected alternative identity mismatch")
    matches = [row for row in component_map if (row.get("section"), row.get("heading")) == (cfg["section"], cfg["selected_heading"])]
    if len(matches) != 1 or selected["component_guid"] != matches[0]["component_guid"]:
        raise ValueError("guide selection selected alternative heading mismatch")
    objects = selected.get("objects")
    if (not isinstance(objects, list) or len(objects) != len(cfg["files"])
            or any(not isinstance(item, dict) for item in objects)
            or {item.get("file") for item in objects} != set(cfg["files"])):
        raise ValueError("guide selection selected object evidence mismatch")
    for item in objects:
        if (not isinstance(item.get("sha256"), str) or SHA256.fullmatch(item["sha256"]) is None
                or type(item.get("size")) is not int or item["size"] <= 0):
            raise ValueError("guide selection actual archive checksum/size evidence missing")
        if cfg["files"][item["file"]] is not None and (item["sha256"], item["size"]) != cfg["files"][item["file"]]:
            raise ValueError("guide selection selected object does not match reviewed pin")
    return set(admitted)
