r"""Write a build's instruction overlay from the old kotor-mod-manager installer.

The old app (https://github.com/ChristopherVR/kotor-mod-manager, Python) had its
own per-mod install logic: archive layout detection (installer/detector.py),
build-guide directives read from the guide's prose (installer/build_directives.py)
and hand-checked rules per mod (installer/build_overrides.py). It installed 172
of the 173 KOTOR 1 Full Build mods it was tested with.

This script runs that installer, unchanged, over a whole build against a
throwaway copy of the game, with the archives ModSync downloaded. It does not
re-implement any of its rules. Instead it records what the old installer did to
the game folder, file by file, and writes those actions as ModSync instructions:

* every archive it unpacked becomes an Extract step,
* every file it copied into the game becomes a Copy step (from the same file
  inside the unpacked archive, to the same game folder),
* every file it deleted or renamed in the game (or in the mod's own folder,
  before a patcher ran) becomes a Delete or Rename step,
* every patcher it would have run becomes a Patcher step with the same
  tslpatchdata folder and the same namespace (option) index,
* its end-of-install duplicate texture cleanup (delete a .tpc that has a .tga
  or .dds twin in Override) becomes the matching DelDuplicate step.

Patchers are not run (as in the old app's own fake-game test mode), so nothing
is patched; their choice of folder and namespace is recorded instead. Mods the
old installer left for the player to do by hand, failed, skipped, or changed in
a way this script cannot express exactly, get no overlay entry and keep the
build's own (drafted) instructions.

The overlay is TOML in ModSync's own guide format. The desktop app merges it
into the build with the engine's merge support when the build loads, so these
mods' instructions are authored rather than drafted.

Usage (Python 3.11+ with the old app's own dependencies; run it from a folder that
holds no downloaded files):

  python overlay_from_old_installer.py \
      --old-app D:\Development\kotor-mod-installer \
      --build k1_full \
      --guide guide.toml            (modsync convert of the build's Markdown) \
      --mods D:\kmi_test\mods       (ModSync's mods folder with the downloads) \
      --downloads report.json ...   (fetch-downloads JSON reports for --mods) \
      --game-copy D:\work\game      (a THROWAWAY copy of the game; it is modded) \
      --work D:\work                (scratch; gets hard links and unpacked mods) \
      --old-builds old_builds.json  (optional snapshot of the old scraper) \
      --only guids.txt              (optional: only write these components) \
      --out k1_full.toml

The game copy and the work folder are written to; the mods folder is only
read (archives are hard-linked into --work, which must be on the same drive).
"""
import argparse
import dataclasses
import json
import ntpath
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import tomllib
from collections import OrderedDict
from pathlib import Path

ARCHIVES = (".zip", ".7z", ".rar", ".exe")


def norm_url(url: str) -> str:
    u = url.strip().lower()
    u = re.sub(r"^https?://(www\.)?", "", u).rstrip("/")
    m = re.match(r"deadlystream\.com/files/file/(\d+)", u)
    return f"ds:{m.group(1)}" if m else u


def long(p) -> Path:
    """Drop the Windows long-path prefix so paths compare normally."""
    s = os.fspath(p)
    if s.startswith("\\\\?\\"):
        s = s[4:]
    return Path(os.path.abspath(s))


def disk(p: Path) -> Path:
    """The same path with the long-path prefix, for reading deep mod folders."""
    return Path("\\\\?\\" + str(p)) if os.name == "nt" else p


def under(p: Path, root: Path) -> bool:
    try:
        return os.path.normcase(str(p)).startswith(os.path.normcase(str(root)) + os.sep)
    except (TypeError, ValueError):
        return False


def rel(p: Path, root: Path) -> str:
    return str(p)[len(str(root)) + 1:]


# --------------------------------------------------------------------------
# Inputs
# --------------------------------------------------------------------------

