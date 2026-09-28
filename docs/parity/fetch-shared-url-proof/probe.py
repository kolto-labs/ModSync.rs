#!/usr/bin/env python3
"""Fetch shared-URL production receipts: real fetch-downloads in one isolated network
namespace with a recording loopback HTTPS origin. Generic fixtures only; no Rust harness."""
import atexit, hashlib, http.server, io, json, os, pathlib, shutil, ssl, subprocess, tempfile, threading, time, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fetch-shared-url'))).resolve()
shutil.rmtree(OUT,ignore_errors=True);OUT.mkdir(parents=True)
BIN=pathlib.Path(os.environ.get('MODSYNC_BINARY', str(TARGET/'debug/modsync'))).resolve()
def digest(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def sources():return {str(p.relative_to(REPO)):digest(p) for p in sorted(list((REPO/'src').rglob('*.rs'))+[REPO/'Cargo.toml',REPO/'Cargo.lock'])}
BINARY_BEFORE=digest(BIN);SOURCE_BEFORE=sources()
NEWEST=max((REPO/p).stat().st_mtime for p in SOURCE_BEFORE)
assert os.environ['FETCH_PARENT_NETNS'] != os.readlink('/proc/self/ns/net')
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
def git(*a):
 r=subprocess.run(['git','-C',str(REPO),*a],capture_output=True,text=True);return r.stdout.strip() if r.returncode==0 else None
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fetch-shared-url-'))
# Receipts under OUT are kept; the private fixture root is removed unless kept on request.
if os.environ.get('MODSYNC_PROOF_KEEP')!='1':atexit.register(shutil.rmtree,ROOT,ignore_errors=True)
ENV={k:v for k,v in os.environ.items() if not k.lower().endswith('_proxy') and not any(t in k for t in ['NEXUS','MODSYNC_FOMOD'])}
for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
 ENV[k]=str(ROOT/k);pathlib.Path(ENV[k]).mkdir()
CHECKS=[];RUNS=[];REQUESTS=[]
def snapshot(p):return {str(f.relative_to(p)):digest(f) for f in sorted(p.rglob('*')) if f.is_file()}
def check(name,actual,expected=True):
 row=dict(name=name,actual=actual,expected=expected,passed=actual==expected);CHECKS.append(row)
 if not row['passed']:print('FAIL',name,repr(actual)[:400],flush=True)
# The S2a FOMOD archive generator: one step, one SelectExactlyOne group, two plugins.
def plugin(name,source,dest='Override/example.txt'):
 return f'<plugin name="{name}"><description>Generic option</description><files><file source="{source}" destination="{dest}"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
def package(names=('Alpha','Beta'),contents=('alpha\n','beta\n'),dests=('Override/example.txt','Override/example.txt')):
 xml='<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'+plugin(names[0],'payload/a.txt',dests[0])+plugin(names[1],'payload/b.txt',dests[1])+'</plugins></group></optionalFileGroups></installStep></installSteps></config>'
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml),('payload/a.txt',contents[0]),('payload/b.txt',contents[1])]:
   z.writestr(zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0)),value)
 return b.getvalue()
def plain(text):
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:z.writestr(zipfile.ZipInfo('payload.txt',date_time=(2020,1,1,0,0,0)),text)
 return b.getvalue()
FOMOD=package();PLAIN_A=plain('from url a\n');PLAIN_B=plain('from url b\n');SECOND=plain('second archive\n')
SERVE={'/control/example.zip':FOMOD,'/shared-w4/example.zip':FOMOD,'/shared-w1/example.zip':FOMOD,'/a/example.zip':PLAIN_A,'/b/example.zip':PLAIN_B,
 '/hash/example.zip':PLAIN_A,'/interleaved/example.zip':PLAIN_A,'/interleaved/second.zip':SECOND,'/names/example.zip':PLAIN_A,'/case/example.zip':PLAIN_A}
