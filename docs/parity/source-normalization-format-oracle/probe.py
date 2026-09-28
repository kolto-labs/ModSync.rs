from pathlib import Path
import hashlib,json,os,subprocess,tempfile,uuid
root=Path(tempfile.mkdtemp(prefix='source-format-oracle-'));out=Path('/home/brunner56/.cache/modsync-resume/source-format-oracle');out.mkdir(exist_ok=True)
rust='/home/brunner56/.cache/modsync-resume/wave2-reviewed-binary'
dll='/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0/ModSync.Core.dll'
for d in ['home','config','cache','tmp']:(root/d).mkdir()
cases=[('relative','x/file.txt'),('array_filtered',['','  ','x/file.txt']),('backslash',['\\x\\file.txt']),('repeated',['x//sub///file.txt/']),('mod',['<<modDirectory>>//x//file.txt']),('game',['<<gameDirectory>>/x/file.txt']),('kotor',['<<kotorDirectory>>/x/file.txt']),('mixed_case',['<<MODDIRECTORY>>/x/file.txt']),('unknown',['<<unknownDirectory>>/x/file.txt']),('drive',['C:\\x\\file.txt']),('absolute',['/x/file.txt']),('traversal',['../../outside.txt'])]
components=[]
for name,source in cases:
 components.append({'Guid':str(uuid.uuid5(uuid.NAMESPACE_URL,'example/'+name)),'Name':'Example '+name,'IsSelected':True,'Instructions':[{'Action':'Copy','Source':source,'Destination':'<<kotorDirectory>>/Override'}]})
components[0]['Options']=[{'Guid':str(uuid.uuid5(uuid.NAMESPACE_URL,'example/option')),'Name':'Example Option','Instructions':[{'Action':'Copy','Source':['\\nested\\file.txt'],'Destination':'<<kotorDirectory>>/Override'}]}]
(root/'input.json').write_text(json.dumps(components,indent=2));records=[]
def call(label,args,timeout=90):
 p=subprocess.run(args,capture_output=True,text=True,timeout=timeout)
 r={'label':label,'argv':args,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr};records.append(r);(out/(label+'.json')).write_text(json.dumps(r,indent=2)+'\n');return p.returncode
sandbox=['bwrap','--unshare-net','--ro-bind','/','/','--dev','/dev','--proc','/proc','--bind',str(root),str(root),'--setenv','HOME',str(root/'home'),'--setenv','XDG_CONFIG_HOME',str(root/'config'),'--setenv','XDG_CACHE_HOME',str(root/'cache'),'--setenv','TMPDIR',str(root/'tmp'),'--chdir',str(root),'--']
for fmt in ['json','toml','yaml','xml']:
 inp=root/('input.'+fmt)
 if fmt!='json':assert call('render_'+fmt,[rust,'convert',str(root/'input.json'),str(inp),'-f',fmt])==0
 dest=root/('csharp-'+fmt+'.json')
 code=call('csharp_'+fmt,sandbox+['dotnet',dll,'convert','--input',str(inp),'--output',str(dest),'--format','json','--plaintext','--non-interactive'])
 for p in [inp,dest]:
  if p.exists():(out/p.name).write_bytes(p.read_bytes())
def components_of(v):
 if isinstance(v,list):return v
 for k in ['thisMod','Components','ModSync']:
  if isinstance(v.get(k),list):return v[k]
 raise ValueError(list(v))
def projection(path):
 def item(c):return {'name':c.get('Name'),'instructions':[{'action':i.get('Action'),'source':i.get('Source')} for i in c.get('Instructions',[])],'options':[item(o) for o in c.get('Options',[])]}
 return [item(c) for c in components_of(json.loads(path.read_text()))]
projections={fmt:projection(out/('csharp-'+fmt+'.json')) for fmt in ['json','toml','yaml','xml'] if (out/('csharp-'+fmt+'.json')).exists()}
receipt={'root':str(root),'rustSha256':hashlib.sha256(Path(rust).read_bytes()).hexdigest(),'csharpSha256':hashlib.sha256(Path(dll).read_bytes()).hexdigest(),'cases':len(cases),'projection':projections,'matchesJson':{k:v==projections.get('json') for k,v in projections.items()},'exitCodes':{r['label']:r['exit'] for r in records},'scope':'Pinned C# conversion oracle only; no source changes or post-fix Rust acceptance. Bare empty/whitespace scalar and malformed non-string shapes not included.'}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:receipt[k] for k in ['cases','matchesJson','exitCodes','scope']},indent=2))