def downloads_by_component(reports: list[str], mods: Path) -> dict:
    """{guid: {registry url: [file names]}} from fetch-downloads reports."""
    found: dict = {}
    for report in reports:
        data = json.loads(Path(report).read_text(encoding="utf-8"))
        r = data.get("report") or {}
        rows = [o for o in r.get("outcomes", []) if o.get("status") == "completed"]
        rows += [s for s in r.get("skipped", []) if s.get("reason") == "alreadyPresent"]
        for row in rows:
            if not row.get("path"):
                continue
            name = ntpath.basename(row["path"])
            if not (mods / name).is_file():
                continue
            names = found.setdefault(row["componentGuid"], {}).setdefault(row["registryUrl"], [])
            if name not in names:
                names.append(name)
    return found


def old_build_mods(args):
    from scraper.build_scraper import BuildMod, scrape_build
    if args.old_builds:
        data = json.loads(Path(args.old_builds).read_text(encoding="utf-8"))
        key = {"k1_spoiler_free": "k1_spoilerfree", "k2_spoiler_free": "k2_spoilerfree"}.get(
            args.build, args.build)
        return [BuildMod(**m) for m in data[key]]
    return scrape_build(args.build)


def match_components(old_mods, guide) -> dict:
    """{id(old mod): component} by the mod's main link."""
    by_link = {}
    for c in guide["thisMod"]:
        for link in c.get("ResourceRegistry", {}):
            by_link.setdefault(norm_url(link), c)
    out = {}
    for m in old_mods:
        main = m.ds_url if m.source_host == "deadlystream" else m.url
        c = by_link.get(norm_url(main or ""))
        if c is not None:
            out[id(m)] = c
    return out


def stage_cache(old_mods, matched, files, mods: Path, dl: Path, manifest_name: str):
    """Hard-link each mod's downloads where the old app keeps its cache."""
    from installer.download_paths import download_folder_name
    for m in old_mods:
        c = matched.get(id(m))
        if c is None:
            continue
        links = files.get(c["Guid"], {})
        dest = dl / download_folder_name(m.file_id, m.slug)
        placed = []
        sources = [m.url, *m.patch_urls]
        main = m.ds_url if m.source_host == "deadlystream" else m.url
        groups = [(norm_url(main), dest)] + [
            (norm_url(u), dest / f"_linked_patch_{i}") for i, u in enumerate(m.patch_urls, 1)]
        for key, folder in groups:
            for link, names in links.items():
                if norm_url(link) != key:
                    continue
                for name in names:
                    folder.mkdir(parents=True, exist_ok=True)
                    target = folder / name
                    if not target.exists():
                        os.link(mods / name, target)
                    placed.append(target.relative_to(dest).as_posix())
        if placed:
            (dest / manifest_name).write_text(
                json.dumps({"files": placed, "sources": sources}), encoding="utf-8")


# --------------------------------------------------------------------------
# Recording what the old installer does
# --------------------------------------------------------------------------

