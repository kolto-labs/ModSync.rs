import json,sys,collections
def strip(v):
  if isinstance(v,dict):
    o={k:strip(x) for k,x in v.items()}
    return {k:x for k,x in o.items() if x not in (None,'',[],{},0,False)}
  if isinstance(v,list): return [strip(x) for x in v]
  if isinstance(v,(int,float)) and not isinstance(v,bool): return v
  return v
def comps(a):
  return a if isinstance(a,list) else a.get('Components',a.get('thisMod'))
base,other=sys.argv[1],sys.argv[2]
A=comps(json.load(open(base)));B=comps(json.load(open(other)))
d=collections.Counter();ex={}
if len(A)!=len(B): print('COUNT',len(A),len(B))
def cmp(a,b,path):
  if a==b: return
  if isinstance(a,dict) and isinstance(b,dict):
    for k in set(a)|set(b): cmp(a.get(k),b.get(k),path+(k,))
  elif isinstance(a,list) and isinstance(b,list) and len(a)==len(b):
    for x,y in zip(a,b): cmp(x,y,path+('[]',))
  else:
    key='.'.join(p for p in path); d[key]+=1; ex.setdefault(key,(a,b))
for a,b in zip(A,B): cmp(strip(a),strip(b),())
for k,v in sorted(d.items()): print(f'  {k}: {v}  e.g. {json.dumps(ex[k],default=str)[:250]}')
