from pathlib import Path
s=(Path(__file__).parent/"probe.py").read_text()
exec(s.split("for fmt in [\'json\',\'toml\',\'yaml\',\'xml\']:")[0])
exec("def component("+s.split("def component(")[1].split("opts=")[0])
exec(("payload="+s.split("payload=")[1]).replace("out/\'receipt.json\'", "out/\'safety-receipt.json\'"))
