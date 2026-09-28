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
 '/hash/example.zip':PLAIN_A,'/interleaved/example.zip':PLAIN_A,'/interleaved/second.zip':SECOND,'/names/example.zip':PLAIN_A,'/case/example.zip':PLAIN_A,
 '/ha/example.zip':PLAIN_A,'/hb/example.zip':PLAIN_B,'/d/example.zip':PLAIN_B,'/hash-owner/example.zip':PLAIN_A,'/unknown/example.zip':PLAIN_A,
 '/unknown-first/example.zip':PLAIN_A,'/chain-y/example.zip':PLAIN_B,'/trans-u1/example.zip':PLAIN_A,'/trans-u2/example.zip':PLAIN_B,
 '/named-u1/example.zip':PLAIN_A,'/named-u2/example.zip':PLAIN_A,'/seq-u1/example.zip':PLAIN_A,'/seq-u2/example.zip':PLAIN_A,'/seq-other/example.zip':PLAIN_B,
 '/rerun/example.zip':PLAIN_A}
# Not served (404): /missing/example.zip, /missing-a/example.zip, /chain-x/example.zip,
# /names-404/example.zip, /unnamed-404/example.zip.
# Every response, 404 included, is held briefly, so two concurrent requests overlap (as
# before the fix with --workers 4) and a request made only after another has answered
# starts at least DELAY after it.
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
  time.sleep(DELAY)
  self.send_response(code);self.send_header('Content-Type','application/zip');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  row['end']=time.monotonic()
 def log_message(self,*args):pass
srv=http.server.ThreadingHTTPServer(('127.0.0.1',0),Origin);ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey);srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
threading.Thread(target=srv.serve_forever,daemon=True).start();BASE=f'https://127.0.0.1:{srv.server_port}'
GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222','cccccccc-3333-4333-8333-333333333333','dddddddd-4444-4444-8444-444444444444']
# Report JSON before this change: every key must survive, and non-shared rows gain none.
TARGET_KEYS=['componentGuid','registryUrl','filename','expectedSha256','sources']
OUTCOME_KEYS=['targetIndex','componentGuid','registryUrl','filename','status','source','path','sha256','manual','error']
def choices(plugin='Beta',filename='example.zip'):
 return {'version':1,'archives':[{'archiveFileName':filename,'selections':[{'stepName':'Choose','groupName':'Variant','plugins':[plugin]}]}]}
def comp(i,path,filename='example.zip',sha256=None,flag=True,url=None):
 # filename=None authors no Files map: the name is revealed only by the transfer.
 meta={'Files':{filename:flag}} if filename is not None else {}
 meta['custom']='keep'
 if sha256:meta['ContentHashSHA256']=sha256
 return {'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':True,'Instructions':[{'Action':'Extract','Source':[f"<<modDirectory>>/{filename or 'example.zip'}"]}],'ResourceRegistry':{url or BASE+path:meta},'CustomField':'keep'}
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
def errors(r):return [o.get('error') for o in outcomes(r)]
def events_of(r,i):return [e['phase']['phase'] for e in r['events'] if e['targetIndex']==i]
def mods(r):return {k:v for k,v in r['after'].items() if k.startswith('mods/')}
def starts(r,path):return [x['start'] for x in r['requests'] if x['path']==path]
def terminal(r):
 # The last event of each target index: (component, phase), in index order.
 last={}
 for e in r['events']:last[e['targetIndex']]=(e['componentGuid'],e['phase']['phase'])
 return [last[i] for i in sorted(last)]
def replan(c,label):
 # A dry run: the whole plan (targets and skips), with no request and no writes.
 r=fetch(c,label,['--dry-run']);check(r['name']+'_no_request',r['requests'],[]);check(r['name']+'_no_writes',r['after'],r['before'])
 return r['data'] or {}
