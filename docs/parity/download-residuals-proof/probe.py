#!/usr/bin/env python3
"""Download residuals production receipts: real fetch-downloads in one isolated network
namespace, with a recording loopback HTTPS origin and a synthetic loopback Nexus API.
Generic fixtures only; no Rust test or harness.

  cargo build --locked --lib --bin modsync -j2
  RESIDUALS_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/download-residuals-proof/probe.py
"""
import atexit, hashlib, http.server, io, json, os, pathlib, shutil, signal, ssl, subprocess, tempfile, threading, time, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/download-residuals'))).resolve()
_MARK=OUT/'.modsync-proof-out'
if OUT.exists() and any(OUT.iterdir()) and not _MARK.exists() and (TARGET/'cloud-proof').resolve() not in OUT.parents:raise SystemExit(f'Refusing to delete {OUT}: not an empty or earlier probe output directory (set MODSYNC_PROOF_OUT to a dedicated folder)')
shutil.rmtree(OUT,ignore_errors=True);OUT.mkdir(parents=True);_MARK.write_text('')
BIN=pathlib.Path(os.environ.get('MODSYNC_BINARY', str(TARGET/'debug/modsync'))).resolve()
def digest(p):
 h=hashlib.sha256()
 with open(p,'rb') as f:
  for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk)
 return h.hexdigest()
def sources():return {str(p.relative_to(REPO)):digest(p) for p in sorted(list((REPO/'src').rglob('*.rs'))+[REPO/'Cargo.toml',REPO/'Cargo.lock'])}
BINARY_BEFORE=digest(BIN);SOURCE_BEFORE=sources()
NEWEST=max((REPO/p).stat().st_mtime for p in SOURCE_BEFORE)
assert os.environ['RESIDUALS_PARENT_NETNS'] != os.readlink('/proc/self/ns/net'),'run under unshare -Urn with RESIDUALS_PARENT_NETNS'
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
def git(*a):
 r=subprocess.run(['git','-C',str(REPO),*a],capture_output=True,text=True);return r.stdout.strip() if r.returncode==0 else None
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='download-residuals-'))
if os.environ.get('MODSYNC_PROOF_KEEP')!='1':atexit.register(shutil.rmtree,ROOT,ignore_errors=True)
ENV={k:v for k,v in os.environ.items() if not k.lower().endswith('_proxy') and not any(t in k for t in ['NEXUS','MODSYNC_FOMOD'])}
for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
 ENV[k]=str(ROOT/k);pathlib.Path(ENV[k]).mkdir()
CHECKS=[];RUNS=[];REQUESTS=[]
def snapshot(p):return {str(f.relative_to(p)):digest(f) for f in sorted(p.rglob('*')) if f.is_file() and f.stat().st_size<(64<<20)}
def check(name,actual,expected=True):
 row=dict(name=name,actual=actual,expected=expected,passed=actual==expected);CHECKS.append(row)
 if not row['passed']:print('FAIL',name,repr(actual)[:400],flush=True)
def plain(text):
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:z.writestr(zipfile.ZipInfo('payload.txt',date_time=(2020,1,1,0,0,0)),text)
 return b.getvalue()
