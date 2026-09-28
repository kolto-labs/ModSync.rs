#!/usr/bin/env python3
"""FOMOD S2a production receipts: real fetch-downloads/validate/install in one isolated
network namespace with a loopback HTTPS origin. Generic fixtures only; no Rust harness."""
import atexit, hashlib, http.server, io, json, os, pathlib, shutil, ssl, subprocess, tempfile, threading, time, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fomod-s2a'))).resolve()
shutil.rmtree(OUT,ignore_errors=True);OUT.mkdir(parents=True)
BIN=pathlib.Path(os.environ.get('MODSYNC_BINARY', str(TARGET/'debug/modsync'))).resolve()
def digest(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def sources():return {str(p.relative_to(REPO)):digest(p) for p in sorted(list((REPO/'src').rglob('*.rs'))+[REPO/'Cargo.toml',REPO/'Cargo.lock'])}
BINARY_BEFORE=digest(BIN);SOURCE_BEFORE=sources()
NEWEST=max((REPO/p).stat().st_mtime for p in SOURCE_BEFORE)
assert os.environ['FOMOD_PARENT_NETNS'] != os.readlink('/proc/self/ns/net')
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
def git(*a):
 r=subprocess.run(['git','-C',str(REPO),*a],capture_output=True,text=True);return r.stdout.strip() if r.returncode==0 else None
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-s2a-'))
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
def plugin(name,source,dest='Override/example.txt'):
 return f'<plugin name="{name}"><description>Generic option</description><files><file source="{source}" destination="{dest}"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
def package(names=('Alpha','Beta'),contents=('alpha\n','beta\n'),dests=('Override/example.txt','Override/example.txt')):
 xml='<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'+plugin(names[0],'payload/a.txt',dests[0])+plugin(names[1],'payload/b.txt',dests[1])+'</plugins></group></optionalFileGroups></installStep></installSteps></config>'
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml),('payload/a.txt',contents[0]),('payload/b.txt',contents[1])]:
   z.writestr(zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0)),value)
 return b.getvalue()
V1=package()
V2_RENAMED=package(names=('Gamma','Delta'),contents=('gamma v2\n','delta v2\n'))
V2_COMPATIBLE=package(contents=('alpha v2\n','beta v2\n'))
# Same names, but Beta now maps outside the game tree: applying succeeds, mapping fails.
V2_UNSAFE_MAP=package(contents=('alpha v2\n','beta v2\n'),dests=('Override/example.txt','../escape.txt'))
SERVE={'/example.zip':V1,'/second.zip':V1,'/renamed/example.zip':V2_RENAMED,'/compatible/example.zip':V2_COMPATIBLE}
key=ROOT/'loopback.key';cert=ROOT/'loopback.pem'
certargs=['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','1','-subj','/CN=Loopback Fixture','-addext','subjectAltName=IP:127.0.0.1','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign']
subprocess.run(certargs,capture_output=True,check=True)
leafkey=ROOT/'leaf.key';leaf=ROOT/'leaf.pem';csr=ROOT/'leaf.csr';ext=ROOT/'leaf.ext'
ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
leafargs=[['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(leafkey),'-out',str(csr),'-subj','/CN=Loopback Server'],['openssl','x509','-req','-in',str(csr),'-CA',str(cert),'-CAkey',str(key),'-CAcreateserial','-out',str(leaf),'-days','1','-extfile',str(ext)]]
for args in leafargs:subprocess.run(args,capture_output=True,check=True)
class Origin(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  REQUESTS.append(dict(path=self.path,time=time.monotonic()))
  data=SERVE.get(self.path);code=200 if data is not None else 404;data=data if data is not None else b'missing'
  self.send_response(code);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*args):pass
srv=http.server.ThreadingHTTPServer(('127.0.0.1',0),Origin);ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey);srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
threading.Thread(target=srv.serve_forever,daemon=True).start();BASE=f'https://127.0.0.1:{srv.server_port}'
GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222']
EXTRACT={'Action':'Extract','Source':['<<modDirectory>>/example.zip']}
def choices(plugin='Beta',filename='example.zip'):
 return {'version':1,'archives':[{'archiveFileName':filename,'selections':[{'stepName':'Choose','groupName':'Variant','plugins':[plugin]}]}]}
def resource(path,filename,status=None):
 meta={'Files':{filename:True},'custom':'keep'}
 if status:meta['handlerMetadata']={'fomodPromptStatus':{filename:status},'unrelated':'keep'}
 return {BASE+path:meta}
def comp(i=0,path='/example.zip',status=None,saved=None,instructions=(EXTRACT,)):
 c={'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':True,'Instructions':list(instructions),'ResourceRegistry':resource(path,'example.zip',status),'CustomField':'keep'}
 if saved is not None:c['FomodChoices']=saved
 return c
def setup(name,components,existing):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components}))
 (sd/'settings.json').write_text(json.dumps({'peerSharing':False,'usePublicPeerList':False,'telemetryOptIn':False}))
 for filename,data in existing.items():(mods/filename).write_bytes(data)
 return dict(name=name,case=case,guide=guide,mods=mods,settings=sd,scratch=scratch,runs=0)