# Each archive response is held briefly, so two concurrent requests for one file overlap
# (as before the fix with --workers 4) instead of finishing one after the other.
DELAY=0.4
key=ROOT/'loopback.key';cert=ROOT/'loopback.pem'
certargs=['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','1','-subj','/CN=Loopback Fixture','-addext','subjectAltName=IP:127.0.0.1','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign']
subprocess.run(certargs,capture_output=True,check=True)
leafkey=ROOT/'leaf.key';leaf=ROOT/'leaf.pem';csr=ROOT/'leaf.csr';ext=ROOT/'leaf.ext'
ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
leafargs=[['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(leafkey),'-out',str(csr),'-subj','/CN=Loopback Server'],['openssl','x509','-req','-in',str(csr),'-CA',str(cert),'-CAkey',str(key),'-CAcreateserial','-out',str(leaf),'-days','1','-extfile',str(ext)]]
for args in leafargs:subprocess.run(args,capture_output=True,check=True)
class Origin(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  row=dict(path=self.path,start=time.monotonic(),thread=threading.get_ident());REQUESTS.append(row)
  data=SERVE.get(self.path);code=200 if data is not None else 404;data=data if data is not None else b'missing'
  if code==200:time.sleep(DELAY)
  self.send_response(code);self.send_header('Content-Type','application/zip');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  row['end']=time.monotonic()
 def log_message(self,*args):pass
srv=http.server.ThreadingHTTPServer(('127.0.0.1',0),Origin);ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey);srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
threading.Thread(target=srv.serve_forever,daemon=True).start();BASE=f'https://127.0.0.1:{srv.server_port}'
GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222','cccccccc-3333-4333-8333-333333333333']
# Report JSON before this change: every key must survive, and non-shared rows gain none.
TARGET_KEYS=['componentGuid','registryUrl','filename','expectedSha256','sources']
OUTCOME_KEYS=['targetIndex','componentGuid','registryUrl','filename','status','source','path','sha256','manual','error']
def choices(plugin='Beta',filename='example.zip'):
 return {'version':1,'archives':[{'archiveFileName':filename,'selections':[{'stepName':'Choose','groupName':'Variant','plugins':[plugin]}]}]}
def comp(i,path,filename='example.zip',sha256=None,flag=True):
 meta={'Files':{filename:flag},'custom':'keep'}
 if sha256:meta['ContentHashSHA256']=sha256
 return {'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':True,'Instructions':[{'Action':'Extract','Source':[f'<<modDirectory>>/{filename}']}],'ResourceRegistry':{BASE+path:meta},'CustomField':'keep'}
def setup(name,components):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components}))
 (sd/'settings.json').write_text(json.dumps({'peerSharing':False,'usePublicPeerList':False,'telemetryOptIn':False}))
 return dict(name=name,case=case,guide=guide,mods=mods,settings=sd,scratch=scratch,runs=0)
def comps_of(raw):
 data=json.loads(raw);return data['Components'] if isinstance(data,dict) else data
def components(c):return comps_of(c['guide'].read_text())
def lines(text):
 rows=[]
 for line in text.splitlines():
  try:rows.append(json.loads(line))
  except Exception:pass
 return rows
def fetch(c,label,flags=(),sidecar=None):
 c['runs']+=1;name=f"{c['name']}_{c['runs']:02d}_{label}"
 args=[str(BIN),'fetch-downloads','--input',str(c['guide']),'--source-dir',str(c['mods']),'--settings-dir',str(c['settings']),'--scratch-dir',str(c['scratch']),'--peer-mode','off','--retries','0','--extra-ca-file',str(cert),*flags]
 if sidecar is not None:
  path=c['case']/f"choices-{c['runs']:02d}.json";path.write_text(json.dumps(sidecar));args+=['--fomod-choices',str(path)]
 before=snapshot(c['case']);start=len(REQUESTS)
 shutil.copytree(c['case'],OUT/'fixtures'/name/'before',dirs_exist_ok=True)
 r=subprocess.run(args,capture_output=True,text=True,env=ENV,timeout=60)
 shutil.copytree(c['case'],OUT/'fixtures'/name/'after',dirs_exist_ok=True)
 rows=lines(r.stdout);data=None
 if '--progress-json' in flags:data=next((x.get('summary') or x.get('plan') for x in rows if x.get('type') in ('summary','plan')),None)
 else:
  try:data=json.loads(r.stdout)
  except Exception:data=None
 events=[x['event'] for x in rows if x.get('type')=='acquisition']
 row=dict(name=name,argv=args,exit=r.returncode,stdout=r.stdout,stderr=r.stderr,data=data,events=events,requests=REQUESTS[start:],before=before,after=snapshot(c['case']))
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2)+'\n');RUNS.append(row)
 check(name+'_scratch_clean',not any(p.startswith('scratch/') for p in row['after']))
 return row
def asked(r,path):return sum(1 for x in r['requests'] if x['path']==path)
def outcomes(r):return (r['data'] or {}).get('report',{}).get('outcomes',[])
def statuses(r):return [(o['targetIndex'],o['componentGuid'],o['status'],o.get('sharedWith')) for o in outcomes(r)]
def recorded(r):return ((r['data'] or {}).get('recordSummary') or {}).get('recorded')
def consumers(r):return [(v['componentGuid'],v['archive'].rsplit('/',1)[-1],v['outcome']) for v in (r['data'] or {}).get('fomodConfiguration',{}).get('consumers',[])]
def terminal(r):
 # The last event of each target index: (component, phase), in index order.
 last={}
 for e in r['events']:last[e['targetIndex']]=(e['componentGuid'],e['phase']['phase'])
 return [last[i] for i in sorted(last)]