# The fetch-shared-url slow archive: about 1 MB on the wire, 1 GiB of zeros expanded, so
# archive::verify spends seconds between a transfer's end and its no-clobber publish.
def slow(size=1<<30):
 b=io.BytesIO();info=zipfile.ZipInfo('payload.bin',date_time=(2020,1,1,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED
 with zipfile.ZipFile(b,'w') as z:
  with z.open(info,'w') as f:
   chunk=bytes(1<<20)
   for _ in range(size>>20):f.write(chunk)
 return b.getvalue()
A=plain('from url a\n');B=plain('from url b\n');A_SHA=hashlib.sha256(A).hexdigest();B_SHA=hashlib.sha256(B).hexdigest()
SLOW=slow();SLOW_SHA=hashlib.sha256(SLOW).hexdigest()
NEXUS_ARCHIVE=plain('nexus payload\n');NEXUS_SHA=hashlib.sha256(NEXUS_ARCHIVE).hexdigest();OTHER=plain('another payload\n')
# path -> (bytes, response delay seconds, send Content-Length)
DELAY=0.4
SERVE={'/seq-unnamed/example.zip':(A,DELAY,True),'/seq-known/example.zip':(A,DELAY,True),
 '/mis-unnamed/example.zip':(A,DELAY,True),'/mis-known/example.zip':(B,DELAY,True),
 '/conc-unnamed/example.zip':(SLOW,DELAY,True),'/conc-known/example.zip':(SLOW,DELAY+1.0,True),
 '/length/example.zip':(A,DELAY,True),'/nolength/example.zip':(A,DELAY,False),
 '/nexus-cdn/example-archive.zip':(NEXUS_ARCHIVE,0,True),
 '/hseq-unnamed/example.zip':(A,DELAY,True),'/hseq-known/example.zip':(A,DELAY,True),
 '/hmis-unnamed/example.zip':(A,DELAY,True),'/hmis-known/example.zip':(B,DELAY,True),
 '/hpresent/example.zip':(A,DELAY,True),
 '/hconc-unnamed/example.zip':(SLOW,DELAY,True),'/hconc-known/example.zip':(SLOW,DELAY+1.0,True)}
# Not served (404): /missing/big.zip.
CA=ROOT/'ca.pem';cakey=ROOT/'ca.key';leaf=ROOT/'leaf.pem';leafkey=ROOT/'leaf.key';csr=ROOT/'leaf.csr';ext=ROOT/'leaf.ext'
ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
certargs=[['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(cakey),'-out',str(CA),'-days','1','-subj','/CN=Loopback Fixture CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign'],
 ['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(leafkey),'-out',str(csr),'-subj','/CN=Loopback Server'],
 ['openssl','x509','-req','-in',str(csr),'-CA',str(CA),'-CAkey',str(cakey),'-CAcreateserial','-out',str(leaf),'-days','1','-extfile',str(ext)]]
for args in certargs:subprocess.run(args,capture_output=True,check=True)
class Origin(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  row=dict(server='origin',path=self.path,method='GET',metadata=self.headers.get('Range')=='bytes=0-0',start=time.monotonic());REQUESTS.append(row)
  data,delay,length=SERVE.get(self.path,(b'missing',DELAY,True));code=200 if self.path in SERVE else 404
  time.sleep(delay)
  if not length:
   # No length announced: HTTP/1.1 chunked, as a streaming server sends it.
   self.protocol_version='HTTP/1.1';self.close_connection=True
   self.send_response(code);self.send_header('Content-Type','application/zip');self.send_header('Transfer-Encoding','chunked');self.send_header('Connection','close');self.end_headers()
   for i in range(0,len(data),64):
    part=data[i:i+64];self.wfile.write(b'%x\r\n'%len(part)+part+b'\r\n')
   self.wfile.write(b'0\r\n\r\n');row['end']=time.monotonic();return
  self.send_response(code);self.send_header('Content-Type','application/zip');self.send_header('Content-Length',str(len(data)))
  self.end_headers();self.wfile.write(data);row['end']=time.monotonic()
 def do_HEAD(self):
  t=time.monotonic();REQUESTS.append(dict(server='origin',path=self.path,method='HEAD',metadata=True,start=t,end=t));self.send_error(501)
 def log_message(self,*args):pass
ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey)
srv=http.server.ThreadingHTTPServer(('127.0.0.1',0),Origin);srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
threading.Thread(target=srv.serve_forever,daemon=True).start();BASE=f'https://127.0.0.1:{srv.server_port}'
API_KEY='fixture-api-key-7f3c';FILES_DELAY=3.0
class Api(http.server.BaseHTTPRequestHandler):
 """Synthetic Nexus API (as the H1a proof): validate, file metadata and download_link."""
 def do_GET(self):
  REQUESTS.append(dict(server='api',path=self.path,method='GET',metadata=False,start=time.monotonic()))
  rest=self.path.split('?')[0].strip('/').split('/',1)[1]
  if rest=='users/validate.json':body={'name':'fixture','is_premium':True}
  elif rest.endswith('download_link.json'):body=[{'URI':BASE+'/nexus-cdn/example-archive.zip'}]
  elif rest.endswith('/files.json'):
   # The file list a bare mod page (no file_id) is expanded from during execution-time
   # discovery; slow, so a signal can land while discovery waits for it.
   time.sleep(FILES_DELAY);body={'files':[{'file_id':7,'file_name':'example-archive.zip','category_name':'MAIN'}]}
  elif rest.startswith('games/'):body={'file_name':'example-archive.zip'}
  else:body=None
  data=json.dumps(body).encode();self.send_response(200 if body is not None else 404)
  self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*args):pass
api=http.server.ThreadingHTTPServer(('127.0.0.1',0),Api)
# A cancelled discovery closes its file-list request early; that broken pipe is expected.
api.handle_error=lambda request,address:None
threading.Thread(target=api.serve_forever,daemon=True).start()
API=f'http://127.0.0.1:{api.server_port}/v1/'
GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222']
PAGE='https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1'
def comp(i,url,filename='example.zip',sha256=None):
 # filename=None authors no Files map: the name is revealed only by the transfer.
 meta={'Files':{filename:True}} if filename is not None else {}
 if sha256:meta['ContentHashSHA256']=sha256
 return {'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':True,'Instructions':[{'Action':'Extract','Source':[f"<<modDirectory>>/{filename or 'example.zip'}"]}],'ResourceRegistry':{url:meta}}
def setup(name,components):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components}))
 (sd/'settings.json').write_text(json.dumps({'peerSharing':False,'usePublicPeerList':False,'telemetryOptIn':False}))
 return dict(name=name,case=case,guide=guide,mods=mods,settings=sd,scratch=scratch,runs=0)