class Recorder:
    def __init__(self, game: Path, dl: Path):
        self.game, self.dl = game, dl
        self.ops: dict = OrderedDict()     # key -> list of ops
        self.unsupported: dict = {}        # key -> reasons
        self.local = threading.local()
        self.extracting = 0

    def key(self):
        return getattr(self.local, "key", None)

    def add(self, *op):
        k = self.key()
        if k is not None:
            self.ops.setdefault(k, []).append(op)

    def refuse(self, why: str):
        k = self.key()
        if k is not None:
            self.unsupported.setdefault(k, []).append(why)

    def install(self, plmod, extractor, patcher_strategy):
        rec = self
        real_extract = extractor.extract

        def extract(archive, dest=None):
            rec.extracting += 1
            try:
                out = real_extract(archive, dest)
            finally:
                rec.extracting -= 1
            rec.add("extract", long(archive), long(out))
            return out
        extractor.extract = extract
        plmod.extract = extract

        def nested(real, *a, **kw):
            # Only the outermost file operation is recorded (copy2 calls
            # copyfile, move calls rename or copy2 and unlink).
            rec.local.depth = getattr(rec.local, "depth", 0) + 1
            try:
                return real(*a, **kw), rec.local.depth > 1
            finally:
                rec.local.depth -= 1

        def copier(real):
            def wrapped(src, dst, *a, **kw):
                s, d = long(src), long(dst)
                out, inner = nested(real, src, dst, *a, **kw)
                if inner:
                    return out
                if under(d, rec.game):
                    if os.path.isdir(d):
                        d = d / s.name
                    if under(s, rec.game):
                        # The old app backed up dialog.tlk as dialog.tlk.bak;
                        # ModSync keeps its own restore point instead.
                        if d.name.lower() != s.name.lower() + ".bak":
                            rec.refuse(f"copies inside the game folder ({s.name})")
                    else:
                        rec.add("copy", s, d)
                return out
            return wrapped
        for name in ("copy2", "copy", "copyfile"):
            setattr(shutil, name, copier(getattr(shutil, name)))

        def mover(real):
            def wrapped(src, dst, *a, **kw):
                s, d = long(src), long(dst)
                out, inner = nested(real, src, dst, *a, **kw)
                if inner:
                    return out
                if under(d, rec.game) and under(s, rec.game):
                    rec.add("rename", s, d)
                elif under(d, rec.game):
                    rec.add("copy", s, d)
                elif under(s, rec.game):
                    rec.refuse(f"moves a file out of the game folder ({s.name})")
                return out
            return wrapped
        os.replace = mover(os.replace)
        os.rename = mover(os.rename)
        shutil.move = mover(shutil.move)

        def remover(real):
            def wrapped(path, *a, **kw):
                p = long(path)
                out, inner = nested(real, path, *a, **kw)
                if inner:
                    return out
                if under(p, rec.game):
                    rec.add("delete", p)
                elif under(p, rec.dl) and not rec.extracting and rec.key() is not None:
                    rec.add("delete_mod", p)
                return out
            return wrapped
        os.remove = remover(os.remove)
        os.unlink = remover(os.unlink)

        def tree_remover(real):
            def wrapped(path, *a, **kw):
                if under(long(path), rec.game):
                    rec.refuse("removes a folder from the game")
                return real(path, *a, **kw)
            return wrapped
        shutil.rmtree = tree_remover(shutil.rmtree)

        # Patchers are recorded, never run.
        from installer.patcher_strategy import _find_tslpatchdata, resolve_namespace_index

        class Dry:
            strategy = "recorded"

        def cascade(mod_root=None, exe=None, game_dir=None, option_hint="",
                    directives=None, cb=None, on_waiting=None, allow_manual=True, **kw):
            td = _find_tslpatchdata(mod_root, exe)
            if td is None:
                raise patcher_strategy.PatcherError(f"no tslpatchdata under {mod_root}")
            index = resolve_namespace_index(td, option_hint, cb, directives)
            rec.add("patch", long(td), long(exe) if exe else None, index)
            return Dry()

        def holo(exe, game_dir, tslpatchdata, namespace_index=0, cb=None, **kw):
            rec.add("patch", long(tslpatchdata), long(exe) if exe else None, namespace_index)

        plmod.run_tslpatcher_cascade = cascade
        plmod.run_holopatcher = holo

        real_popen = subprocess.Popen

        class NoProcess:
            pid, returncode = -1, 0
            def poll(self): return 0
            def wait(self, timeout=None): return 0
            def kill(self): pass
            def terminate(self): pass
            def __enter__(self): return self
            def __exit__(self, *exc): return False

        def popen(cmd, *a, **kw):
            # Tools such as 7-Zip run as usual; a program that came with a mod
            # is never started, and the mod is left out.
            first = cmd[0] if isinstance(cmd, (list, tuple)) else cmd
            if not under(long(first), rec.dl):
                return real_popen(cmd, *a, **kw)
            rec.refuse(f"runs a program ({Path(str(first)).name})")
            return NoProcess()
        subprocess.Popen = popen

        P = plmod.Pipeline
        real_one = P._extract_and_install

        def extract_and_install(self, pm):
            rec.local.key = id(pm.build_mod)
            try:
                return real_one(self, pm)
            finally:
                rec.local.key = None
        P._extract_and_install = extract_and_install

        real_cleanup = P._run_texture_cleanup

        def cleanup(self):
            rec.cleanup_mods = [pm.build_mod for pm in self._cleanup_mods]
            rec.local.key = "cleanup"
            try:
                return real_cleanup(self)
            finally:
                rec.local.key = None
        P._run_texture_cleanup = cleanup
        # The game program file patches need a person; they stay manual.
        P._run_exe_tools = lambda self: None