def dry(c,label):
 r=fetch(c,label,['--dry-run']);check(r['name']+'_no_request',r['requests'],[]);check(r['name']+'_no_writes',r['after'],r['before'])
 return (r['data'] or {}).get('targets',[])

# 1. Positive control: one component, one FOMOD archive, no pre-placed file.
url=BASE+'/control/example.zip';c=setup('control',[comp(0,'/control/example.zip')])
targets=dry(c,'plan');check('control_plan_keys',[sorted(t) for t in targets],[sorted(TARGET_KEYS)])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'],choices('Beta'))
check('control_exit',r['exit'],0);check('control_one_request',asked(r,'/control/example.zip'),1)
check('control_outcomes',statuses(r),[(0,GUIDS[0],'completed',None)]);check('control_outcome_keys',[sorted(o) for o in outcomes(r)],[sorted(OUTCOME_KEYS)])
check('control_recorded',recorded(r),[[GUIDS[0],url,'example.zip']]);check('control_consumers',consumers(r),[(GUIDS[0],'example.zip','configured')])
check('control_terminal_events',terminal(r),[(GUIDS[0],'completed')]);check('control_archive_bytes',r['after'].get('mods/example.zip'),hashlib.sha256(FOMOD).hexdigest())
check('control_saved_choices',[x.get('FomodChoices') for x in components(c)],[choices('Beta')])

# 2. Two components name one URL and file, nothing pre-placed: one transfer, both owners.
for workers in ['4','1']:
 n=f'shared_w{workers}';path=f'/shared-w{workers}/example.zip';url=BASE+path;c=setup(n,[comp(0,path),comp(1,path)])
 targets=dry(c,'plan');check(n+'_plan_shared',[t.get('sharedDestination') for t in targets],[None,{'kind':'sameUrl','targetIndex':0}])
 check(n+'_plan_keys_additive',[sorted(set(t)-set(TARGET_KEYS)) for t in targets],[[],['sharedDestination']])
 r=fetch(c,'fetch',['--workers',workers,'--record','--progress-json'],choices('Beta'))
 check(n+'_exit',r['exit'],0);check(n+'_one_request',asked(r,path),1)
 check(n+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
 check(n+'_outcome_keys_additive',[sorted(set(o)-set(OUTCOME_KEYS)) for o in outcomes(r)],[[],['sharedWith']])
 check(n+'_outcome_keys_kept',[set(OUTCOME_KEYS)<=set(o) for o in outcomes(r)],[True,True])
 check(n+'_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',hashlib.sha256(FOMOD).hexdigest())]*2)
 check(n+'_recorded',recorded(r),[[GUIDS[0],url,'example.zip'],[GUIDS[1],url,'example.zip']])
 check(n+'_saved_files',[x['ResourceRegistry'][url]['Files'] for x in components(c)],[{'example.zip':True}]*2)
 check(n+'_consumers',consumers(r),[(GUIDS[0],'example.zip','configured'),(GUIDS[1],'example.zip','configured')])
 check(n+'_session_scope',[s['componentGuids'] for s in (r['data'] or {}).get('fomodConfiguration',{}).get('acceptedSessions',[])],[[GUIDS[0],GUIDS[1]]])
 check(n+'_saved_choices',[x.get('FomodChoices') for x in components(c)],[choices('Beta')]*2)
 check(n+'_terminal_events',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'completed')])
 check(n+'_follower_single_event',[(e['phase']['phase'],e['componentGuid']) for e in r['events'] if e['targetIndex']==1],[('completed',GUIDS[1])])
 check(n+'_mods_tree',{k:v for k,v in r['after'].items() if k.startswith('mods/')},{'mods/example.zip':hashlib.sha256(FOMOD).hexdigest()})

