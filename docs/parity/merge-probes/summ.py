import json,sys
def load(p):
    try: v=json.load(open(p,encoding="utf-8-sig"))
    except Exception as e: return f"ERR {e}"
    if isinstance(v,dict): v=v.get("thisMod") or v.get("Components") or v.get("components") or v
    return v
def s(c):
    src=lambda i: (",".join(i.get("Source") or []) if isinstance(i.get("Source"),list) else (i.get("Source") or "")).replace("\\","/")
    ins=[(i.get("Action"),src(i),None if str(i.get("Action")).lower()=="extract" else i.get("Destination")) for i in c.get("Instructions") or []]
    out=f'{c.get("Guid","")[:8]} {c.get("Name")!r}'
    for k in ["Author","Description","Tier","InstallationMethod"]:
        if c.get(k): out+=f' {k[:4]}={c[k]!r}'
    if c.get("Language"): out+=f' Lang={c["Language"]}'
    if ins: out+=f' Ins={ins}'
    rr=c.get("ResourceRegistry") or {}
    if rr: out+=" RR="+str(list(rr.keys()))
    opts=c.get("Options") or []
    if opts: out+=" Opts="+str([((o.get("Guid") or "")[:8],o.get("Name"),o.get("Description") or "") for o in opts])
    return out
v=load(sys.argv[1])
if isinstance(v,str): print(v)
else:
    for c in v: print("  "+s(c))
