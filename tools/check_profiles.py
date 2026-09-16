"""Cross-read profile files and compare CRUD/activation against the pinned C# CLI."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll',type=Path,required=True)
parser.add_argument('--rust',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
assert shutil.which('bwrap')
reference=args.reference_dll.resolve(strict=True)
rust=args.rust.resolve(strict=True)
output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
fixture=Path(__file__).resolve().parents[1]/'tests/fixtures/profile.json'
sandbox=['bwrap','--ro-bind','/','/','--dev','/dev','--tmpfs','/tmp','--tmpfs',str(Path.home()/'.local/share'),'--tmpfs',str(Path.home()/'.config'),'--bind',str(output),str(output),'--unshare-net','--die-with-parent']
def command(side, storage, action, *extra):
    executable=sandbox+['dotnet',str(reference)] if side=='csharp' else [str(rust)]
    result=subprocess.run(executable+['profile','--settings-dir',str(storage),'--action',action,*extra],capture_output=True,timeout=120,cwd=output)
    (output/(side+'-'+storage.name+'-'+action+'-'+str(len(list(output.glob('*.log'))))+'.log')).write_bytes(result.stdout+result.stderr)
    return result

def normalize(value):
    if isinstance(value,list): return [normalize(v) for v in value]
    if isinstance(value,dict):
        result={}
        for k,v in value.items():
            if k.lower() in ['createdutc','lastusedutc'] and v:
                datetime.datetime.fromisoformat(v.replace('Z','+00:00'))
                if v.startswith('2026-01-02') or v.startswith('2026-02-03'): result[k]=v
                else: result[k]='<runtime timestamp>'
            else: result[k]=normalize(v)
        return result
    return value

def parse_show(result):
    assert result.returncode==0, result.stderr.decode()
    text=result.stdout.decode('utf-8-sig')
    for offset,c in enumerate(text):
        if c=='{':
            try: value,_=json.JSONDecoder().raw_decode(text[offset:])
            except ValueError: continue
            if isinstance(value,dict) and value.get('Name')=='Renamed': return value
    raise AssertionError('No profile JSON in CLI output')

stores={}
for side in ['csharp','rust']:
    storage=output/side;profiles=storage/'profiles';profiles.mkdir(parents=True,exist_ok=True)
    assert not any(profiles.iterdir()), 'Use a fresh output directory'
    (profiles/'Loadout.json').write_bytes(fixture.read_bytes())
    (storage/'settings.json').write_text(json.dumps({'theme':'dark','managedDeploymentEnabled':True,'nested':{'keep':7}}))
    actions=[('create',['--name','Empty'],0),('create',['--name','Empty'],1),('clone',['--from','Loadout','--to','Copy'],0),('rename',['--from','Copy','--to','Renamed'],0),('activate',['--name','Renamed'],0),('delete',['--name','Empty'],0),('delete',['--name','Empty'],1)]
    for action,extra,expected in actions:
        result=command(side,storage,action,*extra)
        assert result.returncode==expected,(side,action,result.returncode,result.stderr.decode())
    stores[side]=storage
    assert not (profiles/'Copy.json').exists()
    assert not (profiles/'Empty.json').exists()
    assert normalize(json.loads((profiles/'Loadout.json').read_text()))==normalize(json.loads(fixture.read_text()))
cs=json.loads((stores['csharp']/'profiles/Renamed.json').read_text())
rs=json.loads((stores['rust']/'profiles/Renamed.json').read_text())
assert normalize(cs)==normalize(rs)
for key in ['csharp','rust']:
    settings=json.loads((stores[key]/'settings.json').read_text())
    assert settings=={'theme':'dark','managedDeploymentEnabled':True,'nested':{'keep':7},'activeProfileName':'Renamed'}
reads=[]
for writer,storage in stores.items():
    a=parse_show(command('csharp',storage,'show','--name','Renamed','--json'))
    b=parse_show(command('rust',storage,'show','--name','Renamed','--json'))
    assert normalize(a)==normalize(b)
    reads.append({'writer':writer,'both_readers_match':True})
receipt={'reference_commit':'d31ccbd3bfcddb4705917eafa6483715c166de3b','reference_dll_sha256':hashlib.sha256(reference.read_bytes()).hexdigest(),'fixture_sha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'operations_per_implementation':len(actions),'cross_reads':reads,'profile':normalize(rs),'settings_preserved':True,'scope':'ASCII-named profiles; create/duplicate, clone, rename, activate, delete/missing and cross-reader show JSON. Runtime timestamps validated as ISO dates then normalized. Not default storage discovery, malformed schema parity, GUI activation, install integration, crash atomicity or all CLI output modes.'}
(output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