# --------------------------------------------------------------------------
# Translating the record into ModSync instructions
# --------------------------------------------------------------------------

class Untranslatable(Exception):
    pass


def translate(ops, dl_root: Path, game: Path, mods: Path):
    def mod_path(p: Path) -> str:
        if not under(p, dl_root):
            raise Untranslatable(f"uses a file outside its download: {p}")
        parts = Path(rel(p, dl_root)).parts[1:]     # drop the mod's cache folder
        if parts and (parts[0].startswith("_linked_patch_") or parts[0] == "_loose"):
            parts = parts[1:]
        if not parts:
            raise Untranslatable(f"unexpected path {p}")
        return "<<modDirectory>>\\" + "\\".join(parts)

    def game_path(p: Path) -> str:
        return "<<kotorDirectory>>\\" + rel(p, game) if under(p, game) else "<<kotorDirectory>>"

    def step(action, source, destination="", arguments="", overwrite=True):
        return {"Action": action, "Source": source, "Destination": destination,
                "Arguments": arguments, "Overwrite": overwrite}

    extracts = []        # (dest dir, instruction), in the order they were unpacked
    emitted = set()
    out = []

    def need(p: Path):
        """Emit the Extract steps a path inside an unpacked archive depends on."""
        for dest, archive in extracts:
            if (p == dest or under(p, dest)) and dest not in emitted:
                emitted.add(dest)
                if archive.with_suffix("").name != dest.name:
                    raise Untranslatable(f"unpacks {archive.name} to {dest.name}")
                out.append(step("Extract", [mod_path(archive)]))

    pending = []   # consecutive copies: (src, dst)
    staged = {"n": 0}

    def flush():
        if not pending:
            return
        runs = []
        for src, dst in pending:
            if runs and runs[-1][0] == (src.parent, dst.parent):
                runs[-1][1].append((src, dst))
            else:
                runs.append(((src.parent, dst.parent), [(src, dst)]))
        for (src_dir, dst_dir), items in runs:
            for src, _ in items:
                need(src)
            same = [s for s, d in items if s.name.lower() == d.name.lower()]
            renamed = [(s, d) for s, d in items if s.name.lower() != d.name.lower()]
            if same:
                on_disk = sorted(f.name.lower() for f in disk(src_dir).iterdir() if f.is_file())
                names = sorted({s.name.lower() for s in same})
                if names == on_disk and len(same) == len(names) and len(names) > 1:
                    sources = [mod_path(src_dir) + "\\*"]
                else:
                    sources = [mod_path(s) for s in same]
                out.append(step("Copy", sources, game_path(dst_dir)))
            for s, d in renamed:
                # A file installed under another name: copy it to a folder of
                # its own next to it in the mod, rename it there, then copy it
                # into the game, so nothing already in the game is renamed.
                if s.suffix.lower() != d.suffix.lower():
                    raise Untranslatable(f"installs {s.name} as {d.name}, a different kind of file")
                staged["n"] += 1
                folder = mod_path(s.parent) + f"\\renamed-{staged['n']}"
                out.append(step("Copy", [mod_path(s)], folder))
                out.append(step("Rename", [folder + "\\" + s.name], d.name))
                out.append(step("Copy", [folder + "\\" + d.name], game_path(d.parent)))
        pending.clear()

    for op in ops:
        kind = op[0]
        if kind != "copy":
            flush()
        if kind == "extract":
            extracts.append((op[2], op[1]))
        elif kind == "copy":
            pending.append((op[1], op[2]))
        elif kind == "delete":
            out.append(step("Delete", [game_path(op[1])], overwrite=False))
        elif kind == "delete_mod":
            need(op[1])
            out.append(step("Delete", [mod_path(op[1])], overwrite=False))
        elif kind == "rename":
            src, dst = op[1], op[2]
            if src.parent != dst.parent:
                raise Untranslatable(f"moves {src.name} between game folders")
            out.append(step("Rename", [game_path(src)], dst.name))
        elif kind == "patch":
            td, exe, index = op[1], op[2], op[3]
            need(td)
            if exe is not None and (exe.parent / "tslpatchdata") == td:
                source = mod_path(exe)
            elif td.name.lower() == "tslpatchdata":
                source = mod_path(td.parent)
            else:
                raise Untranslatable("runs a patcher without a tslpatchdata folder")
            has_namespaces = any(f.name.lower() == "namespaces.ini" for f in disk(td).iterdir())
            out.append(step("Patcher", [source], "<<kotorDirectory>>",
                            str(index) if has_namespaces else ""))
    flush()
    return out