def comps_of(raw):
 data=json.loads(raw);return data['Components'] if isinstance(data,dict) else data
def components(c):return comps_of(c['guide'].read_text())
def execute(c,label,args,text='',env=None):
 c['runs']+=1;name=f"{c['name']}_{c['runs']:02d}_{label}";before=snapshot(c['case'])
 shutil.copytree(c['case'],OUT/'fixtures'/name/'before',dirs_exist_ok=True)
 r=subprocess.run(args,input=text,capture_output=True,text=True,env=ENV|(env or {}),timeout=60)
 shutil.copytree(c['case'],OUT/'fixtures'/name/'after',dirs_exist_ok=True)
 try:data=json.loads(r.stdout)
 except Exception:data=None
 row=dict(name=name,argv=args,env=env or {},exit=r.returncode,stdout=r.stdout,stderr=r.stderr,data=data,before=before,after=snapshot(c['case']))
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2)+'\n');RUNS.append(row);return row
def fetch(c,label,flags=(),sidecar=None,env=None):
 args=[str(BIN),'fetch-downloads','--input',str(c['guide']),'--source-dir',str(c['mods']),'--settings-dir',str(c['settings']),'--scratch-dir',str(c['scratch']),'--peer-mode','off','--retries','0','--extra-ca-file',str(cert),*flags]
 if sidecar is not None:
  path=c['case']/f"choices-{c['runs']+1:02d}.json";path.write_text(sidecar if isinstance(sidecar,str) else json.dumps(sidecar));args+=['--fomod-choices',str(path)]
 r=execute(c,label,args,env=env)
 r['consumers']=[(v['componentGuid'],v['archive'].rsplit('/',1)[-1],v['outcome']) for v in (r['data'] or {}).get('fomodConfiguration',{}).get('consumers',[])]
 check(r['name']+'_scratch_clean',not any(p.startswith('scratch/') for p in r['after']))
 return r
def outcomes(r):return [o for _,_,o in r['consumers']]
def validate(c,label):
 (c['case']/'unused-game'/'Override').mkdir(parents=True,exist_ok=True)
 r=execute(c,label,[str(BIN),'validate','--input',str(c['guide']),'--game-dir',str(c['case']/'unused-game'),'--source-dir',str(c['mods']),'--scratch-dir',str(c['scratch']),'--validation-report'])
 # Validation is pure: the guide, mod and game trees are byte-identical (scratch aside).
 kept=lambda t:{k:v for k,v in t.items() if not k.startswith('scratch/')}
 check(r['name']+'_trees_unchanged',kept(r['after']),kept(r['before']))
 return r
def env_choices(c,value):
 path=c['case']/'environment-choices.json';path.write_text(json.dumps(value));return {'MODSYNC_FOMOD_CHOICES':str(path)}