def components(c):
 data=json.loads(c['guide'].read_text());return data['Components'] if isinstance(data,dict) else data
def lines(text):
 rows=[]
 for line in text.splitlines():
  try:rows.append(json.loads(line))
  except Exception:pass
 return rows
def fetch(c,label,flags=(),interrupt_after=None):
 c['runs']+=1;name=f"{c['name']}_{c['runs']:02d}_{label}"
 args=[str(BIN),'fetch-downloads','--input',str(c['guide']),'--source-dir',str(c['mods']),'--settings-dir',str(c['settings']),'--scratch-dir',str(c['scratch']),'--peer-mode','off','--retries','0','--extra-ca-file',str(CA),'--progress-json',*flags]
 before=snapshot(c['case']);start=len(REQUESTS);began=time.monotonic();timing=dict(started=began)
 p=subprocess.Popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=ENV)
 if interrupt_after is not None:
  # SIGINT (Ctrl+C) as a user would send it, a fixed time after the process started.
  try:p.wait(timeout=interrupt_after)
  except subprocess.TimeoutExpired:
   p.send_signal(signal.SIGINT);timing['signal']=time.monotonic()
 out,err=p.communicate(timeout=180);timing['exited']=time.monotonic()
 rows=lines(out);data=next((x.get('summary') or x.get('plan') for x in rows if x.get('type') in ('summary','plan')),None)
 events=[x['event'] for x in rows if x.get('type')=='acquisition']
 row=dict(name=name,argv=args,exit=p.returncode,stdout=out,stderr=err,data=data,events=events,requests=REQUESTS[start:],before=before,after=snapshot(c['case']),timing=timing)
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2)+'\n');RUNS.append(row)
 check(name+'_scratch_clean',not any(k.startswith('scratch/') for k in row['after']))
 check(name+'_no_temp_left',sorted(k for k in row['after'] if k.startswith('mods/.') or k.endswith('.tmp')),[])
 return row
def asked(r,path,server='origin'):return sum(1 for x in r['requests'] if x['server']==server and x['path']==path and not x['metadata'])
def outcomes(r):return (r['data'] or {}).get('report',{}).get('outcomes',[])
def statuses(r):return [(o['targetIndex'],o['componentGuid'],o['status'],o.get('sharedWith')) for o in outcomes(r)]
def errors(r):return [o.get('error') for o in outcomes(r)]
def files(r):return [((o.get('path') or '').rsplit('/',1)[-1],o.get('sha256')) for o in outcomes(r)]
def recorded(r):return ((r['data'] or {}).get('recordSummary') or {}).get('recorded')
def terminal(r):
 last={}
 for e in r['events']:last[e['targetIndex']]=e['phase']['phase']
 return [last[i] for i in sorted(last)]
