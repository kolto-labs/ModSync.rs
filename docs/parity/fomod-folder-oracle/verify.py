from pathlib import Path
import json
r = json.loads(Path(__file__).with_name("result.json").read_text())
assert r["Result"] == "Success"
assert r["Instructions"] == [{"Action":"Copy","Source":["<<modDirectory>>/package/textures/*"],"Destination":"<<kotorDirectory>>/Override","Overwrite":True}]
assert {p:v for p,v in r["Files"].items() if not p.startswith(".modsync/")} == {"Override/top.txt":"top"}
print("Reference folder mapping installs top-level files only; session metadata excluded from asset comparison.")
