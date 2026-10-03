import json,sys,collections
exec(open(__import__('os').path.join(__import__('os').path.dirname(__file__),'compare_direct.py')).read().split("g,m=sys.argv")[0])
def strip(v):
  if isinstance(v,dict): return {k:strip(x) for k,x in v.items() if not empty(strip(x))}
  if isinstance(v,list): return [strip(x) for x in v]
  return v
g,m=sys.argv[1],sys.argv[2]
cs=json.load(open(f'{g}-cs-{m}.json')); rs=json.load(open(f'{g}-rs-{m}.json'))
cn=[];rn=[];walk(cs,(),cn);walk(rs,(),rn);cg=gmap(cn);rg=gmap(rn)
d=collections.Counter()
for (pc,c),(pr,r) in zip(cn,rn):
  for k in ('Instructions','ResourceRegistry'):
    a=strip(norm(c.get(k),cg));b=strip(norm(r.get(k),rg))
    if a!=b:
      # instruction-level
      if k=='Instructions' and len(a or [])==len(b or []):
        for x,y in zip(a or [],b or []):
          for kk in set(x)|set(y):
            if x.get(kk)!=y.get(kk): d[(k,kk,x.get('Action'),str(x.get(kk)),str(y.get(kk)))]+=1
      else: d[(k,'whole')]+=1; print(pc,json.dumps(a)[:300],'\n   ',json.dumps(b)[:300])
for k,v in d.items():print(g,m,k,v)