# 3. Different URLs, one destination: the later target fails explicitly, without a request.
# Null Files flags allow the file; recording would turn a completed owner's flag true.
urla=BASE+'/a/example.zip';urlb=BASE+'/b/example.zip';c=setup('different_urls',[comp(0,'/a/example.zip',flag=None),comp(1,'/b/example.zip',flag=None)])
targets=dry(c,'plan');check('different_plan_conflict',[t.get('sharedDestination') for t in targets],[None,{'kind':'differentUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('different_exit',r['exit'],1);check('different_requests',(asked(r,'/a/example.zip'),asked(r,'/b/example.zip')),(1,0))
check('different_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',None)])
error=(outcomes(r)[1:2] or [{}])[0].get('error') or ''
check('different_explicit_error',f'Download destination example.zip is also planned from {urla} for component {GUIDS[0]}; one file cannot come from two URLs, so {urlb} was not requested'==error)
check('different_not_a_race','already exists' not in error and 'File exists' not in error)
check('different_follower_event',[(e['phase']['phase'],e['phase'].get('will_fallback')) for e in r['events'] if e['targetIndex']==1],[('sourceFailed',False)])
check('different_recorded',recorded(r),[[GUIDS[0],urla,'example.zip']]);check('different_files_flags',[x['ResourceRegistry'][u]['Files'] for x,u in zip(components(c),[urla,urlb])],[{'example.zip':True},{'example.zip':None}])
check('different_file_from_first',r['after'].get('mods/example.zip'),hashlib.sha256(PLAIN_A).hexdigest())

# 4. A sharing component's own authored hash is checked against the shared bytes.
c=setup('hash_follower',[comp(0,'/hash/example.zip'),comp(1,'/hash/example.zip',sha256='0'*64)]);url=BASE+'/hash/example.zip'
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('hash_exit',r['exit'],1);check('hash_one_request',asked(r,'/hash/example.zip'),1)
check('hash_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',0)])
check('hash_error','SHA-256 mismatch: the shared download' in ((outcomes(r)[1:2] or [{}])[0].get('error') or ''))
check('hash_recorded',recorded(r),[[GUIDS[0],url,'example.zip']])

# 5. Guide order and indices hold when the sharing target is not adjacent to its owner.
c=setup('interleaved',[comp(0,'/interleaved/example.zip'),comp(1,'/interleaved/second.zip','second.zip'),comp(2,'/interleaved/example.zip')])
urlx=BASE+'/interleaved/example.zip';urly=BASE+'/interleaved/second.zip'
targets=dry(c,'plan');check('interleaved_plan',[t.get('sharedDestination') for t in targets],[None,None,{'kind':'sameUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('interleaved_exit',r['exit'],0);check('interleaved_requests',(asked(r,'/interleaved/example.zip'),asked(r,'/interleaved/second.zip')),(1,1))
check('interleaved_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None),(2,GUIDS[2],'completed',0)])
check('interleaved_recorded',recorded(r),[[GUIDS[0],urlx,'example.zip'],[GUIDS[1],urly,'second.zip'],[GUIDS[2],urlx,'example.zip']])
check('interleaved_terminal_events',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'completed'),(GUIDS[2],'completed')])

# 6. Control: one URL, two different names are two destinations, so nothing is merged.
c=setup('distinct_names',[comp(0,'/names/example.zip'),comp(1,'/names/example.zip','copy.zip')]);url=BASE+'/names/example.zip'
targets=dry(c,'plan');check('distinct_plan_unshared',[sorted(t) for t in targets],[sorted(TARGET_KEYS)]*2)
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('distinct_exit',r['exit'],0);check('distinct_two_requests',asked(r,'/names/example.zip'),2)
check('distinct_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('distinct_files',{k:v for k,v in r['after'].items() if k.startswith('mods/')},{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest(),'mods/copy.zip':hashlib.sha256(PLAIN_A).hexdigest()})
check('distinct_recorded',recorded(r),[[GUIDS[0],url,'example.zip'],[GUIDS[1],url,'copy.zip']])

# 7. Known names compare ASCII case-insensitively (like existing-file detection and the
# engine's source matching): one file, and each component records its own key.
c=setup('case_variant',[comp(0,'/case/example.zip','Example.zip'),comp(1,'/case/example.zip')]);url=BASE+'/case/example.zip'
targets=dry(c,'plan');check('case_plan_shared',[t.get('sharedDestination') for t in targets],[None,{'kind':'sameUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('case_exit',r['exit'],0);check('case_one_request',asked(r,'/case/example.zip'),1)
check('case_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
check('case_one_file',{k:v for k,v in r['after'].items() if k.startswith('mods/')},{'mods/Example.zip':hashlib.sha256(PLAIN_A).hexdigest()})
check('case_recorded',recorded(r),[[GUIDS[0],url,'Example.zip'],[GUIDS[1],url,'example.zip']])

srv.shutdown();srv.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),certificateCommand=certargs,leafCertificateCommands=leafargs,responseDelaySeconds=DELAY,requests=REQUESTS,checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FETCH_SHARED_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
