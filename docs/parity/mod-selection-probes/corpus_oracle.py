"""Independent expected sets for the real-corpus sanity check.
Mirrors the plan's rules, not the Rust code: reset by cumulative tier and
case-insensitive category, then dependency closure over effective nodes
(component first, option second, option brings its ancestors)."""
import json, sys, tomllib

def prio(t):
    t = t.strip().lower()
    for key, p in (("essential", 1), ("recommend", 2), ("suggest", 3), ("option", 4)):
        if key in t:
            return p
    return {"1": 1, "2": 2, "3": 3, "4": 4}.get(t)

def load(path):
    d = tomllib.load(open(path, "rb"))
    comps = d["thisMod"]
    if not any(c.get("IsSelected", False) for c in comps):
        for c in comps:
            c["IsSelected"] = True
    return comps

def nodes(comps):
    out = []
    def walk(c, chain):
        out.append(chain + [c])
        for o in c.get("Options", []):
            walk(o, chain + [c])
    for c in comps:
        walk(c, [])
    return out

def lower(v):
    return [x.lower() for x in (v if isinstance(v, list) else [v] if v else [])]

def expected(comps, tiers=(), cats=(), add_cats=()):
    for c in comps:
        tm = (not tiers) or any(
            c.get("Tier", "") and ((prio(t) is not None and prio(c["Tier"]) is not None and prio(c["Tier"]) <= prio(t))
                                   or (prio(t) is None and c["Tier"].lower() == t.lower()))
            for t in tiers)
        cm = (not cats) or any(x in [y.lower() for y in cats] for x in lower(c.get("Category")))
        c["IsSelected"] = tm and cm
    for c in comps:
        if not c.get("WidescreenOnly") and any(x in [y.lower() for y in add_cats] for x in lower(c.get("Category"))):
            c["IsSelected"] = True
    allnodes = nodes(comps)
    top = {c["Guid"].lower(): c for c in comps}
    opt = {}
    for chain in allnodes:
        if len(chain) > 1:
            opt.setdefault(chain[-1]["Guid"].lower(), chain)
    eff = lambda chain: all(n.get("IsSelected", False) for n in chain)
    changed = True
    while changed:
        changed = False
        for chain in allnodes:
            if not eff(chain):
                continue
            for dep in lower(chain[-1].get("Dependencies")):
                target = [top[dep]] if dep in top else opt.get(dep)
                for n in target or []:
                    if not n.get("IsSelected", False):
                        n["IsSelected"] = True
                        changed = True
    conflicts = set()
    for chain in allnodes:
        if not eff(chain):
            continue
        for r in lower(chain[-1].get("Restrictions")):
            target = [top[r]] if r in top else opt.get(r)
            if target and eff(target):
                conflicts.add(tuple(sorted((chain[-1]["Guid"].lower(), r))))
    return [ch[-1]["Guid"] for ch in allnodes if ch[-1].get("IsSelected", False)], len(conflicts)

if __name__ == "__main__":
    path, mode = sys.argv[1], sys.argv[2]
    comps = load(path)
    if mode == "default":
        # CLI default-all: every top-level component, options as loaded, no resolution.
        for c in comps:
            c["IsSelected"] = True
        sel = [ch[-1]["Guid"] for ch in nodes(comps) if ch[-1].get("IsSelected", False)]
        res = (sel, 0)
    elif mode == "tier2":
        res = expected(comps, tiers=["2 - Recommended"])
    elif mode == "tier1-add-bugfix":
        res = expected(comps, tiers=["1 - Essential"], add_cats=["bugfix"])
    print(json.dumps({"selected": res[0], "conflicts": res[1]}))
