#!/usr/bin/env python3
"""Serialized production-only gates. Never build/run test targets."""
from pathlib import Path
import subprocess, os, time, json, hashlib
repo=Path(__file__).resolve().parents[3]
env=os.environ.copy()
target=Path(env.get('CARGO_TARGET_DIR', str(repo/'target'))).resolve()
env['CARGO_TARGET_DIR']=str(target)
out=Path(env.get('MODSYNC_PROOF_OUT', str(target/'cloud-proof/gates'))).resolve()
out.mkdir(parents=True, exist_ok=True)
rows=[]
commands=[('fmt',['cargo','fmt','--all','--','--check']),('build-headless',['cargo','build','--locked','--no-default-features','--lib','--bin','modsync','-j2']),('clippy-headless',['cargo','clippy','--locked','--no-default-features','--lib','--bin','modsync','-j2','--','-D','warnings']),('build-desktop',['cargo','build','--locked','--lib','--bin','modsync','-j2']),('clippy-desktop',['cargo','clippy','--locked','--lib','--bin','modsync','-j2','--','-D','warnings'])]
for name,args in commands:
 start=time.time()
 with (out/(name+'.log')).open('w') as f:r=subprocess.run(args,cwd=repo,env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append(dict(argv=args,exit=r.returncode,seconds=time.time()-start,log=name+'.log'));print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
record=dict(commands=rows,target=env['CARGO_TARGET_DIR'],binarySha256=hashlib.sha256((target/'debug'/('modsync.exe' if os.name == 'nt' else 'modsync')).read_bytes()).hexdigest(),sourceSha256={str(p.relative_to(repo)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((repo/'src').rglob('*.rs'))})
(out/'build-receipt.json').write_text(json.dumps(record,indent=2)+'\n')
