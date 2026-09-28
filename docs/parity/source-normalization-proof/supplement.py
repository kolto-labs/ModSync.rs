from pathlib import Path
# Reuse production CLI wrappers only; no Rust harness or test binaries.
exec((Path(__file__).parent/'probe.py').read_text().split("for fmt in ['json','toml','yaml','xml']:")[0])
import yaml,xml.etree.ElementTree as ET
oldrust=Path('/home/brunner56/.cache/modsync-resume/wave2-reviewed-binary')
def render(cs,fmt):
 if fmt=='json':return json.dumps(cs)
 if fmt=='yaml':return yaml.safe_dump_all(cs,sort_keys=False,explicit_start=True)
 if fmt=='toml':
  def table(x,key):
   scalar=[k+' = '+json.dumps(v) for k,v in x.items() if k not in ['Instructions','Options'] and v is not None]
   for k,v in x.items():
    if k in ['Instructions','Options']:
     for child in v:scalar.append(table(child,key+'.'+k))
   return '[['+key+']]\n'+'\n'.join(scalar)+'\n'
  return '\n'.join(table(c,'thisMod') for c in cs)
 doc=ET.Element('ModSync',{'xmlns:json':'http://james.newtonking.com/projects/json'})
 def add(parent,k,v):
  if v is None:return
  if isinstance(v,list):
   if not v:ET.SubElement(parent,k,{'json:Array':'true'})
   else:
    for i in v:
     node=ET.SubElement(parent,k,{'json:Array':'true'} if len(v)==1 else {})
     if isinstance(i,dict):
      for kk,vv in i.items():add(node,kk,vv)
     else:node.text=str(i).lower() if isinstance(i,bool) else str(i)
  elif isinstance(v,dict):
   node=ET.SubElement(parent,k)
   for kk,vv in v.items():add(node,kk,vv)
  else:ET.SubElement(parent,k).text=str(v).lower() if isinstance(v,bool) else str(v)
 for c in cs:add(doc,'ModComponents',c)
 return ET.tostring(doc,encoding='unicode')
def comp(name,source):
 return dict(Guid=str(uuid.uuid5(uuid.NAMESPACE_URL,name)),Name=name,IsSelected=True,Instructions=[dict(Action='Copy',Source=source,Destination='<<kotorDirectory>>\\literal//destination/',Arguments=' literal\\argument ' )])
for fmt in ['json','toml','yaml','xml']:
 cases=[comp('Missing',None),comp('Null',None),comp('Empty array',[]),comp('Blank array',['','  ']),comp('Colon',['http:////example//asset/']),comp('Leading slashes',['////x////file.txt']),comp('Choose parent',['x/file.txt'])]
 del cases[0]['Instructions'][0]['Source']
 a='AAAAAAAA-1111-4111-8111-111111111111';b='bbbbbbbb-2222-4222-8222-222222222222'
 cases[-1]['Instructions']=[dict(Action='cHoOsE',Source=[a,b,'literal GUID text'])]
 o1=comp('First option',['\\x\\file.txt']);o1['Guid']=a;o2=comp('Second option',['x/file.txt']);o2['Guid']=b
 o1['Options']=[comp('Grandchild',['nested\\deep//asset/'])];cases[-1]['Options']=[o1,o2]
 inp=root/('supplement-'+fmt+'.'+fmt);inp.write_text(render(cases,fmt));shutil.copy(inp,out/inp.name)
 r,rp=conv('supplement-rust-'+fmt,inp);c,cp=conv('supplement-csharp-'+fmt,inp,True);rr,rrp=conv('supplement-reload-'+fmt,rp)
 old=root/('supplement-old-'+fmt+'.json');oldcode=call('supplement-old-'+fmt,[oldrust,'convert',inp,old,'-f','json'])
 def other(p):
  def x(c):return dict(destination=[i.get('Destination') for i in c.get('Instructions',[])],arguments=[i.get('Arguments') for i in c.get('Instructions',[])],options=[x(o) for o in c.get('Options',[])])
  return [x(c) for c in comps(json.loads(p.read_text()))]
 checks['supplement-'+fmt]=dict(rust=r,csharp=c,idempotent=project(rp)==project(rrp),equal=project(rp)==project(cp) if cp.exists() else None,rustProjection=project(rp),csharpProjection=project(cp) if cp.exists() else None,unrelatedFieldsUnchanged=other(rp)==other(old) if oldcode==0 else None,nullRepresentation='omitted (format has no null token)' if fmt in ['toml','xml'] else 'authored null')
 # Choose Source is intentionally exempt even if values look like paths or blanks.
 choose=comp('Choose literal',['  ','\\literal\\path','{AAAAAAAA-1111-4111-8111-111111111111}']);choose['Instructions'][0]['Action']='CHOOSE'
 inp=root/('choose-literal-'+fmt+'.'+fmt);inp.write_text(render([choose],fmt));shutil.copy(inp,out/inp.name)
 r,rp=conv('choose-literal-rust-'+fmt,inp);c,cp=conv('choose-literal-csharp-'+fmt,inp,True)
 checks['choose-literal-'+fmt]=dict(rust=r,csharp=c,rustProjection=project(rp),csharpProjection=project(cp) if cp.exists() else None)
 # Mixed typed array preserved as an error for typed formats; XML is untyped text.
 inp=root/('malformed-'+fmt+'.'+fmt);inp.write_text(render([comp('Malformed',['x',42])],fmt));shutil.copy(inp,out/inp.name)
 r,rp=conv('typed-malformed-rust-'+fmt,inp);c,cp=conv('typed-malformed-csharp-'+fmt,inp,True)
 old=root/('malformed-old-'+fmt+'.json');oldcode=call('typed-malformed-old-'+fmt,[oldrust,'convert',inp,old,'-f','json'])
 checks['typed-malformed-'+fmt]=dict(rust=r,csharp=c,oldRust=oldcode,rustProjection=project(rp) if rp.exists() else None,csharpProjection=project(cp) if cp.exists() else None)
(out/'supplement-receipt.json').write_text(json.dumps(dict(root=str(root),rustSha256=hashlib.sha256(rust.read_bytes()).hexdigest(),oldRustSha256=hashlib.sha256(oldrust.read_bytes()).hexdigest(),checks=checks,commands=len(records)),indent=2)+'\n')
print(json.dumps(checks,indent=2))