# --------------------------------------------------------------------------
# TOML output
# --------------------------------------------------------------------------

def toml_str(s: str) -> str:
    if "'" not in s and "\n" not in s:
        return f"'{s}'"
    return json.dumps(s, ensure_ascii=False)


def write_overlay(path: Path, build: str, entries, header: str):
    lines = [f"# {line}".rstrip() for line in header.splitlines()]
    lines.append("")
    for c, instructions, note in entries:
        lines.append("[[thisMod]]")
        lines.append(f"Guid = {toml_str(c['Guid'])}")
        lines.append(f"Name = {toml_str(c['Name'])}")
        lines.append(f"# {note}")
        lines.append("")
        for i in instructions:
            lines.append("[[thisMod.Instructions]]")
            lines.append(f"Action = {toml_str(i['Action'])}")
            lines.append("Source = [" + ", ".join(toml_str(s) for s in i["Source"]) + "]")
            lines.append(f"Destination = {toml_str(i['Destination'])}")
            lines.append(f"Arguments = {toml_str(i['Arguments'])}")
            lines.append(f"Overwrite = {'true' if i['Overwrite'] else 'false'}")
            lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8", newline="\n")


# --------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--old-app", required=True)
    ap.add_argument("--build", required=True)
    ap.add_argument("--guide", required=True)
    ap.add_argument("--mods", required=True)
    ap.add_argument("--downloads", nargs="+", required=True)
    ap.add_argument("--game-copy", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--old-builds")
    ap.add_argument("--only")
    ap.add_argument("--out", required=True)
    ap.add_argument("--log")
    args = ap.parse_args()

    work = Path(args.work).resolve()
    game = Path(args.game_copy).resolve()
    mods = Path(args.mods).resolve()
    if not under(game, work):
        sys.exit("--game-copy must be inside --work: it is modded during the run")
    # Keep the old app's settings and library away from the real ones.
    home = work / "home"
    home.mkdir(parents=True, exist_ok=True)
    os.environ["USERPROFILE"] = os.environ["HOME"] = str(home)
    os.environ["APPDATA"] = os.environ["LOCALAPPDATA"] = str(home)
    sys.path.insert(0, str(Path(args.old_app).resolve()))

    import installer.extractor as extractor
    import installer.patcher_strategy as patcher_strategy
    import installer.pipeline as plmod
    import installer.config_loader as cl
    from scraper.deadlystream import DeadlyStreamClient

    guide = tomllib.loads(Path(args.guide).read_text(encoding="utf-8"))
    old_mods = old_build_mods(args)
    matched = match_components(old_mods, guide)
    files = downloads_by_component(args.downloads, mods)
    dl = work / "downloads"
    dl.mkdir(parents=True, exist_ok=True)
    stage_cache(old_mods, matched, files, mods, dl, plmod.Pipeline._CACHE_MANIFEST)

    cached = []
    from installer.download_paths import download_folder_name
    for m in old_mods:
        d = dl / download_folder_name(m.file_id, m.slug)
        if d.is_dir() and any(f.is_file() for f in d.iterdir()):
            cached.append(m)
        elif plmod.texture_dedupe.is_cleanup_step(m.name):
            cached.append(m)

    # Headless patcher strategy only: never open a patcher window.
    config = cl.load_config()
    config["installer_types"]["tslpatcher"]["execution"]["strategies"] = ["holopatcher_shim"]
    cl.load_config.cache_clear()
    cl.load_config = lambda: config

    rec = Recorder(game, dl.resolve())
    rec.cleanup_mods = []
    rec.install(plmod, extractor, patcher_strategy)
    logs = []
    pipe = plmod.Pipeline(
        cached, game_path=game, download_dir=dl, client=DeadlyStreamClient(),
        on_log=lambda m, t="": logs.append(f"[{t or 'info'}] {m}"),
        auto_unattended=True, record_to_library=False, game_type=(
            "KOTOR2" if args.build.startswith("k2") else "KOTOR1"))
    pipe.start()
    while pipe.is_running:
        time.sleep(2)
    if args.log:
        Path(args.log).write_text("\n".join(logs), encoding="utf-8")

    only = set(Path(args.only).read_text().split()) if args.only else None
    status = {id(pm.build_mod): pm.status for pm in pipe.mods}
    # The old app could list one guide entry as two mods (one per link); their
    # steps are joined, in the order the old installer ran them.
    ran = {id(pm.build_mod): i for i, pm in enumerate(pipe.mods)}
    parts: dict = OrderedDict()
    report = []
    for m in sorted(old_mods, key=lambda m: ran.get(id(m), len(ran))):
        c = matched.get(id(m))
        if c is None or (only is not None and c["Guid"] not in only):
            continue
        st = status.get(id(m))
        why = None
        instructions = []
        if plmod.texture_dedupe.is_cleanup_step(m.name):
            if st == plmod.ModStatus.DONE:
                instructions = [{"Action": "DelDuplicate", "Source": [".tga", ".dds", ".tpc"],
                                 "Destination": "<<kotorDirectory>>\\Override",
                                 "Arguments": ".tpc", "Overwrite": True}]
            else:
                why = f"old installer status {st.name if st else 'not run'}"
        elif st != plmod.ModStatus.DONE:
            why = f"old installer status {st.name if st else 'not run (no download)'}"
        elif id(m) in rec.unsupported:
            why = "; ".join(rec.unsupported[id(m)])
        else:
            try:
                instructions = translate(rec.ops.get(id(m), []), dl.resolve(), game, mods)
            except Untranslatable as e:
                why = str(e)
            if not why and not instructions:
                why = "the old installer changed nothing"
        report.append({"guid": c["Guid"], "name": c["Name"], "file_id": m.file_id,
                       "covered": why is None, "reason": why or ""})
        source = (f"DeadlyStream file {m.file_id}" if m.source_host == "deadlystream"
                  else f"build entry {m.guide_index or m.install_order}")
        part = parts.setdefault(c["Guid"], {"c": c, "steps": [], "from": [], "why": []})
        part["steps"] += instructions
        part["from"].append(source)
        if why:
            part["why"].append(why)
    order = {c["Guid"]: i for i, c in enumerate(guide["thisMod"])}
    entries = [
        (p["c"], p["steps"], "From the old installer's handling of " + " and ".join(p["from"]) + ".")
        for _, p in sorted(parts.items(), key=lambda kv: order.get(kv[0], 0))
        if not p["why"]
    ]
    header = (
        f"Instruction overlay for the {args.build} community build.\n"
        "Generated by desktop/tauri/tools/overlay_from_old_installer.py from what the old\n"
        "kotor-mod-manager installer did with each mod's downloaded archives. Do not edit\n"
        "by hand; regenerate it. See README.md in this folder.")
    write_overlay(Path(args.out), args.build, entries, header)
    Path(args.out).with_suffix(".report.json").write_text(
        json.dumps(report, indent=1, ensure_ascii=False), encoding="utf-8")
    print(f"{len(entries)} mod(s) written, {sum(1 for p in parts.values() if p['why'])} left out")


if __name__ == "__main__":
    main()