def dry(c,label):return replan(c,label).get('targets',[])
def present(plan):return [(s['componentGuid'],s['reason'],(s.get('path') or '').rsplit('/',1)[-1]) for s in plan.get('skipped',[])]
def identity(path):
 # The file itself, not only its bytes: a replaced file gets a new inode.
 st=os.stat(path);return (st.st_ino,st.st_mtime_ns,st.st_size)

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

# 3. Different URLs, one file name, the owner succeeds: the later target takes the
# published file by name (as a present file would be taken), with no request of its own.
# Null Files flags allow the file; recording turns each completed owner's flag true.
urla=BASE+'/a/example.zip';urlb=BASE+'/b/example.zip';c=setup('different_urls',[comp(0,'/a/example.zip',flag=None),comp(1,'/b/example.zip',flag=None)])
targets=dry(c,'plan');check('different_plan_linked',[t.get('sharedDestination') for t in targets],[None,{'kind':'differentUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('different_exit',r['exit'],0);check('different_requests',(asked(r,'/a/example.zip'),asked(r,'/b/example.zip')),(1,0))
check('different_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
check('different_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',hashlib.sha256(PLAIN_A).hexdigest())]*2)
check('different_follower_event',events_of(r,1),['completed'])
check('different_recorded',recorded(r),[[GUIDS[0],urla,'example.zip'],[GUIDS[1],urlb,'example.zip']]);check('different_files_flags',[x['ResourceRegistry'][u]['Files'] for x,u in zip(components(c),[urla,urlb])],[{'example.zip':True}]*2)
check('different_file_from_first',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest()})

# 4. The same, but the later target authors the hash of its own URL's bytes: the owner's
# file fails that hash, so it fails without a request and is not recorded.
urla=BASE+'/ha/example.zip';urlb=BASE+'/hb/example.zip';c=setup('different_hash',[comp(0,'/ha/example.zip'),comp(1,'/hb/example.zip',sha256=hashlib.sha256(PLAIN_B).hexdigest())])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('different_hash_exit',r['exit'],1);check('different_hash_requests',(asked(r,'/ha/example.zip'),asked(r,'/hb/example.zip')),(1,0))
check('different_hash_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',0)])
check('different_hash_error','SHA-256 mismatch: the shared download' in ((errors(r)[1:2] or [''])[0] or ''))
check('different_hash_recorded',recorded(r),[[GUIDS[0],urla,'example.zip']]);check('different_hash_file',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest()})

# 5. Different URLs, one file name, the owner's URL is missing (404): the later target
# then runs its own URL, after the owner in the same worker, and is recorded.
urla=BASE+'/missing-a/example.zip';urlb=BASE+'/d/example.zip';c=setup('different_owner_404',[comp(0,'/missing-a/example.zip'),comp(1,'/d/example.zip')])
targets=dry(c,'plan');check('different_owner_404_plan',[t.get('sharedDestination') for t in targets],[None,{'kind':'differentUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('different_owner_404_exit',r['exit'],1);check('different_owner_404_requests',(asked(r,'/missing-a/example.zip'),asked(r,'/d/example.zip')),(1,1))
check('different_owner_404_serialized',[b-a>=DELAY for a,b in zip(starts(r,'/missing-a/example.zip'),starts(r,'/d/example.zip'))],[True])
check('different_owner_404_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'completed',None)])
check('different_owner_404_terminal',terminal(r),[(GUIDS[0],'sourceFailed'),(GUIDS[1],'completed')])
check('different_owner_404_recorded',recorded(r),[[GUIDS[1],urlb,'example.zip']]);check('different_owner_404_file',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_B).hexdigest()})

# 6. A sharing component's own authored hash is checked against the shared bytes.
c=setup('hash_follower',[comp(0,'/hash/example.zip'),comp(1,'/hash/example.zip',sha256='0'*64)]);url=BASE+'/hash/example.zip'
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('hash_exit',r['exit'],1);check('hash_one_request',asked(r,'/hash/example.zip'),1)
check('hash_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',0)])
check('hash_error','SHA-256 mismatch: the shared download' in ((outcomes(r)[1:2] or [{}])[0].get('error') or ''))
check('hash_recorded',recorded(r),[[GUIDS[0],url,'example.zip']])