def mods(r):return {k:v for k,v in r['after'].items() if k.startswith('mods/')}
def identity(path):st=os.stat(path);return (st.st_ino,st.st_mtime_ns,st.st_size)
def stream(r):return [(e['targetIndex'],e['phase']['phase']) for e in r['events']]
def transfers(r,i):return [(e['phase']['bytes'],e['phase'].get('total')) for e in r['events'] if e['targetIndex']==i and e['phase']['phase']=='transferring']
def plan_of(c,label):
 r=fetch(c,label,['--dry-run']);check(r['name']+'_no_request',r['requests'],[]);check(r['name']+'_no_writes',r['after'],r['before'])
 return r['data'] or {}

# 1. Known name after an unnamed publish, outside any group, --workers 1 (item 4).
# A (no Files, U1) and B (Files example.zip, U2) are not linked: different URLs, and A's
# name is unknown when planning. A runs first and publishes example.zip. B then transfers
# U2 (the same bytes, no hash), finds its name present and adopts that file, as an
# unnamed target already does. Before this change B failed before any request with the
# untyped "Download destination already exists".
u1=BASE+'/seq-unnamed/example.zip';u2=BASE+'/seq-known/example.zip'
c=setup('known_after_unnamed',[comp(0,u1,None),comp(1,u2)])
plan=plan_of(c,'plan');check('known_after_unnamed_plan',[(t.get('filename'),t.get('sharedDestination')) for t in plan.get('targets',[])],[(None,None),('example.zip',None)])
r=fetch(c,'fetch',['--workers','1','--record'])
check('known_after_unnamed_exit',r['exit'],0)
check('known_after_unnamed_requests',(asked(r,'/seq-unnamed/example.zip'),asked(r,'/seq-known/example.zip')),(1,1))
seq=[[x['start'] for x in r['requests'] if x['path']==p and not x['metadata']] for p in ('/seq-unnamed/example.zip','/seq-known/example.zip')]
check('known_after_unnamed_serialized',bool(seq[0] and seq[1]) and seq[1][0]-seq[0][0]>=DELAY)
check('known_after_unnamed_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('known_after_unnamed_errors',errors(r),[None,None])
check('known_after_unnamed_same_file',files(r),[('example.zip',A_SHA)]*2)
check('known_after_unnamed_terminal',terminal(r),['completed','completed'])
check('known_after_unnamed_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip']])
check('known_after_unnamed_file',mods(r),{'mods/example.zip':A_SHA})

# 2. The same order, but U2 serves other bytes and B authors no hash: B transfers, then
# refuses the present file with the typed adoption error, and the file is kept.
u1=BASE+'/mis-unnamed/example.zip';u2=BASE+'/mis-known/example.zip'
c=setup('known_after_unnamed_mismatch',[comp(0,u1,None),comp(1,u2)])
r=fetch(c,'fetch',['--workers','1','--record'])
check('known_mismatch_exit',r['exit'],1)
check('known_mismatch_requests',(asked(r,'/mis-unnamed/example.zip'),asked(r,'/mis-known/example.zip')),(1,1))
check('known_mismatch_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',None)])
e=(errors(r)[1:2] or [''])[0] or ''
check('known_mismatch_error',['differs from the downloaded bytes' in e,'no ContentHashSHA256' in e,'was not replaced' in e,'Download destination already exists' in e],[True,True,True,False])
check('known_mismatch_terminal',terminal(r),['completed','sourceFailed'])
check('known_mismatch_recorded',recorded(r),[[GUIDS[0],u1,'example.zip']])
check('known_mismatch_file',mods(r),{'mods/example.zip':A_SHA})

