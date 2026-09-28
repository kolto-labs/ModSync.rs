from pathlib import Path
import json,hashlib,tempfile,subprocess,shutil,uuid,os
repo=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent
root=Path(tempfile.mkdtemp(prefix='source-normalization-proof-')); rust=Path('/home/brunner56/.cache/modsync-rs-target-lead/debug/modsync');dll=Path('/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0/ModSync.Core.dll')
for d in ['home','config','cache','tmp']:(root/d).mkdir()
sandbox=['bwrap','--unshare-net','--ro-bind','/','/','--dev','/dev','--proc','/proc','--bind',str(root),str(root),'--setenv','HOME',str(root/'home'),'--setenv','XDG_CONFIG_HOME',str(root/'config'),'--setenv','XDG_CACHE_HOME',str(root/'cache'),'--setenv','TMPDIR',str(root/'tmp'),'--chdir',str(root),'--']
records=[];checks={}
def call(label,args):
 p=subprocess.run([str(x) for x in args],capture_output=True,text=True,timeout=90,env={**os.environ,'HOME':str(root/'home'),'XDG_CONFIG_HOME':str(root/'config'),'XDG_CACHE_HOME':str(root/'cache')})
 r=dict(label=label,argv=[str(x) for x in args],exit=p.returncode,stdout=p.stdout,stderr=p.stderr);records.append(r);(out/(label+'.receipt.json')).write_text(json.dumps(r,indent=2)+'\n');return p.returncode
def comps(v):
 if isinstance(v,list):return v
 return next(v[k] for k in ['thisMod','Components','ModSync'] if isinstance(v.get(k),list))
def project(p):
 def c(x):return dict(name=x.get('Name'),instructions=[dict(action=i.get('Action'),source=i.get('Source',[])) for i in x.get('Instructions',[])],options=[c(o) for o in x.get('Options',[])])
 return [c(x) for x in comps(json.loads(p.read_text()))]
def conv(label,inp,cs=False):
 dest=root/(label+'.json'); args=sandbox+['dotnet',str(dll),'convert','--input',str(inp),'--output',str(dest),'--format','json','--plaintext','--non-interactive'] if cs else [rust,'convert',inp,dest,'-f','json']
 code=call(label,args)
 if dest.exists():shutil.copy(dest,out/dest.name)
 return code,dest
for fmt in ['json','toml','yaml','xml']:
 inp=root/('matrix.'+fmt);shutil.copy(repo/'docs/parity/source-normalization-format-oracle'/('input.'+fmt),inp);shutil.copy(inp,out/inp.name)
 r,rp=conv('matrix-rust-'+fmt,inp);c,cp=conv('matrix-csharp-'+fmt,inp,True);rr,rrp=conv('matrix-reload-'+fmt,rp)
 checks['matrix-'+fmt]=dict(rust=r,csharp=c,equal=project(rp)==project(cp),idempotent=project(rp)==project(rrp),rustProjection=project(rp),csharpProjection=project(cp))
for inp0 in sorted((repo/'docs/parity/source-normalization-scalar-oracle').glob('*')):
 if inp0.suffix not in ['.json','.toml','.yaml','.xml'] or 'receipt' in inp0.name:continue
 if not any(inp0.name.startswith(x+'-') for x in ['relative','empty','whitespace','nonstring']):continue
 inp=root/inp0.name;shutil.copy(inp0,inp);shutil.copy(inp,out/inp.name)
 r,rp=conv('scalar-rust-'+inp.stem,inp);c,cp=conv('scalar-csharp-'+inp.stem,inp,True)
 checks['scalar-'+inp.stem]=dict(rust=r,csharp=c,rustProjection=project(rp) if rp.exists() else None,csharpProjection=project(cp) if cp.exists() else None)
# Common embedded metadata: exact same authored body on both engines.
yaml=(repo/'docs/parity/source-normalization-expanded/embed.md').read_text()
toml='# Guide\n\n## Mod List\n\n### Example Component\n\n<!--<<ModSync>>\nGuid = "aaaaaaaa-0000-0000-0000-000000000050"\nName = "Example Component"\n[[Instructions]]\nAction = "Copy"\nSource = [\'\\x\\foo.tga\']\nDestination = "<<kotorDirectory>>/Override"\n-->\n'
for label,text in [('yaml',yaml),('toml',toml)]:
 inp=root/('embedded-'+label+'.md');inp.write_text(text);shutil.copy(inp,out/inp.name)
 r,rp=conv('embedded-rust-'+label,inp);c,cp=conv('embedded-csharp-'+label,inp,True)
 checks['embedded-'+label]=dict(rust=r,csharp=c,equal=project(rp)==project(cp),rustProjection=project(rp),csharpProjection=project(cp))