# 7. Guide order and indices hold when the sharing target is not adjacent to its owner.
c=setup('interleaved',[comp(0,'/interleaved/example.zip'),comp(1,'/interleaved/second.zip','second.zip'),comp(2,'/interleaved/example.zip')])
urlx=BASE+'/interleaved/example.zip';urly=BASE+'/interleaved/second.zip'
targets=dry(c,'plan');check('interleaved_plan',[t.get('sharedDestination') for t in targets],[None,None,{'kind':'sameUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('interleaved_exit',r['exit'],0);check('interleaved_requests',(asked(r,'/interleaved/example.zip'),asked(r,'/interleaved/second.zip')),(1,1))
check('interleaved_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None),(2,GUIDS[2],'completed',0)])
check('interleaved_recorded',recorded(r),[[GUIDS[0],urlx,'example.zip'],[GUIDS[1],urly,'second.zip'],[GUIDS[2],urlx,'example.zip']])
check('interleaved_terminal_events',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'completed'),(GUIDS[2],'completed')])

# 8. Control: one URL, two different names are two destinations, so nothing is merged.
c=setup('distinct_names',[comp(0,'/names/example.zip'),comp(1,'/names/example.zip','copy.zip')]);url=BASE+'/names/example.zip'
targets=dry(c,'plan');check('distinct_plan_unshared',[sorted(t) for t in targets],[sorted(TARGET_KEYS)]*2)
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('distinct_exit',r['exit'],0);check('distinct_two_requests',asked(r,'/names/example.zip'),2)
check('distinct_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('distinct_files',{k:v for k,v in r['after'].items() if k.startswith('mods/')},{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest(),'mods/copy.zip':hashlib.sha256(PLAIN_A).hexdigest()})
check('distinct_recorded',recorded(r),[[GUIDS[0],url,'example.zip'],[GUIDS[1],url,'copy.zip']])

# 9. Known names compare ASCII case-insensitively (like existing-file detection and the
# engine's source matching): one file, and each component records its own key.
c=setup('case_variant',[comp(0,'/case/example.zip','Example.zip'),comp(1,'/case/example.zip')]);url=BASE+'/case/example.zip'
targets=dry(c,'plan');check('case_plan_shared',[t.get('sharedDestination') for t in targets],[None,{'kind':'sameUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('case_exit',r['exit'],0);check('case_one_request',asked(r,'/case/example.zip'),1)
check('case_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
check('case_one_file',{k:v for k,v in r['after'].items() if k.startswith('mods/')},{'mods/Example.zip':hashlib.sha256(PLAIN_A).hexdigest()})
check('case_recorded',recorded(r),[[GUIDS[0],url,'Example.zip'],[GUIDS[1],url,'example.zip']])

# 10. One URL and file name, the URL is missing (404): the owner's one request fails, and
# the later target, whose source list is identical, takes that outcome without a request.
url=BASE+'/missing/example.zip';c=setup('same_404',[comp(0,'/missing/example.zip'),comp(1,'/missing/example.zip')])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('same_404_exit',r['exit'],1);check('same_404_one_request',asked(r,'/missing/example.zip'),1)
check('same_404_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'failed',0)])
check('same_404_same_error',[bool(e) and e==errors(r)[0] for e in errors(r)],[True,True])
check('same_404_follower_event',events_of(r,1),['sourceFailed']);check('same_404_recorded',recorded(r),[]);check('same_404_no_file',mods(r),{})

# 11. One manual-host URL (not automated) and file name: host classification needs no
# network, and the namespace has no route, so any request would fail instead. Both
# targets are manualRequired; the later one takes the owner's outcome.
url='https://deadlystream.com/files/file/0000-example-mod/';c=setup('same_manual',[comp(0,None,url=url),comp(1,None,url=url)])
targets=dry(c,'plan');check('same_manual_plan',[t.get('sharedDestination') for t in targets],[None,{'kind':'sameUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--progress-json'])
# No `no_request` check here: the loopback origin cannot see a request to this host, so
# such a check would be vacuous. The outcomes and events below are the evidence: with no
# route, any attempt would end `failed`/`sourceFailed`, never `manualRequired`.
check('same_manual_exit',r['exit'],3)
check('same_manual_outcomes',statuses(r),[(0,GUIDS[0],'manualRequired',None),(1,GUIDS[1],'manualRequired',0)])
check('same_manual_manual',[o.get('manual') for o in outcomes(r)],[{'url':url,'reason':'unsupported'}]*2)
check('same_manual_events',[events_of(r,0),events_of(r,1)],[['resolving','manualRequired'],['manualRequired']]);check('same_manual_no_file',mods(r),{})

# 12. The owner authors a wrong hash and the later target none, one URL: the owner's one
# transfer fails its hash, and the later target, whose source list is identical, takes
# that failure without a second request (the owner's hash governs the one transfer).
url=BASE+'/hash-owner/example.zip';c=setup('hash_owner',[comp(0,'/hash-owner/example.zip',sha256='0'*64),comp(1,'/hash-owner/example.zip')])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('hash_owner_exit',r['exit'],1);check('hash_owner_one_request',asked(r,'/hash-owner/example.zip'),1)
check('hash_owner_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'failed',0)])
check('hash_owner_same_error',['SHA-256 mismatch; download discarded' in (e or '') and e==errors(r)[0] for e in errors(r)],[True,True])
check('hash_owner_recorded',recorded(r),[]);check('hash_owner_no_file',mods(r),{})

# 13. One URL, a Files name first and none second: the name the transfer would reveal
# comes from the same URL, so the second target takes the first one's file.
url=BASE+'/unknown/example.zip';c=setup('unknown_follower',[comp(0,'/unknown/example.zip'),comp(1,'/unknown/example.zip',None)])
targets=dry(c,'plan');check('unknown_follower_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[('example.zip',None),(None,{'kind':'sameUrl','targetIndex':0})])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('unknown_follower_exit',r['exit'],0);check('unknown_follower_one_request',asked(r,'/unknown/example.zip'),1)
check('unknown_follower_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
check('unknown_follower_recorded',recorded(r),[[GUIDS[0],url,'example.zip'],[GUIDS[1],url,'example.zip']])
check('unknown_follower_saved_files',[x['ResourceRegistry'][url].get('Files') for x in components(c)],[{'example.zip':True}]*2)
check('unknown_follower_one_file',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest()})

# 14. The reverse order: no Files name first, a Files name second. The second runs after
# the first and takes its file when the revealed name is its own.
url=BASE+'/unknown-first/example.zip';c=setup('unknown_first',[comp(0,'/unknown-first/example.zip',None),comp(1,'/unknown-first/example.zip')])
targets=dry(c,'plan');check('unknown_first_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[(None,None),('example.zip',{'kind':'sameUrl','targetIndex':0})])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('unknown_first_exit',r['exit'],0);check('unknown_first_one_request',asked(r,'/unknown-first/example.zip'),1)
check('unknown_first_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0)])
check('unknown_first_recorded',recorded(r),[[GUIDS[0],url,'example.zip'],[GUIDS[1],url,'example.zip']])
check('unknown_first_one_file',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_A).hexdigest()})

# 15. A link through a linked target: A (name, URL X), B (same name, URL Y) and C (no
# name, URL Y) form one group owned by A. X is missing, so B runs its own URL, and C,
# from B's URL, takes B's file (sharedWith 1).
urlx=BASE+'/chain-x/example.zip';urly=BASE+'/chain-y/example.zip';c=setup('chain',[comp(0,'/chain-x/example.zip'),comp(1,'/chain-y/example.zip'),comp(2,'/chain-y/example.zip',None)])
targets=dry(c,'plan');check('chain_plan',[t.get('sharedDestination') for t in targets],[None,{'kind':'differentUrl','targetIndex':0},{'kind':'differentUrl','targetIndex':0}])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('chain_exit',r['exit'],1);check('chain_requests',(asked(r,'/chain-x/example.zip'),asked(r,'/chain-y/example.zip')),(1,1))
check('chain_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'completed',None),(2,GUIDS[2],'completed',1)])
check('chain_recorded',recorded(r),[[GUIDS[1],urly,'example.zip'],[GUIDS[2],urly,'example.zip']])
check('chain_one_file',mods(r),{'mods/example.zip':hashlib.sha256(PLAIN_B).hexdigest()})

# 16. Transitive groups. A target may share a file with two earlier targets that do not
# share one with each other; all three must be one group, or two of them race.
# (a) B (no Files, URL U2), A (example.zip, URL U1), C (example.zip, URL U2): C shares
# B's URL and A's name. B owns the group and publishes example.zip from U2; A and C then
# take that file by name without a request, as a rerun takes a present file
# (AlreadyPresent). The rerun's dry-run plan shows exactly that.
u1=BASE+'/trans-u1/example.zip';u2=BASE+'/trans-u2/example.zip';B_SHA=hashlib.sha256(PLAIN_B).hexdigest();A_SHA=hashlib.sha256(PLAIN_A).hexdigest()
for workers in ['4','1']:
 n=f'transitive_unnamed_first_w{workers}';c=setup(n,[comp(0,'/trans-u2/example.zip',None),comp(1,'/trans-u1/example.zip'),comp(2,'/trans-u2/example.zip')])
 targets=dry(c,'plan');check(n+'_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[(None,None),('example.zip',{'kind':'differentUrl','targetIndex':0}),('example.zip',{'kind':'sameUrl','targetIndex':0})])
 r=fetch(c,'fetch',['--workers',workers,'--record','--progress-json'])
 check(n+'_exit',r['exit'],0);check(n+'_requests',(asked(r,'/trans-u1/example.zip'),asked(r,'/trans-u2/example.zip')),(0,1))
 check(n+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',0),(2,GUIDS[2],'completed',0)])
 check(n+'_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',B_SHA)]*3)
 check(n+'_follower_events',[events_of(r,1),events_of(r,2)],[['completed'],['completed']])
 check(n+'_errors',errors(r),[None]*3)
 check(n+'_recorded',recorded(r),[[GUIDS[0],u2,'example.zip'],[GUIDS[1],u1,'example.zip'],[GUIDS[2],u2,'example.zip']])
 check(n+'_file',mods(r),{'mods/example.zip':B_SHA})
 again=replan(c,'rerun');check(n+'_rerun_targets',again.get('targets'),[])
 check(n+'_rerun_present',present(again),[(g,'alreadyPresent','example.zip') for g in GUIDS[:3]])
# (b) The other order: A (example.zip, U1), B (no Files, U2), C (example.zip, U2). A owns
# the group and publishes example.zip from U1. B, unnamed, must run U2 itself, after A in
# the same worker. Its transfer reveals example.zip, which is present: B adopts that file
# when it holds exactly the bytes B downloaded (no hash is authored), as a named target
# takes a present file (AlreadyPresent), and never replaces it. C takes A's file by name
# without a request. Here U1 and U2 serve the same bytes.
u1=BASE+'/named-u1/example.zip';u2=BASE+'/named-u2/example.zip'
for workers in ['4','1']:
 n=f'transitive_named_first_w{workers}';c=setup(n,[comp(0,'/named-u1/example.zip'),comp(1,'/named-u2/example.zip',None),comp(2,'/named-u2/example.zip')])
 targets=dry(c,'plan');check(n+'_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[('example.zip',None),(None,{'kind':'differentUrl','targetIndex':0}),('example.zip',{'kind':'differentUrl','targetIndex':0})])
 r=fetch(c,'fetch',['--workers',workers,'--record','--progress-json'])
 check(n+'_exit',r['exit'],0);check(n+'_requests',(asked(r,'/named-u1/example.zip'),asked(r,'/named-u2/example.zip')),(1,1))
 check(n+'_serialized',[b-a>=DELAY for a,b in zip(starts(r,'/named-u1/example.zip'),starts(r,'/named-u2/example.zip'))],[True])
 check(n+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None),(2,GUIDS[2],'completed',0)])
 check(n+'_errors',errors(r),[None]*3)
 check(n+'_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',A_SHA)]*3)
 check(n+'_terminal',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'completed'),(GUIDS[2],'completed')])
 check(n+'_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip'],[GUIDS[2],u2,'example.zip']])
 check(n+'_file',mods(r),{'mods/example.zip':A_SHA})
 # Recording gave B its revealed name, so a rerun finds all three present by name.
 again=replan(c,'rerun');check(n+'_rerun_targets',again.get('targets'),[])
 check(n+'_rerun_present',present(again),[(g,'alreadyPresent','example.zip') for g in GUIDS[:3]])
# (c) The same order, but U2 serves other bytes than U1 and B authors no hash: nothing
# says which bytes B wants, so B fails with a clear error after its one request, and the
# present file keeps A's bytes. No temp file is left. A rerun still plans B (unnamed).
u1=BASE+'/trans-u1/example.zip';u2=BASE+'/trans-u2/example.zip'
for workers in ['4','1']:
 n=f'transitive_named_first_mismatch_w{workers}';c=setup(n,[comp(0,'/trans-u1/example.zip'),comp(1,'/trans-u2/example.zip',None),comp(2,'/trans-u2/example.zip')])
 targets=dry(c,'plan');check(n+'_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[('example.zip',None),(None,{'kind':'differentUrl','targetIndex':0}),('example.zip',{'kind':'differentUrl','targetIndex':0})])
 r=fetch(c,'fetch',['--workers',workers,'--record','--progress-json'])
 check(n+'_exit',r['exit'],1);check(n+'_requests',(asked(r,'/trans-u1/example.zip'),asked(r,'/trans-u2/example.zip')),(1,1))
 check(n+'_serialized',[b-a>=DELAY for a,b in zip(starts(r,'/trans-u1/example.zip'),starts(r,'/trans-u2/example.zip'))],[True])
 check(n+'_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',None),(2,GUIDS[2],'completed',0)])
 e=(errors(r)[1:2] or [''])[0] or ''
 check(n+'_unnamed_error',['differs from the downloaded bytes' in e,'no ContentHashSHA256' in e,'was not replaced' in e],[True]*3)
 check(n+'_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)[::2]],[('example.zip',A_SHA)]*2)
 check(n+'_terminal',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'sourceFailed'),(GUIDS[2],'completed')])
 check(n+'_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[2],u2,'example.zip']])
 check(n+'_file',mods(r),{'mods/example.zip':A_SHA})
 again=replan(c,'rerun');check(n+'_rerun_targets',[(t['componentGuid'],t.get('filename'),t.get('sharedDestination')) for t in again.get('targets',[])],[(GUIDS[1],None,None)])
 check(n+'_rerun_present',present(again),[(GUIDS[0],'alreadyPresent','example.zip'),(GUIDS[2],'alreadyPresent','example.zip')])

# 17. Failure inheritance needs the same name, or no name on both sides. A (a.zip), B (no
# Files), C (c.zip) and D (a.zip) on one missing (404) URL are one group through B. B
# asked for no name, and A asked for a.zip, which may fail for that name alone (a Nexus
# link is chosen by name), so B makes its own request. C knows a different name, so it
# does too. D asked for A's name with identical sources and inherits A's failure. The
# three requests run one after another.
url=BASE+'/names-404/example.zip';c=setup('names_404',[comp(0,'/names-404/example.zip','a.zip'),comp(1,'/names-404/example.zip',None),comp(2,'/names-404/example.zip','c.zip'),comp(3,'/names-404/example.zip','a.zip')])
targets=dry(c,'plan');check('names_404_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[('a.zip',None),(None,{'kind':'sameUrl','targetIndex':0}),('c.zip',{'kind':'sameUrl','targetIndex':0}),('a.zip',{'kind':'sameUrl','targetIndex':0})])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('names_404_exit',r['exit'],1);check('names_404_requests',asked(r,'/names-404/example.zip'),3)
check('names_404_serialized',[b-a>=DELAY for a,b in zip(starts(r,'/names-404/example.zip'),starts(r,'/names-404/example.zip')[1:])],[True,True])
check('names_404_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'failed',None),(2,GUIDS[2],'failed',None),(3,GUIDS[3],'failed',0)])
check('names_404_events',[events_of(r,1)[:1],events_of(r,1)[-1:],events_of(r,2)[:1],events_of(r,2)[-1:],events_of(r,3)],[['resolving'],['sourceFailed'],['resolving'],['sourceFailed'],['sourceFailed']])
check('names_404_recorded',recorded(r),[]);check('names_404_no_file',mods(r),{})
# Control: two unnamed targets with identical sources made the same request, so the
# second still inherits the first one's failure without a request.
url=BASE+'/unnamed-404/example.zip';c=setup('unnamed_404',[comp(0,'/unnamed-404/example.zip',None),comp(1,'/unnamed-404/example.zip',None)])
targets=dry(c,'plan');check('unnamed_404_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[(None,None),(None,{'kind':'sameUrl','targetIndex':0})])
r=fetch(c,'fetch',['--workers','4','--record','--progress-json'])
check('unnamed_404_exit',r['exit'],1);check('unnamed_404_one_request',asked(r,'/unnamed-404/example.zip'),1)
check('unnamed_404_outcomes',statuses(r),[(0,GUIDS[0],'failed',None),(1,GUIDS[1],'failed',0)])
check('unnamed_404_follower_event',events_of(r,1),['sourceFailed']);check('unnamed_404_no_file',mods(r),{})

# 18. Ungrouped and sequential: A (example.zip, U1) and B (no Files, U2) share no name and
# no URL, so they are not linked, and --workers 1 runs A, then B. B's transfer reveals
# example.zip, which A published. B authors the hash of U2's bytes, which equal A's, so it
# adopts the present file (subject to that hash), without replacing it.
u1=BASE+'/seq-u1/example.zip';u2=BASE+'/seq-u2/example.zip';c=setup('ungrouped_sequential',[comp(0,'/seq-u1/example.zip'),comp(1,'/seq-u2/example.zip',None,sha256=A_SHA)])
targets=dry(c,'plan');check('ungrouped_sequential_plan',[(t.get('filename'),t.get('sharedDestination')) for t in targets],[('example.zip',None),(None,None)])
r=fetch(c,'fetch',['--workers','1','--record','--progress-json'])
check('ungrouped_sequential_exit',r['exit'],0);check('ungrouped_sequential_requests',(asked(r,'/seq-u1/example.zip'),asked(r,'/seq-u2/example.zip')),(1,1))
check('ungrouped_sequential_serialized',[b-a>=DELAY for a,b in zip(starts(r,'/seq-u1/example.zip'),starts(r,'/seq-u2/example.zip'))],[True])
check('ungrouped_sequential_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'completed',None)])
check('ungrouped_sequential_errors',errors(r),[None]*2)
check('ungrouped_sequential_same_file',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',A_SHA)]*2)
check('ungrouped_sequential_terminal',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'completed')])
check('ungrouped_sequential_recorded',recorded(r),[[GUIDS[0],u1,'example.zip'],[GUIDS[1],u2,'example.zip']])
check('ungrouped_sequential_file',mods(r),{'mods/example.zip':A_SHA})
# The same, but U2 serves other bytes and B authors their hash: B's download matches its
# hash, the present file does not, so B fails and the present file is kept.
u2=BASE+'/seq-other/example.zip';c=setup('ungrouped_hash',[comp(0,'/seq-u1/example.zip'),comp(1,'/seq-other/example.zip',None,sha256=B_SHA)])
r=fetch(c,'fetch',['--workers','1','--record','--progress-json'])
check('ungrouped_hash_exit',r['exit'],1);check('ungrouped_hash_requests',(asked(r,'/seq-u1/example.zip'),asked(r,'/seq-other/example.zip')),(1,1))
check('ungrouped_hash_outcomes',statuses(r),[(0,GUIDS[0],'completed',None),(1,GUIDS[1],'failed',None)])
e=(errors(r)[1:2] or [''])[0] or ''
check('ungrouped_hash_error',['SHA-256 mismatch: the present file' in e,'was not replaced' in e],[True,True])
check('ungrouped_hash_terminal',terminal(r),[(GUIDS[0],'completed'),(GUIDS[1],'sourceFailed')])
check('ungrouped_hash_recorded',recorded(r),[[GUIDS[0],u1,'example.zip']]);check('ungrouped_hash_file',mods(r),{'mods/example.zip':A_SHA})

# 19. Rerun of an unnamed target (no Files, no hash). The first run downloads example.zip
# without --record, so the component stays unnamed. The second run transfers again (the
# name is unknown until then), finds example.zip present with the same bytes, adopts it
# without replacing it (same inode and mtime), and records it. A dry run after that
# finds it present by name.
url=BASE+'/rerun/example.zip';c=setup('unnamed_rerun',[comp(0,'/rerun/example.zip',None)])
r=fetch(c,'first',['--workers','4','--progress-json'])
check('unnamed_rerun_first',(r['exit'],asked(r,'/rerun/example.zip'),statuses(r)),(0,1,[(0,GUIDS[0],'completed',None)]))
kept=identity(c['mods']/'example.zip')
r=fetch(c,'again',['--workers','4','--record','--progress-json'])
check('unnamed_rerun_exit',r['exit'],0);check('unnamed_rerun_one_request',asked(r,'/rerun/example.zip'),1)
check('unnamed_rerun_outcomes',statuses(r),[(0,GUIDS[0],'completed',None)]);check('unnamed_rerun_errors',errors(r),[None])
check('unnamed_rerun_file_outcome',[((o['path'] or '').rsplit('/',1)[-1],o['sha256']) for o in outcomes(r)],[('example.zip',A_SHA)])
check('unnamed_rerun_terminal',terminal(r),[(GUIDS[0],'completed')])
check('unnamed_rerun_recorded',recorded(r),[[GUIDS[0],url,'example.zip']])
check('unnamed_rerun_saved_files',[x['ResourceRegistry'][url].get('Files') for x in components(c)],[{'example.zip':True}])
check('unnamed_rerun_not_replaced',identity(c['mods']/'example.zip'),kept)
check('unnamed_rerun_file',mods(r),{'mods/example.zip':A_SHA})
again=replan(c,'rerun');check('unnamed_rerun_rerun_targets',again.get('targets'),[])
check('unnamed_rerun_rerun_present',present(again),[(GUIDS[0],'alreadyPresent','example.zip')])

srv.shutdown();srv.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),certificateCommand=certargs,leafCertificateCommands=leafargs,responseDelaySeconds=DELAY,requests=REQUESTS,checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FETCH_SHARED_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
