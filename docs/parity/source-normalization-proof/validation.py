from pathlib import Path
s=(Path(__file__).parent/'probe.py').read_text()
exec(s.split("for fmt in ['json','toml','yaml','xml']:")[0])
exec('def component('+s.split('def component(')[1].split('opts=')[0])
mods=root/'mods';game=root/'game';(mods/'x').mkdir(parents=True);(game/'Override').mkdir(parents=True);(mods/'x/file.txt').write_bytes(b'Validation source')
def tree():return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for d in [mods,game] for p in d.rglob('*') if p.is_file()}
for name,source in [('relative',['x/file.txt']),('bare-empty',''),('bare-whitespace','  '),('filtered-empty',['','  '])]:
 inp=root/(name+'.json');inp.write_text(json.dumps([component(source)]));shutil.copy(inp,out/('validation-'+inp.name));before=tree();code=call('validation-'+name,[rust,'validate','--input',inp,'--game-dir',game,'--source-dir',mods,'--use-file-selection']);after=tree();checks[name]=dict(exit=code,unchanged=before==after,before=before,after=after)
(out/'validation-receipt.json').write_text(json.dumps(dict(root=str(root),checks=checks),indent=2)+'\n');print(json.dumps(checks,indent=2))