# Existing loader dedup fixtures, including their Markdown exclusions.
for inp0 in sorted(Path('/tmp/dedup-probe/fx').iterdir()):
 inp=root/('dedup-'+inp0.name);shutil.copy(inp0,inp);shutil.copy(inp,out/inp.name)
 r,rp=conv('dedup-rust-'+inp0.stem,inp);c,cp=conv('dedup-csharp-'+inp0.stem,inp,True)
 def dedup(p):
  return [dict(name=x.get('Name'),options=[o.get('Guid') for o in x.get('Options',[])],choose=[i.get('Source',[]) for i in x.get('Instructions',[]) if i.get('Action','').lower()=='choose']) for x in comps(json.loads(p.read_text()))]
 checks['dedup-'+inp0.stem]=dict(rust=r,csharp=c,rustProjection=dedup(rp) if rp.exists() else None,csharpProjection=dedup(cp) if cp.exists() else None)
# Deep options, mixed-case Choose, filtered-array dedup, immutable unrelated fields.
a='11111111-1111-1111-1111-111111111111';b='22222222-2222-2222-2222-222222222222'
def component(source,name='Example Copy'):
 return dict(Guid=str(uuid.uuid5(uuid.NAMESPACE_URL,name)),Name=name,IsSelected=True,Instructions=[dict(Action='Copy',Source=source,Destination='<<kotorDirectory>>/Override',Arguments='literal\\argument')])
opts=[dict(**component(['','  ','x//file.txt'],'Option A')),dict(**component(['<<modDirectory>>/x/file.txt'],'Option B'))];opts[0]['Guid']=a;opts[1]['Guid']=b
special=component(['x/file.txt']);special['Instructions'] += [dict(Action='cHoOsE',Source=[a,b])];special['Options']=opts
inp=root/'special.json';inp.write_text(json.dumps([special]));shutil.copy(inp,out/inp.name)
r,rp=conv('special-rust',inp);c,cp=conv('special-csharp',inp,True);rr,rrp=conv('special-reload',rp)
checks['special']=dict(rust=r,csharp=c,rustProjection=project(rp),csharpProjection=project(cp),idempotent=project(rp)==project(rrp))
fence=root/'fenced.json.md';fence.write_text('```json\n'+json.dumps([special])+'\n```');shutil.copy(fence,out/fence.name);r,rp=conv('fenced-rust',fence);checks['fenced-rust-only']=dict(exit=r,projection=project(rp))
# Malformed array/object leaves existing serde errors in place.
for name,source in [('array-number',['x',42]),('object',{'path':'x'}),('array-null',[None]),('array-bool',[True]),('bare-bool',True)]:
 inp=root/(name+'.json');inp.write_text(json.dumps([component(source)]));shutil.copy(inp,out/inp.name);r,rp=conv('malformed-'+name,inp);checks['malformed-'+name]=dict(exit=r)
# Same authored guide and identical payload in each engine's isolated game.
payload=b'Example normalized Copy\x00\xff\n';mods=root/'mods';mods.mkdir();(mods/'x').mkdir();(mods/'x/file.txt').write_bytes(payload)
def install(label,source,cs=False,skip=False):
 game=root/(label+'-game');game.mkdir();(game/'Override').mkdir();inp=root/(label+'-guide.json');inp.write_text(json.dumps([component(source)]));shutil.copy(inp,out/inp.name)
 args= ['install','--input',str(inp),'--game-dir',str(game),'--source-dir',str(mods),'--use-file-selection']
 cmd=sandbox+['dotnet',str(dll)]+args+['-y']+(['--skip-validation'] if skip else []) if cs else [rust]+args
 code=call(label,cmd);manifest={str(p.relative_to(game)):hashlib.sha256(p.read_bytes()).hexdigest() for p in game.rglob('*') if p.is_file()};return dict(exit=code,manifest=manifest,guideSha256=hashlib.sha256(inp.read_bytes()).hexdigest(),skipValidation=skip)
checks['copy-rust']=install('copy-rust',['x/file.txt']);checks['copy-csharp-full-validation']=install('copy-csharp-full',['x/file.txt'],True);checks['copy-csharp-skip-validation']=install('copy-csharp-skip',['x/file.txt'],True,True)
sentinel=root/'outside.txt';sentinel.write_bytes(b'OUTSIDE MUST STAY');inside=mods/str(sentinel).lstrip('/');inside.parent.mkdir(parents=True);inside.write_bytes(payload)
checks['absolute-inside']=install('absolute-inside',[str(sentinel)])
for name,source in [('traversal','../outside.txt'),('drive','C:\\x\\file.txt'),('unknown','<<unknownDirectory>>/x/file.txt'),('symlink','link.txt'),('mixed','<<MODDIRECTORY>>/x/file.txt')]:
 if name=='symlink':(mods/'link.txt').symlink_to(sentinel)
 checks['reject-'+name]=install('reject-'+name,[source])
 if name=='symlink':(mods/'link.txt').unlink()
checks['outside-sentinel-preserved']=sentinel.read_bytes()==b'OUTSIDE MUST STAY';checks['payloadSha256']=hashlib.sha256(payload).hexdigest()
receipt=dict(root=str(root),sourceCommit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),rustSha256=hashlib.sha256(rust.read_bytes()).hexdigest(),csharpSha256=hashlib.sha256(dll.read_bytes()).hexdigest(),checks=checks,commands=len(records))
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:v for k,v in checks.items() if k.startswith(('matrix-','embedded-','copy-','absolute-','reject-'))},indent=2))