# 3. A known name loses a concurrent no-clobber publish, outside any group, --workers 4
# (item 4). A (no Files, U1) and B (Files example.zip, U2) serve the same slow archive;
# U2 answers 1 s later. B ends its transfer while A still verifies, so B finds its name
# absent and starts verifying (its `verifying` event comes before A's `completed`). A then
# publishes, and B's publish fails AlreadyExists. B adopts A's file (same bytes) rather
# than failing with "File exists"; its `completed` comes after A's.
u1=BASE+'/conc-unnamed/example.zip';u2=BASE+'/conc-known/example.zip'
c=setup('known_loses_publish',[comp(0,u1,None),comp(1,u2)])
r=fetch(c,'fetch',['--workers','4','--record'])
check('known_loses_exit',r['exit'],0)
check('known_loses_requests',(asked(r,'/conc-unnamed/example.zip'),asked(r,'/conc-known/example.zip')),(1,1))
starts=[[x['start'] for x in r['requests'] if x['path']==p and not x['metadata']] for p in ('/conc-unnamed/example.zip','/conc-known/example.zip')]
check('known_loses_overlap',bool(starts[0] and starts[1]) and abs(starts[1][0]-starts[0][0])<DELAY)
s=stream(r)
def at(item):return s.index(item) if item in s else None
order=[at((1,'verifying')),at((0,'completed')),at((1,'completed'))]
check('known_loses_order',None not in order and order==sorted(order),True)
check('known_loses_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('known_loses_errors',errors(r),[None,None])
check('known_loses_same_file',files(r),[('example.zip',SLOW_SHA)]*2)
check('known_loses_terminal',terminal(r),['completed','completed'])
check('known_loses_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip']])
check('known_loses_file',mods(r),{'mods/example.zip':SLOW_SHA})

# 4. Unnamed Nexus target whose API-revealed name is present (item 3). The page URL names
# file_id 1 and the component authors no Files, so the plan target is unnamed; the API
# reveals example-archive.zip before the transfer. That file is already present.
nexus=['--nexus-api-base',API,'--nexus-mods-api-key',API_KEY]
for name,present,expect in [('nexus_present_same',NEXUS_ARCHIVE,'adopt'),('nexus_present_other',OTHER,'refuse')]:
 c=setup(name,[comp(0,PAGE,None)]);(c['mods']/'example-archive.zip').write_bytes(present);kept=identity(c['mods']/'example-archive.zip')
 plan=plan_of(c,'plan');check(name+'_plan_unnamed',[t.get('filename') for t in plan.get('targets',[])],[None])
 r=fetch(c,'fetch',['--workers','1','--record',*nexus])
 check(name+'_api_link',asked(r,'/v1/games/kotor/mods/1/files/1/download_link.json','api'),1)
 check(name+'_transferred',asked(r,'/nexus-cdn/example-archive.zip'),1)
 check(name+'_not_replaced',identity(c['mods']/'example-archive.zip'),kept)
 if expect=='adopt':
  check(name+'_exit',r['exit'],0)
  check(name+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None)]);check(name+'_errors',errors(r),[None])
  check(name+'_file_outcome',files(r),[('example-archive.zip',NEXUS_SHA)])
  check(name+'_terminal',terminal(r),['completed'])
  check(name+'_recorded',recorded(r),[[GUIDS[0],PAGE,'example-archive.zip']])
  check(name+'_saved_files',[x['ResourceRegistry'][PAGE].get('Files') for x in components(c)],[{'example-archive.zip':True}])
  check(name+'_file',mods(r),{'mods/example-archive.zip':NEXUS_SHA})
 else:
  check(name+'_exit',r['exit'],1)
  check(name+'_outcomes',statuses(r),[(0,GUIDS[0],'failed',None)])
  e=(errors(r) or [''])[0] or ''
  check(name+'_error',['differs from the downloaded bytes' in e,'was not replaced' in e,'Download destination already exists' in e],[True,True,False])
  check(name+'_terminal',terminal(r),['sourceFailed'])
  check(name+'_not_recorded',recorded(r) or [],[])
  check(name+'_file',mods(r),{'mods/example-archive.zip':hashlib.sha256(OTHER).hexdigest()})

