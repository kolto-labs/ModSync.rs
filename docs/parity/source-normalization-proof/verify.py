"""Assert retained production CLI results; does not compile or execute Rust tests."""
from pathlib import Path
import json
p=Path(__file__).parent
read=lambda n:json.loads((p/n).read_text())
r=read('receipt.json')['checks'];s=read('supplement-receipt.json')['checks'];v=read('validation-receipt.json')['checks'];checks=[]
def check(name,value):
 assert value,name
 checks.append(name)
for fmt in ['json','toml','yaml','xml']:
 m=r['matrix-'+fmt];check('12-shape Source parity '+fmt,m['rust']==m['csharp']==0 and m['equal'] and m['idempotent'])
 t=s['supplement-'+fmt];check('supplement load and reload '+fmt,t['rust']==t['csharp']==0 and t['idempotent'] and t['unrelatedFieldsUnchanged'])
 proj=t['rustProjection'];check('empty defaults '+fmt,all(x['instructions'][0]['source']==[] for x in proj[:4]))
 check('deep option source '+fmt,proj[-1]['options'][0]['options'][0]['instructions'][0]['source']==['<<modDirectory>>/nested/deep/asset'])
 check('Choose keeps original case and surviving reference '+fmt,proj[-1]['instructions'][0]['source']==['AAAAAAAA-1111-4111-8111-111111111111','literal GUID text'])
 literal=s['choose-literal-'+fmt]['rustProjection'][0]['instructions'][0]['source'];check('Choose whole Source exempt '+fmt,literal==['  ','\\literal\\path','{AAAAAAAA-1111-4111-8111-111111111111}'])
 bad=s['typed-malformed-'+fmt];check('typed malformed established outcome '+fmt,bad['rust']==bad['oldRust']==(0 if fmt=='xml' else 1))
for fmt in ['yaml','toml']:check('embedded parity '+fmt,r['embedded-'+fmt]['equal'])
for key,value in r.items():
 if key.startswith('dedup-') and key!='dedup-g':check(key,value['rustProjection']==value['csharpProjection'])
check('fenced Markdown dedup exemption',len(r['fenced-rust-only']['projection'][0]['options'])==2)
check('filtered array dedup and Choose cleanup',r['special']['rustProjection']==r['special']['csharpProjection'] and r['special']['idempotent'] and len(r['special']['rustProjection'][0]['options'])==1)
for name in ['array-number','object','array-null','array-bool','bare-bool']:check('malformed '+name,r['malformed-'+name]['exit']==1)
for fmt in ['json','toml','yaml']:
 for name,text in [('empty',''),('whitespace','  ')]:check('bare scalar preserved '+name+' '+fmt,r['scalar-'+name+'-'+fmt]['rustProjection'][0]['instructions'][0]['source']==[text])
check('same authored Copy guide',r['copy-rust']['guideSha256']==r['copy-csharp-skip-validation']['guideSha256'])
check('Copy payload bytes',r['copy-rust']['manifest']['Override/file.txt']==r['copy-csharp-skip-validation']['manifest']['Override/file.txt']==r['payloadSha256'])
check('absolute reinterpretation contained',r['absolute-inside']['exit']==0 and r['absolute-inside']['manifest']['Override/outside.txt']==r['payloadSha256'] and r['outside-sentinel-preserved'])
for name,text in [('traversal','Unsafe path: ../outside.txt'),('drive','Unsafe path: C:/'),('unknown','Unsafe path: <<unknownDirectory>>'),('symlink','Symlinks are unsupported'),('mixed','Path must start')]:
 receipt=read('reject-'+name+'.receipt.json');check('specific rejection '+name,r['reject-'+name]['exit']==1 and not r['reject-'+name]['manifest'] and text in receipt['stderr'])
check('validation no mutation',all(x['unchanged'] for x in v.values()))
check('relative validation accepted',v['relative']['exit']==0)
check('empty downstream validation rejects',all(v[k]['exit']==1 for k in ['bare-empty','bare-whitespace','filtered-empty']))
f=read('freeze.json');check('53 frozen files and suffixes unchanged',f['count']==53 and all(x['unchanged'] for x in f['entries']))
print(json.dumps(dict(assertions=len(checks),passed=checks),indent=2))
