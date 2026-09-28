#!/usr/bin/env python3
"""Serialized production-only gates. Never build/run test targets."""
from pathlib import Path
import subprocess, os, time, json, hashlib
out=Path(__file__).resolve().parent
repo=out.parents[2]
env=os.environ.copy();env['CARGO_TARGET_DIR']='/home/brunner56/.cache/modsync-rs-target-lead';rows=[]
commands=[('fmt',['cargo','fmt','--all','--','--check']),('build-headless',['cargo','build','--locked','--no-default-features','--lib','--bin','modsync','-j2']),('clippy-headless',['cargo','clippy','--locked','--no-default-features','--lib','--bin','modsync','-j2','--','-D','warnings']),('build-desktop',['cargo','build','--locked','--lib','--bin','modsync','-j2']),('clippy-desktop',['cargo','clippy','--locked','--lib','--bin','modsync','-j2','--','-D','warnings'])]
for name,args in commands:
 start=time.time()
 with (out/(name+'.log')).open('w') as f:r=subprocess.run(args,cwd=repo,env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append(dict(argv=args,exit=r.returncode,seconds=time.time()-start,log=name+'.log'));print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
record=dict(commands=rows,target=env['CARGO_TARGET_DIR'],binarySha256=hashlib.sha256(Path(env['CARGO_TARGET_DIR']+'/debug/modsync').read_bytes()).hexdigest(),sourceSha256={str(p.relative_to(repo)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((repo/'src').rglob('*.rs'))})
(out/'build-receipt.json').write_text(json.dumps(record,indent=2)+'\n')