# 5. Origin totals (item 5). With Content-Length, every transferring event carries that
# total and the last one reaches it. Without one (chunked), the total
# is null rather than invented, and the transfer still completes.
for name,path,total in [('total_known','/length/example.zip',len(A)),('total_unknown','/nolength/example.zip',None)]:
 c=setup(name,[comp(0,BASE+path)])
 r=fetch(c,'fetch',['--workers','1'])
 t=transfers(r,0)
 check(name+'_exit',r['exit'],0);check(name+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None)])
 check(name+'_has_transferring',len(t)>=1)
 check(name+'_totals',sorted({x[1] for x in t},key=str),[total])
 check(name+'_last_bytes',t[-1][0] if t else None,len(A))
# The Nexus run above transferred through the same path: its CDN total is its length.
nexus_t=transfers(next(x for x in RUNS if x['name'].startswith('nexus_present_same') and x['name'].endswith('fetch')),0)
check('total_nexus_cdn',sorted({x[1] for x in nexus_t},key=str),[len(NEXUS_ARCHIVE)])

# 6. Existing-file hash cancellation (item 2). A 512 MiB present file under the authored
# name, with a ContentHashSHA256 it does not match, is hashed during planning. A run
# without a signal shows how long that hash takes: since review follow-up 1 the hashed
# target refuses the present file with no request (the 404 origin is never asked), so the
# whole run is timed. A run that gets SIGINT 1 s in stops within 1 s of the signal with
# exit 130 (as a cancel during the run does), before any request, and leaves the file
# untouched.
url=BASE+'/missing/big.zip'
c=setup('hash_cancel',[comp(0,url,'big.zip','0'*64)])
big=c['mods']/'big.zip'
with open(big,'wb') as f:f.truncate(512<<20)
kept=identity(big)
r=fetch(c,'control',['--workers','1'])
control=r['timing']['exited']-r['timing']['started']
hash_seconds=control
check('hash_cancel_control_exit',r['exit'],1)
check('hash_cancel_control_no_request',asked(r,'/missing/big.zip'),0)
check('hash_cancel_control_hash_takes_seconds',hash_seconds>=3.0)
e=(errors(r) or [''])[0] or ''
check('hash_cancel_control_refused',['SHA-256 mismatch: the present file' in e,'was not replaced' in e],[True,True])
r=fetch(c,'cancelled',['--workers','1'],interrupt_after=1.0)
latency=r['timing']['exited']-r['timing'].get('signal',r['timing']['exited'])
check('hash_cancel_signalled','signal' in r['timing'])
check('hash_cancel_exit',r['exit'],130)
check('hash_cancel_prompt',latency<1.0)
check('hash_cancel_no_request',asked(r,'/missing/big.zip'),0)
check('hash_cancel_no_events',r['events'],[])
check('hash_cancel_not_replaced',identity(big),kept)
check('hash_cancel_saved_guide',components(c),[comp(0,url,'big.zip','0'*64)])

