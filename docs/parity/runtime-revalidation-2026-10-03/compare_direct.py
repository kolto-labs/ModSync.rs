import json,sys,collections
def walk(lst,path,out):
  for i,c in enumerate(lst):
    p=path+(i,); out.append((p,c))
    walk(c.get('Options') or [],p,out)
def gmap(nodes):
  return {str(c.get('Guid','')).lower():p for p,c in nodes}
def norm(v,gm):
  if isinstance(v,str) and v.lower() in gm: return ('G',gm[v.lower()])
  if isinstance(v,list): return [norm(x,gm) for x in v]
  if isinstance(v,dict): return {k:norm(x,gm) for k,x in v.items() if k!='Options'}
  return v
def empty(v): return v in (None,'',[],{},0,False)
g,m=sys.argv[1],sys.argv[2]
cs=json.load(open(f'{g}-cs-{m}.json')); rs=json.load(open(f'{g}-rs-{m}.json'))
cn=[];rn=[];walk(cs,(),cn);walk(rs,(),rn)
print(g,m,'nodes',len(cn),len(rn))
cg=gmap(cn);rg=gmap(rn)
diffs=collections.Counter();ex={}
for (pc,c),(pr,r) in zip(cn,rn):
  assert pc==pr,(pc,pr)
  keys=set(c)|set(r)
  for k in keys:
    if k in('Guid','Options'):continue
    a=norm(c.get(k),cg);b=norm(r.get(k),rg)
    if a==b: continue
    if k not in c and empty(b): diffs['rs-only-empty:'+k]+=1;continue
    if k not in r and empty(a): diffs['cs-only-empty:'+k]+=1;continue
    if k not in c: diffs['rs-only:'+k]+=1; ex.setdefault('rs-only:'+k,(pc,b));continue
    if k not in r: diffs['cs-only:'+k]+=1; ex.setdefault('cs-only:'+k,(pc,a));continue
    diffs['value:'+k]+=1; ex.setdefault('value:'+k,(pc,a,b))
for k,v in sorted(diffs.items()):print(' ',k,v)
for k,v in ex.items():print('EX',k,json.dumps(v,default=str)[:600])
