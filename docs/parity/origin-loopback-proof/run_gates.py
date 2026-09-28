#!/usr/bin/env python3
"""Serialized production-only gates; deliberately never compile or run test targets."""
import hashlib,json,os,pathlib,subprocess,time
repo=pathlib.Path(__file__).resolve().parents[3];out=repo/'docs/parity/origin-loopback-proof/gates';out.mkdir(exist_ok=True)
env=os.environ.copy();env['CARGO_TARGET_DIR']='/home/brunner56/.cache/modsync-rs-target-lead'
commands=[['cargo','fmt','--all','--','--check'],['cargo','build','--locked','--offline','--no-default-features','--lib','--bin','modsync','-j2'],['cargo','clippy','--locked','--offline','--no-default-features','--lib','--bin','modsync','-j2','--','-D','warnings'],['cargo','build','--locked','--offline','--lib','--bin','modsync','-j2'],['cargo','clippy','--locked','--offline','--lib','--bin','modsync','-j2','--','-D','warnings']]
records=[]
for i,argv in enumerate(commands):
 start=time.time();log=out/(str(i)+'.log')
 with log.open('w') as stream:r=subprocess.run(argv,cwd=repo,env=env,stdout=stream,stderr=subprocess.STDOUT)
 records.append(dict(argv=argv,exit=r.returncode,seconds=time.time()-start,log=log.name));print(argv,r.returncode,flush=True)
 (out/'receipt.json').write_text(json.dumps(dict(commands=records,target=env['CARGO_TARGET_DIR'],sourceSha256={str(p.relative_to(repo)):hashlib.sha256(p.read_bytes()).hexdigest() for p in (repo/'src').rglob('*.rs')}),indent=2)+'\n')
 if r.returncode:raise SystemExit(r.returncode)