# 7. Hashed known names already present before their transfer (review follow-up 1). With
# an authored ContentHashSHA256, a file present under the name decides the outcome on its
# own (adopt on the hash, refuse otherwise), so Origin now settles it with no request. At
# 59e4e57 each of these transferred first, then reached the same outcome.
# 7a: A (no Files, U1) publishes example.zip first; B (Files example.zip, hash of A's bytes,
# U2) then adopts it without requesting U2.
u1=BASE+'/hseq-unnamed/example.zip';u2=BASE+'/hseq-known/example.zip'
c=setup('hashed_after_unnamed',[comp(0,u1,None),comp(1,u2,'example.zip',A_SHA)])
plan=plan_of(c,'plan');check('hashed_after_unnamed_plan',[(t.get('filename'),t.get('sharedDestination')) for t in plan.get('targets',[])],[(None,None),('example.zip',None)])
r=fetch(c,'fetch',['--workers','1','--record'])
check('hashed_after_unnamed_exit',r['exit'],0)
check('hashed_after_unnamed_requests',(asked(r,'/hseq-unnamed/example.zip'),asked(r,'/hseq-known/example.zip')),(1,0))
check('hashed_after_unnamed_no_transfer',transfers(r,1),[])
check('hashed_after_unnamed_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('hashed_after_unnamed_errors',errors(r),[None,None])
check('hashed_after_unnamed_same_file',files(r),[('example.zip',A_SHA)]*2)
check('hashed_after_unnamed_terminal',terminal(r),['completed','completed'])
check('hashed_after_unnamed_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip']])
check('hashed_after_unnamed_file',mods(r),{'mods/example.zip':A_SHA})
# 7b: the same order, but B's hash is of other bytes: refused, typed, with no request, and
# the present file is kept.
u1=BASE+'/hmis-unnamed/example.zip';u2=BASE+'/hmis-known/example.zip'
c=setup('hashed_after_unnamed_mismatch',[comp(0,u1,None),comp(1,u2,'example.zip',B_SHA)])
r=fetch(c,'fetch',['--workers','1','--record'])
check('hashed_mismatch_exit',r['exit'],1)
check('hashed_mismatch_requests',(asked(r,'/hmis-unnamed/example.zip'),asked(r,'/hmis-known/example.zip')),(1,0))
check('hashed_mismatch_no_transfer',transfers(r,1),[])
check('hashed_mismatch_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',None)])
e=(errors(r)[1:2] or [''])[0] or ''
check('hashed_mismatch_error',['SHA-256 mismatch: the present file' in e,'was not replaced' in e,'Download destination already exists' in e],[True,True,False])
check('hashed_mismatch_terminal',terminal(r),['completed','sourceFailed'])
check('hashed_mismatch_recorded',recorded(r),[[GUIDS[0],u1,'example.zip']])
check('hashed_mismatch_file',mods(r),{'mods/example.zip':A_SHA})
# 7c: a present file under the authored name that fails the plan's existing-file check
# (contract item 2): with a hash authored, it is refused at once again, typed.
url=BASE+'/hpresent/example.zip'
c=setup('hashed_present_mismatch',[comp(0,url,'example.zip',A_SHA)]);(c['mods']/'example.zip').write_bytes(OTHER);kept=identity(c['mods']/'example.zip')
plan=plan_of(c,'plan');check('hashed_present_plan',[t.get('filename') for t in plan.get('targets',[])],['example.zip'])
r=fetch(c,'fetch',['--workers','1','--record'])
check('hashed_present_exit',r['exit'],1)
check('hashed_present_no_request',asked(r,'/hpresent/example.zip'),0)
check('hashed_present_no_transfer',transfers(r,0),[])
check('hashed_present_outcomes',statuses(r),[(0,GUIDS[0],'failed',None)])
e=(errors(r) or [''])[0] or ''
check('hashed_present_error',['SHA-256 mismatch: the present file' in e,'was not replaced' in e,'Download destination already exists' in e],[True,True,False])
check('hashed_present_terminal',terminal(r),['sourceFailed'])
check('hashed_present_not_recorded',recorded(r) or [],[])
check('hashed_present_not_replaced',identity(c['mods']/'example.zip'),kept)
check('hashed_present_file',mods(r),{'mods/example.zip':hashlib.sha256(OTHER).hexdigest()})
# 7d: a hashed name that is absent when its transfer starts and taken while it verifies
# still goes through the race-safe publish: B transfers, loses the no-clobber move to A,
# and adopts A's file under its hash (as case 3, with a hash).
u1=BASE+'/hconc-unnamed/example.zip';u2=BASE+'/hconc-known/example.zip'
c=setup('hashed_loses_publish',[comp(0,u1,None),comp(1,u2,'example.zip',SLOW_SHA)])
r=fetch(c,'fetch',['--workers','4','--record'])
check('hashed_loses_exit',r['exit'],0)
check('hashed_loses_requests',(asked(r,'/hconc-unnamed/example.zip'),asked(r,'/hconc-known/example.zip')),(1,1))
starts=[[x['start'] for x in r['requests'] if x['path']==p and not x['metadata']] for p in ('/hconc-unnamed/example.zip','/hconc-known/example.zip')]
check('hashed_loses_overlap',bool(starts[0] and starts[1]) and abs(starts[1][0]-starts[0][0])<DELAY)
s=stream(r);order=[at((1,'verifying')),at((0,'completed')),at((1,'completed'))]
check('hashed_loses_order',None not in order and order==sorted(order),True)
check('hashed_loses_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('hashed_loses_errors',errors(r),[None,None])
check('hashed_loses_same_file',files(r),[('example.zip',SLOW_SHA)]*2)
check('hashed_loses_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip']])
check('hashed_loses_file',mods(r),{'mods/example.zip':SLOW_SHA})