def config(r):return (r['data'] or {}).get('fomodConfiguration',{})
def fomod_stage(r):return {'messages':[],'hasWarnings':False,'passed':None}|next((s for s in (r['data'] or {}).get('stages',[]) if s['stage']=='FomodConfiguration'),{})
def install(c,label):
 # Install into private copies so repeated Fetch runs keep observing the downloaded tree.
 k=c['runs']+1;mods=c['case']/f'install-{k:02d}-mods';game=c['case']/f'install-{k:02d}-game'
 shutil.copytree(c['mods'],mods);game.mkdir()
 r=execute(c,label,[str(BIN),'install','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(mods),'--scratch-dir',str(c['scratch'])])
 r['game']={str(p.relative_to(game)):p.read_text() for p in sorted(game.rglob('*')) if p.is_file() and '.modsync' not in p.parts}
 return r

# 1. Same-archive reconfiguration replaces prior choices; another archive survives exactly.
other=choices('Alpha','second.zip')
a=comp(saved=other);a['ResourceRegistry'].update(resource('/second.zip','second.zip','configured'))
c=setup('reconfigure',[a],{'example.zip':V1,'second.zip':V1})
r=fetch(c,'first_configure',['--record'],choices('Beta'));check('reconfigure_first_exit',r['exit'],0);check('reconfigure_first_outcomes',outcomes(r),['configured','resolved'])
g1=c['guide'].read_bytes();saved1=components(c)[0]['FomodChoices']
check('reconfigure_first_entries',[x['archiveFileName'] for x in saved1['archives']],['second.zip','example.zip'])
check('reconfigure_first_other_exact',saved1['archives'][0],other['archives'][0])
r=fetch(c,'reconfigure_alpha',['--record'],choices('Alpha'));check('reconfigure_alpha_exit',r['exit'],0);check('reconfigure_alpha_outcomes',outcomes(r),['reconfigured','resolved'])
check('reconfigure_alpha_session',[s['choices'] for s in r['data']['fomodConfiguration']['acceptedSessions']],[choices('Alpha')['archives'][0]])
g2=c['guide'].read_bytes();saved2=components(c)[0]['FomodChoices']
check('reconfigure_alpha_single_entry',[x['archiveFileName'] for x in saved2['archives']].count('example.zip'),1)
check('reconfigure_alpha_replaced',[x for x in saved2['archives'] if x['archiveFileName']=='example.zip'],choices('Alpha')['archives'])
check('reconfigure_alpha_other_exact',[x for x in saved2['archives'] if x['archiveFileName']=='second.zip'],other['archives'])
check('reconfigure_alpha_only_choice_changed',{k:v for k,v in components(c)[0].items() if k!='FomodChoices'},{k:v for k,v in comps_of(g1)[0].items() if k!='FomodChoices'})
check('reconfigure_alpha_guide_changed',g2!=g1)
ri=install(c,'install_alpha');check('reconfigure_install_alpha_exit',ri['exit'],0);check('reconfigure_install_alpha_bytes',ri['game'],{'Override/a.txt':'alpha\n'})
r=fetch(c,'reconfigure_beta',['--record'],choices('Beta'));check('reconfigure_beta_exit',r['exit'],0);check('reconfigure_beta_outcomes',outcomes(r),['reconfigured','resolved'])
g3=c['guide'].read_bytes();check('reconfigure_beta_guide_bytes_equal_first',digest(c['guide']),hashlib.sha256(g1).hexdigest())
r=fetch(c,'same_choices_noop',['--record'],choices('Beta'));check('reconfigure_noop_exit',r['exit'],0);check('reconfigure_noop_outcomes',outcomes(r),['resolved','resolved']);check('reconfigure_noop_sessions',r['data']['fomodConfiguration']['acceptedSessions'],[]);check('reconfigure_noop_guide_bytes',digest(c['guide']),hashlib.sha256(g3).hexdigest())
r=fetch(c,'no_record_reconfigure',[],choices('Alpha'));check('reconfigure_norecord_exit',r['exit'],0);check('reconfigure_norecord_outcomes',outcomes(r),['reconfigured','resolved']);check('reconfigure_norecord_guide_unchanged',digest(c['guide']),hashlib.sha256(g3).hexdigest())
r=fetch(c,'missing_file_not_needed',['--record','--fomod-choices',str(c['case']/'missing.json')]);check('reconfigure_lazy_missing_exit',r['exit'],0);check('reconfigure_lazy_missing_outcomes',outcomes(r),['resolved','resolved']);check('reconfigure_lazy_missing_errors',config(r).get('errors'),[]);check('reconfigure_lazy_missing_guide_unchanged',digest(c['guide']),hashlib.sha256(g3).hexdigest())
r=fetch(c,'bare_after',['--non-interactive']);check('reconfigure_bare_exit',r['exit'],0);check('reconfigure_bare_outcomes',outcomes(r),['resolved','resolved'])
rv=validate(c,'validate');check('reconfigure_validate_exit',rv['exit'],0)
# second.zip is registered and configured but has no Extract here: only it is reported.
check('reconfigure_validate_route_warning_scope',[("'second.zip'" in m,"'example.zip'" in m) for m in fomod_stage(rv)['messages']],[(True,False)])
ri=install(c,'install_beta');check('reconfigure_install_beta_exit',ri['exit'],0);check('reconfigure_install_beta_bytes',ri['game'].get('Override/b.txt'),'beta\n');check('reconfigure_install_beta_no_stale_alpha','Override/a.txt' in ri['game'],False)
(OUT/'reconfigure-guide-diff.json').write_text(json.dumps(dict(first=json.loads(g1),alpha=json.loads(g2),beta=json.loads(g3)),indent=2)+'\n')
# Two consumers share an archive with different saved choices: one session reconfigures both.
a=comp(saved=choices('Alpha'));b=comp(1,saved=choices('Beta'));c=setup('shared_reconfigure',[a,b],{'example.zip':V1})
r=fetch(c,'reconfigure',['--record'],choices('Beta'));check('shared_exit',r['exit'],0);check('shared_outcomes',outcomes(r),['reconfigured','resolved'])
check('shared_session_scope',[s['componentGuids'] for s in r['data']['fomodConfiguration']['acceptedSessions']],[[GUIDS[0]]])
check('shared_saved',[x['FomodChoices'] for x in components(c)],[choices('Beta'),choices('Beta')])
# Invalid explicit entry against a valid saved configuration fails atomically.
# Nothing is committed, so both valid consumers stay resolved and usable; the failed
# entry is a run error, and follow-ups/unresolved agree with the consumer state.
r=fetch(c,'invalid_explicit',['--record'],choices('Unknown'));check('shared_invalid_exit',r['exit'],1);check('shared_invalid_outcomes',outcomes(r),['resolved','resolved']);check('shared_invalid_guide_unchanged',r['before']['guide.json'],r['after']['guide.json'])
fc=config(r);check('shared_invalid_usable',[x['usableChoices'] for x in fc['consumers']],[True,True]);check('shared_invalid_consumer_errors',[x['error'] for x in fc['consumers']],[None,None])
check('shared_invalid_run_error',[("Choices file entry for 'example.zip' does not apply" in e) for e in fc['errors']],[True]);check('shared_invalid_unresolved',fc['unresolved'],0);check('shared_invalid_no_followups',r['data']['fomodFollowups']['followups'],[])

# SPEC3: only --fomod-choices reconfigures valid saved choices. MODSYNC_FOMOD_CHOICES keeps
# its S1 role (pending consumers only), and --fomod-skip wins over the flag (S1 precedence).
c=setup('environment',[comp(saved=choices('Alpha'))],{'example.zip':V1})
r=fetch(c,'record_baseline',['--record','--non-interactive']);check('env_baseline_outcomes',outcomes(r),['resolved']);genv=c['guide'].read_bytes()
r=fetch(c,'env_no_reconfigure',['--record'],env=env_choices(c,choices('Beta')));check('env_valid_exit',r['exit'],0);check('env_valid_outcomes',outcomes(r),['resolved']);check('env_valid_sessions',config(r)['acceptedSessions'],[]);check('env_valid_guide_unchanged',digest(c['guide']),hashlib.sha256(genv).hexdigest())
r=fetch(c,'skip_blocks_flag',['--record','--fomod-skip'],choices('Beta'));check('skip_flag_exit',r['exit'],0);check('skip_flag_outcomes',outcomes(r),['resolved']);check('skip_flag_sessions',config(r)['acceptedSessions'],[]);check('skip_flag_guide_unchanged',digest(c['guide']),hashlib.sha256(genv).hexdigest())
r=fetch(c,'flag_reconfigures',['--record'],choices('Beta'));check('flag_control_exit',r['exit'],0);check('flag_control_outcomes',outcomes(r),['reconfigured']);check('flag_control_saved',components(c)[0]['FomodChoices'],choices('Beta'))
# The environment still configures a pending consumer (S1), leaving the valid one alone.
c=setup('environment_pending',[comp(saved=choices('Alpha')),comp(1)],{'example.zip':V1})
r=fetch(c,'env_pending_only',['--record'],env=env_choices(c,choices('Beta')));check('env_exit',r['exit'],0);check('env_outcomes',outcomes(r),['resolved','configured'])
check('env_saved',[x.get('FomodChoices') for x in components(c)],[choices('Alpha'),choices('Beta')]);check('env_session_scope',[s['componentGuids'] for s in config(r)['acceptedSessions']],[[GUIDS[1]]])

# 2. Archive replaced with different installer names: stale saved choices are refused.
c=setup('stale',[comp(path='/renamed/example.zip',status='configured',saved=choices('Beta'))],{})
r=fetch(c,'fetch_refuses',['--record','--non-interactive']);check('stale_fetch_exit',r['exit'],1);check('stale_fetch_outcomes',outcomes(r),['error'])
error=r['data']['fomodConfiguration']['consumers'][0]['error'] or ''
check('stale_fetch_actionable','do not apply to the current archive' in error and '--fomod-choices' in error and 'fomod-inspect' in error)
check('stale_fetch_actual_download',any(x['path']=='/renamed/example.zip' for x in REQUESTS));check('stale_fetch_choices_kept',components(c)[0]['FomodChoices'],choices('Beta'))
r=fetch(c,'fetch_interactive_refuses',['--interactive']);check('stale_interactive_exit',r['exit'],1);check('stale_interactive_outcomes',outcomes(r),['error']);check('stale_interactive_no_prompt','Configure FOMOD' not in r['stderr'])
rv=validate(c,'validate_refuses');check('stale_validate_exit',rv['exit'],1);st=fomod_stage(rv);check('stale_validate_failed',st['passed'],False)
check('stale_validate_actionable',any('do not apply to the current archive' in m and '--fomod-choices' in m for m in st['messages']))
ri=install(c,'install_refuses');check('stale_install_exit',ri['exit'],1);check('stale_install_actionable','do not apply to the current archive' in ri['stderr']);check('stale_install_game_untouched',ri['game'],{})
missing=c['case']/'missing-choices.json'
r=fetch(c,'missing_choices_file',['--record','--fomod-choices',str(missing)]);check('stale_missing_file_exit',r['exit'],1);check('stale_missing_file_outcomes',outcomes(r),['error'])
check('stale_missing_file_reported',[('Reading FOMOD choices' in e and str(missing) in e) for e in config(r).get('errors',[])],[True]);check('stale_missing_file_choices_kept',components(c)[0]['FomodChoices'],choices('Beta'))
r=fetch(c,'malformed_choices_file',['--record'],'{invalid');bad=r['argv'][-1];check('stale_malformed_file_exit',r['exit'],1);check('stale_malformed_file_outcomes',outcomes(r),['error'])
check('stale_malformed_file_reported',[('Parsing FOMOD choices' in e and bad in e) for e in config(r).get('errors',[])],[True]);check('stale_malformed_file_choices_kept',components(c)[0]['FomodChoices'],choices('Beta'))
r=fetch(c,'recover_with_choices',['--record'],choices('Delta'));check('stale_recover_exit',r['exit'],0);check('stale_recover_outcomes',outcomes(r),['reconfigured'])
check('stale_recover_saved',components(c)[0]['FomodChoices'],choices('Delta'))
rv=validate(c,'validate_recovered');check('stale_recovered_validate_exit',rv['exit'],0)
ri=install(c,'install_recovered');check('stale_recovered_install_exit',ri['exit'],0);check('stale_recovered_install_bytes',ri['game'],{'Override/b.txt':'delta v2\n'})
# Stale saved choices are an S1 error, not a pending consumer: the environment never recovers them.
c=setup('stale_env',[comp(path='/renamed/example.zip',status='configured',saved=choices('Beta'))],{})
r=fetch(c,'env_does_not_recover',['--record'],env=env_choices(c,choices('Delta')));check('stale_env_exit',r['exit'],1);check('stale_env_outcomes',outcomes(r),['error']);check('stale_env_choices_kept',components(c)[0]['FomodChoices'],choices('Beta'))
# Positive control: a replacement whose installer still accepts the saved names is
# mapped fresh from the current archive bytes, never from the earlier archive.
c=setup('compatible',[comp(path='/compatible/example.zip',status='configured',saved=choices('Beta'))],{})
r=fetch(c,'fetch_resolves',['--record','--non-interactive']);check('compatible_fetch_exit',r['exit'],0);check('compatible_fetch_outcomes',outcomes(r),['resolved'])
rv=validate(c,'validate');check('compatible_validate_exit',rv['exit'],0)
ri=install(c,'install_current_bytes');check('compatible_install_exit',ri['exit'],0);check('compatible_install_bytes',ri['game'],{'Override/b.txt':'beta v2\n'})

# 3. C#-style configured-only guide: status plus generated instructions, no FomodChoices.
CS_COPY={'Action':'Copy','Source':['<<modDirectory>>/example/payload/b.txt'],'Destination':'<<kotorDirectory>>/Override'}
c=setup('csharp_configured',[comp(status='configured',instructions=(EXTRACT,CS_COPY))],{'example.zip':V1})
rv=validate(c,'validate_requires_choices');check('csharp_validate_exit',rv['exit'],1);st=fomod_stage(rv)
check('csharp_validate_actionable',any('marked configured' in m and 'not executable choices' in m and '--fomod-choices' in m for m in st['messages']))
ri=install(c,'install_requires_choices');check('csharp_install_exit',ri['exit'],1);check('csharp_install_actionable','marked configured' in ri['stderr'] and '--fomod-choices' in ri['stderr']);check('csharp_install_game_untouched',ri['game'],{})
r=fetch(c,'fetch_interactive',['--interactive']);check('csharp_interactive_exit',r['exit'],3);check('csharp_interactive_outcomes',outcomes(r),['suppressed']);check('csharp_interactive_actionable','marked configured' in r['stderr'] and '--fomod-choices' in r['stderr']);check('csharp_interactive_no_prompt','Configure FOMOD' not in r['stderr']);check('csharp_interactive_guide_unchanged',r['before']['guide.json'],r['after']['guide.json'])
r=fetch(c,'fetch_warn',['--non-interactive']);check('csharp_warn_exit',r['exit'],3);check('csharp_warn_outcomes',outcomes(r),['warned']);check('csharp_warn_actionable','marked configured' in r['stderr'] and '--fomod-choices' in r['stderr']);check('csharp_warn_guide_unchanged',r['before']['guide.json'],r['after']['guide.json'])
r=fetch(c,'import_choices',['--record'],choices('Beta'));check('csharp_import_exit',r['exit'],0);check('csharp_import_outcomes',outcomes(r),['configured'])
after=components(c)[0];check('csharp_import_saved',after['FomodChoices'],choices('Beta'));check('csharp_import_instructions_kept',[{k:i[k] for k in ('Action','Source','Destination') if k in i} for i in after['Instructions']],[{'Action':'Extract','Source':EXTRACT['Source'],'Destination':''},CS_COPY])
rv=validate(c,'validate_imported');check('csharp_imported_validate_exit',rv['exit'],0)
ri=install(c,'install_imported');check('csharp_imported_install_exit',ri['exit'],0);check('csharp_imported_install_bytes',ri['game'],{'Override/b.txt':'beta\n'})

# 4. Registration without Extract: gate validity is distinct from an install route.
NOTE={'Action':'Copy','Source':['<<modDirectory>>/notes.txt'],'Destination':'<<kotorDirectory>>/Override'}
c=setup('registered_only',[comp(status='configured',saved=choices('Beta'),instructions=(NOTE,))],{'example.zip':V1,'notes.txt':b'generic note\n'})
rv=validate(c,'validate_warns');check('registered_validate_exit',rv['exit'],0);st=fomod_stage(rv)
check('registered_validate_passed',st['passed'],True);check('registered_validate_warning',st['hasWarnings'],True);check('registered_warning_count',rv['data']['warningCount'],1)
check('registered_warning_text',any('no selected Extract instruction installs it' in m and 'installs no files' in m for m in st['messages']))
ri=install(c,'install_no_files');check('registered_install_exit',ri['exit'],0);check('registered_install_no_fomod_files',ri['game'],{'Override/notes.txt':'generic note\n'})
c=setup('registered_only_missing',[comp(instructions=())],{'example.zip':V1})
rv=validate(c,'validate_requires_choices');check('registered_missing_exit',rv['exit'],1);check('registered_missing_actionable',any('no explicit FomodChoices' in m and '--fomod-choices' in m for m in fomod_stage(rv)['messages']))
c=setup('registered_only_stale',[comp(saved=choices('Unknown'),instructions=())],{'example.zip':V1})
rv=validate(c,'validate_refuses_invalid');check('registered_invalid_exit',rv['exit'],1);check('registered_invalid_actionable',any('do not apply to the current archive' in m for m in fomod_stage(rv)['messages']))
# Two registrants of one archive without Extract: one warning naming both.
a=comp(status='configured',saved=choices('Beta'),instructions=(NOTE,));b=comp(1,status='configured',saved=choices('Beta'),instructions=())
c=setup('registered_only_shared',[a,b],{'example.zip':V1,'notes.txt':b'generic note\n'})
rv=validate(c,'validate_warns_once');check('registered_shared_exit',rv['exit'],0);check('registered_shared_warning_count',rv['data']['warningCount'],1)
check('registered_shared_warning_names',[("'Example Consumer 1', 'Example Consumer 2'" in m) for m in fomod_stage(rv)['messages']],[True])
# Saved names still apply, but the current archive maps the selection unsafely: every
# route refuses with the recovery hint, including install's mapping step.
c=setup('stale_mapping',[comp(status='configured',saved=choices('Beta'))],{'example.zip':V2_UNSAFE_MAP})
rv=validate(c,'validate_refuses');check('stale_mapping_validate_exit',rv['exit'],1);check('stale_mapping_validate_actionable',any('do not apply to the current archive' in m for m in fomod_stage(rv)['messages']))
ri=install(c,'install_refuses');check('stale_mapping_install_exit',ri['exit'],1);check('stale_mapping_install_actionable','do not apply to the current archive' in ri['stderr'] and 'fomod-inspect' in ri['stderr']);check('stale_mapping_install_game_untouched',ri['game'],{})
# Positive control: the same registration with an Extract route has no warning and installs.
c=setup('registered_with_extract',[comp(status='configured',saved=choices('Beta'))],{'example.zip':V1})
rv=validate(c,'validate_clean');check('extract_validate_exit',rv['exit'],0);check('extract_validate_no_warning',fomod_stage(rv)['hasWarnings'],False);check('extract_warning_count',rv['data']['warningCount'],0)
ri=install(c,'install_files');check('extract_install_exit',ri['exit'],0);check('extract_install_bytes',ri['game'],{'Override/b.txt':'beta\n'})

srv.shutdown();srv.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),certificateCommand=certargs,leafCertificateCommands=leafargs,requests=REQUESTS,checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FOMOD_S2A_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