# 8. Ctrl+C during execution-time discovery (review follow-up 3). A bare Nexus mod page
# (no file_id, no Files) is expanded from the API file list after planning; that list
# answers FILES_DELAY s late. The component extracts example-archive.zip, the one MAIN file.
PAGE2='https://www.nexusmods.com/kotor/mods/2?tab=files'
FILES2='/v1/games/kotor/mods/2/files.json'
def page_comp():
 return {'Guid':GUIDS[0],'Name':'Example Consumer 1','IsSelected':True,'Instructions':[{'Action':'Extract','Source':['<<modDirectory>>/example-archive.zip']}],'ResourceRegistry':{PAGE2:{}}}
# Control without a signal: discovery expands the page and the run completes.
c=setup('discover_control',[page_comp()])
plan=plan_of(c,'plan');check('discover_plan_unnamed',[t.get('filename') for t in plan.get('targets',[])],[None])
r=fetch(c,'fetch',['--workers','1',*nexus])
check('discover_control_exit',r['exit'],0)
check('discover_control_files_listed',asked(r,FILES2,'api'),1)
check('discover_control_link',asked(r,'/v1/games/kotor/mods/2/files/7/download_link.json','api'),1)
check('discover_control_outcomes',[(o['status'],o.get('filename')) for o in outcomes(r)],[('completed','example-archive.zip')])
check('discover_control_file',mods(r),{'mods/example-archive.zip':NEXUS_SHA})
# SIGINT 1 s in, while discovery waits for the file list: exit 130 (it exited 1 before),
# nothing requested after the list, no acquisition event, nothing written.
c=setup('discover_cancel',[page_comp()])
r=fetch(c,'cancelled',['--workers','1',*nexus],interrupt_after=1.0)
listed=[x['start'] for x in r['requests'] if x['server']=='api' and x['path']==FILES2]
check('discover_cancel_signalled','signal' in r['timing'])
check('discover_cancel_in_discovery',bool(listed) and listed[0]<r['timing'].get('signal',0))
check('discover_cancel_exit',r['exit'],130)
check('discover_cancel_no_link',[x['path'] for x in r['requests'] if 'download_link' in x['path'] or x['server']=='origin'],[])
check('discover_cancel_no_events',r['events'],[])
check('discover_cancel_no_writes',mods(r),{})
check('discover_cancel_saved_guide',components(c),[page_comp()])
discover_latency=r['timing']['exited']-r['timing'].get('signal',r['timing']['exited'])
# A real discovery error with no signal still exits 1 with its own message: a non-UTF-8
# name in the mods folder makes the page's filename selection refuse its inventory.
c=setup('discover_error',[page_comp()])
fd=os.open(bytes(c['mods'])+b'/bad-\xff.bin',os.O_WRONLY|os.O_CREAT,0o644);os.write(fd,b'x');os.close(fd)
r=fetch(c,'fetch',['--workers','1',*nexus])
check('discover_error_exit',r['exit'],1)
check('discover_error_files_listed',asked(r,FILES2,'api'),1)
check('discover_error_message','non-UTF-8' in r['stderr'],True)
check('discover_error_no_events',r['events'],[])

srv.shutdown();srv.server_close();api.shutdown();api.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),certificateCommands=certargs,responseDelaySeconds=DELAY,hashControlSeconds=hash_seconds,controlSeconds=control,cancelLatencySeconds=latency,discoverCancelLatencySeconds=discover_latency,requests=REQUESTS,checks=CHECKS,passed=sum(x['passed'] for x in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations','hashControlSeconds','cancelLatencySeconds','discoverCancelLatencySeconds']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('RESIDUALS_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(x['passed'] for x in CHECKS)
