#!/usr/bin/env python3
"""Real production CLI/PTY receipts in one isolated network namespace. No Rust harness."""
import copy, shutil, fcntl, hashlib, http.server, io, json, os, pathlib, pty, select, signal, ssl, subprocess, tempfile, termios, threading, time, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
OUT=pathlib.Path(__file__).resolve().parent/'receipts';OUT.mkdir(exist_ok=True)
BIN=pathlib.Path('/home/brunner56/.cache/modsync-rs-target-lead/debug/modsync')
BINARY_BEFORE=hashlib.sha256(BIN.read_bytes()).hexdigest()
SOURCE_BEFORE={str(p.relative_to(REPO)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((REPO/'src').rglob('*.rs'))}
assert os.environ['FOMOD_PARENT_NETNS'] != os.readlink('/proc/self/ns/net')
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-s1-'))
ENV={k:v for k,v in os.environ.items() if not k.lower().endswith('_proxy') and not any(t in k for t in ['NEXUS','MODSYNC_FOMOD'])}
for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
 ENV[k]=str(ROOT/k);pathlib.Path(ENV[k]).mkdir()
CHECKS=[];RUNS=[];REQUESTS=[]
def digest(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def snapshot(p):return {str(f.relative_to(p)):digest(f) for f in sorted(p.rglob('*')) if f.is_file()}
def check(name,actual,expected=True):
 row=dict(name=name,actual=actual,expected=expected,passed=actual==expected);CHECKS.append(row)
 if not row['passed']:print('FAIL',name,repr(actual),flush=True)
def package(xml=None):
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml or XML),('payload/a.txt','alpha\n'),('payload/b.txt','beta\n')]:
   info=zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0));z.writestr(info,value)
 return b.getvalue()
XML='''<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins><plugin name="Alpha"><description>First variant</description><files><file source="payload/a.txt" destination="Override/example.txt"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin><plugin name="Beta"><description>Second variant</description><files><file source="payload/b.txt" destination="Override/example.txt"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>'''
PAYLOAD=package()
key=ROOT/'loopback.key';cert=ROOT/'loopback.pem'
certargs=['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','1','-subj','/CN=Loopback Fixture','-addext','subjectAltName=IP:127.0.0.1','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign']
r=subprocess.run(certargs,capture_output=True,check=True)
leafkey=ROOT/'leaf.key';leaf=ROOT/'leaf.pem';csr=ROOT/'leaf.csr';ext=ROOT/'leaf.ext'
ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
leafargs=[['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(leafkey),'-out',str(csr),'-subj','/CN=Loopback Server'],['openssl','x509','-req','-in',str(csr),'-CA',str(cert),'-CAkey',str(key),'-CAcreateserial','-out',str(leaf),'-days','1','-extfile',str(ext)]]
for args in leafargs:subprocess.run(args,capture_output=True,check=True)
class Origin(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  REQUESTS.append(dict(path=self.path,time=time.monotonic()))
  data=PAYLOAD;code=200
  if 'failure' in self.path:code=500;data=b'fixture failure'
  if self.path.startswith('/v1/'):
   data=json.dumps({'name':'fixture','is_premium':False}).encode()
  self.send_response(code);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*args):pass
srv=http.server.ThreadingHTTPServer(('127.0.0.1',0),Origin);ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey);srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
threading.Thread(target=srv.serve_forever,daemon=True).start();BASE=f'https://127.0.0.1:{srv.server_port}'
GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222','cccccccc-3333-4333-8333-333333333333','dddddddd-4444-4444-8444-444444444444']
def choices(filename='example.zip',plugin='Beta'):
 return {'version':1,'archives':[{'archiveFileName':filename,'selections':[{'stepName':'Choose','groupName':'Variant','plugins':[plugin]}]}]}
def comp(i=0,filename='example.zip',status=None,saved=None,selected=True,payload=PAYLOAD):
 meta={'Files':{filename:True},'ContentHashSHA256':hashlib.sha256(payload).hexdigest(),'custom':'keep'}
 if status:meta['handlerMetadata']={'fomodPromptStatus':{filename:status},'unrelated':'keep'}
 c={'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':selected,'Instructions':[],'ResourceRegistry':{BASE+'/'+filename:meta},'CustomField':'keep'}
 if saved is not None:c['FomodChoices']=saved
 return c
def setup(name,components=None,settings=None,raw_settings=None,existing=None):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components or [comp()]}))
 (sd/'settings.json').write_text(raw_settings if raw_settings is not None else json.dumps(settings or {'peerSharing':False,'usePublicPeerList':False,'telemetryOptIn':False}))
 for filename,data in (existing if existing is not None else {'example.zip':PAYLOAD}).items():(mods/filename).write_bytes(data)
 sidecar=case/'choices.json';sidecar.write_text(json.dumps(choices()))
 return dict(case=case,guide=guide,mods=mods,settings=sd,scratch=scratch,sidecar=sidecar)
def argv(c,flags):return [str(BIN),'fetch-downloads','--input',str(c['guide']),'--source-dir',str(c['mods']),'--settings-dir',str(c['settings']),'--scratch-dir',str(c['scratch']),'--peer-mode','off','--retries','0','--extra-ca-file',str(cert)]+[str(c['sidecar']) if x=='CHOICES' else x for x in flags]
def retain(name,c,args,env,code,stdout,stderr,before,extra=None):
 shutil.copytree(c['case'],OUT/'fixtures'/name/'after',dirs_exist_ok=True)
 try:data=json.loads(stdout)
 except:data=None
 if data and data.get('type')=='summary':data=data['summary']
 row=dict(name=name,argv=args,environment={k:env[k] for k in ENV if k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']},fomodEnvironment={k:v for k,v in env.items() if k.startswith('MODSYNC_FOMOD')},exit=code,stdout=stdout,stderr=stderr,data=data,before=before,after=snapshot(c['case']),extra=extra)
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2)+'\n');RUNS.append(row);return row
def run(name,flags=(),text='',env_extra=None,case=None,**kw):
 c=case or setup(name,**kw);env=ENV|dict(env_extra or {});args=argv(c,flags);before=snapshot(c['case']);shutil.copytree(c['case'],OUT/'fixtures'/name/'before',dirs_exist_ok=True)
 r=subprocess.run(args,input=text,capture_output=True,text=True,env=env,timeout=20)
 return retain(name,c,args,env,r.returncode,r.stdout,r.stderr,before,dict(stdinUtf8=text))
def outcomes(r):return [c['outcome'] for c in r['data']['fomodConfiguration']['consumers']]
def expect(r,code,outcome=None):
 check(r['name']+'_exit',r['exit'],code);check(r['name']+'_json',r['data'] is not None)
 if outcome is not None:check(r['name']+'_outcomes',outcomes(r),outcome)
 check(r['name']+'_scratch_clean',not any('scratch/' in p for p in r['after']))
 if '--record' not in r['argv']:check(r['name']+'_guide_unchanged',r['before']['guide.json'],r['after']['guide.json'])
 return r
r=subprocess.run([str(BIN),'fetch-downloads','--help'],capture_output=True,text=True,check=True);(OUT/'help.txt').write_text(r.stdout)
for flag in ['--fomod-skip','--fomod-choices','--interactive','--non-interactive']:check('help_'+flag,flag in r.stdout)
expect(run('bare_redirected'),3,['warned'])
expect(run('non_interactive',flags=['--non-interactive'],text='y\n2\n'),3,['warned'])
expect(run('skip_over_choices_mode',flags=['--fomod-skip','--fomod-choices','missing.json','--interactive'],raw_settings='{invalid'),3,['skipped'])
expect(run('explicit_choices_settings_bypass',flags=['--fomod-choices','CHOICES'],raw_settings='{invalid',env_extra={'MODSYNC_FOMOD_CHOICES':'missing.json'}),0,['configured'])
c=setup('environment_choices_settings_bypass',raw_settings='{invalid');expect(run('environment_choices_settings_bypass',case=c,env_extra={'MODSYNC_FOMOD_CHOICES':str(c['sidecar'])}),0,['configured'])
expect(run('settings_skip_over_interactive',flags=['--interactive'],settings={'fomodPostDownloadMode':'skip'},env_extra={'MODSYNC_FOMOD_POST_DOWNLOAD_MODE':'warn-continue'}),3,['skipped'])
expect(run('settings_warn_over_environment_skip',settings={'fomodPostDownloadMode':'warn-continue'},env_extra={'MODSYNC_FOMOD_POST_DOWNLOAD_MODE':'skip'}),3,['warned'])
expect(run('environment_skip',settings={},env_extra={'MODSYNC_FOMOD_POST_DOWNLOAD_MODE':'skip'}),3,['skipped'])
expect(run('interactive_over_warn',flags=['--interactive'],settings={'fomodPostDownloadMode':'warn-continue'},text='y\n2\n'),0,['configured'])
expect(run('unknown_mode_redirected',settings={'fomodPostDownloadMode':'unknown'}),3,['warned'])
expect(run('malformed_needed',raw_settings='{invalid'),1,['error'])
expect(run('missing_choices',flags=['--fomod-choices','missing.json']),1,['error'])
c=setup('bad_choices');c['sidecar'].write_text('{invalid');expect(run('bad_choices',case=c,flags=['--fomod-choices','CHOICES','--record']),1,['error'])
c=setup('unknown_plugin');c['sidecar'].write_text(json.dumps(choices(plugin='Unknown')));expect(run('unknown_plugin',case=c,flags=['--fomod-choices','CHOICES','--record']),1,['error'])
expect(run('invalid_saved_configured',components=[comp(status='configured',saved=choices(plugin='Unknown'))]),1,['error'])
expect(run('suppressed_configured_missing',components=[comp(status='configured')],flags=['--interactive']),3,['suppressed'])
expect(run('suppressed_warned_missing',components=[comp(status='warned')],flags=['--interactive']),3,['suppressed'])
expect(run('configured_valid',components=[comp(status='configured',saved=choices())]),0,['resolved'])
plain=io.BytesIO()
with zipfile.ZipFile(plain,'w') as z:z.writestr('payload.txt','plain fixture')
expect(run('generic_settings_bypass',components=[comp(payload=plain.getvalue())],existing={'example.zip':plain.getvalue()},raw_settings='{invalid',flags=['--fomod-choices','missing.json']),0,[])
r=run('dry_run',flags=['--dry-run','--interactive','--fomod-choices','missing.json'],raw_settings='{invalid');check('dry_run_exit',r['exit'],0);check('dry_run_no_writes',r['before'],r['after']);check('dry_run_no_prompt','Configure FOMOD' not in r['stderr'])
request_count=len(REQUESTS);r=run('conflicting_flags',flags=['--interactive','--non-interactive'],existing={});check('conflict_exit',r['exit'],2);check('conflict_before_acquisition',len(REQUESTS),request_count)
expect(run('actual_https_acquisition',existing={},flags=['--fomod-choices','CHOICES','--record']),0,['configured']);check('actual_https_request',any(x['path']=='/example.zip' for x in REQUESTS))
expect(run('actual_https_failure',components=[comp(filename='failure.zip')],existing={}),1,[])
# Shared archive coverage must be independent of consumer ordering.
for status in ['configured','warned']:
 for reverse in [False,True]:
  name=f'multi_{status}_{reverse}';a=comp(status=status,saved=choices(plugin='Alpha') if status=='configured' else None);b=comp(1);components=[a,b][::-1] if reverse else [a,b]
  c=setup(name,components=components);r=expect(run(name,case=c,flags=['--interactive','--record'],text='y\n2\n'),0)
  byid={v['componentGuid']:v for v in r['data']['fomodConfiguration']['consumers']};check(name+'_pending_configured',byid[GUIDS[1]]['outcome'],'configured');check(name+'_single_prompt',r['stderr'].count('Configure FOMOD'),1)
  if status=='configured':check(name+'_configured_preserved',byid[GUIDS[0]]['outcome'],'resolved')
# Unselected, options, and unrelated archive metadata are not enrolled.
a=comp(selected=False);a['Options']=[comp(2)];b=comp(1,selected=False);c=setup('scope',components=[a,b]);r=expect(run('scope',case=c,flags=['--select',GUIDS[0],'--fomod-choices','CHOICES','--record']),0,['configured'])
after=json.loads(c['guide'].read_text());comps=after if isinstance(after,list) else after.get('Components',after.get('ModSync'));check('scope_authored_selection_preserved',[x['IsSelected'] for x in comps],[False,False]);check('scope_unselected_choices_absent','FomodChoices' not in comps[1]);check('scope_option_choices_absent','FomodChoices' not in comps[0]['Options'][0])
# Persisted warning remains unresolved on restart; explicit choices resolves it.
c=setup('restart');expect(run('restart_warn',case=c,flags=['--record']),3,['warned']);expect(run('restart_suppressed',case=c,flags=['--interactive']),3,['suppressed']);expect(run('restart_choices',case=c,flags=['--record','--fomod-choices','CHOICES']),0,['configured']);expect(run('restart_resolved',case=c),0,['resolved'])
for label,text,expected in [('refusal','n\n',['refused']),('confirm_eof','',['eof']),('confirm_q','q\n',['userQuit']),('selection_eof','y\n',['eof']),('selection_q','y\nq\n',['userQuit']),('pipe_partial_eof','y\n2',['configured'])]:
 expect(run(label,flags=['--interactive'],text=text),0 if expected==['configured'] else 3,expected)
second={'example.zip':PAYLOAD,'second.zip':PAYLOAD};two=[comp(),comp(1,filename='second.zip')]
expect(run('serial_two',components=two,existing=second,flags=['--interactive'],text='y\n2\ny\n1\n'),0,['configured','configured'])
expect(run('refusal_then_configure',components=two,existing=second,flags=['--interactive'],text='n\ny\n2\n'),3,['refused','configured'])
expect(run('eof_stops_prompts',components=two,existing=second,flags=['--interactive']),3,['eof','notPrompted'])
c=setup('mixed_choices',components=two,existing=second);expect(run('mixed_choices',case=c,flags=['--fomod-choices','CHOICES','--record']),1,['configured','error']);expect(run('mixed_restart',case=c,flags=['--interactive'],text='n\n'),3,['resolved','refused'])

# One malformed consumer prevents publication to every pending consumer of that session.
a=comp();b=comp(1);next(iter(b['ResourceRegistry'].values()))['handlerMetadata']='invalid metadata'
c=setup('atomic_consumers',components=[a,b]);r=expect(run('atomic_consumers',case=c,flags=['--fomod-choices','CHOICES','--record']),1,['error','error']);check('atomic_consumers_no_session',r['data']['fomodConfiguration']['acceptedSessions'],[])
# Other archive choices remain exact when one pending archive is configured.
a=comp(saved=choices(filename='second.zip',plugin='Alpha'));a['ResourceRegistry'].update(comp(1,filename='second.zip')['ResourceRegistry']);c=setup('other_archive',components=[a],existing=second)
r=expect(run('other_archive',case=c,flags=['--fomod-choices','CHOICES','--record']),0,['configured','resolved']);after=json.loads(c['guide'].read_text());comps=after if isinstance(after,list) else after.get('Components',after.get('ModSync'));saved=comps[0]['FomodChoices'];check('other_archive_preserved',[v for v in saved['archives'] if v['archiveFileName']=='second.zip'],choices(filename='second.zip',plugin='Alpha')['archives'])
# Production wizard exercises all five shared groups, Required protection,
# explicit clear, back navigation, and visibility changes without a Rust helper.
def plugin(name,required=False,flag=None):
 return '<plugin name="'+name+'"><description>Generic option</description><files><file source="payload/a.txt" destination="Override/example.txt"/></files>'+('<conditionFlags><flag name="route">'+flag+'</flag></conditionFlags>' if flag else '')+'<typeDescriptor><type name="'+('Required' if required else 'Optional')+'"/></typeDescriptor></plugin>'
groups=''.join('<group name="'+kind+'" type="'+kind+'"><plugins>'+plugin('Alpha',required=kind=='SelectExactlyOne')+plugin('Beta')+'</plugins></group>' for kind in ['SelectExactlyOne','SelectAny','SelectAtMostOne','SelectAtLeastOne','SelectAll'])
xml='<config><moduleName>All Groups</moduleName><installSteps><installStep name="Groups"><optionalFileGroups>'+groups+'</optionalFileGroups></installStep><installStep name="Finish"><optionalFileGroups><group name="Extra" type="SelectAny"><plugins>'+plugin('Extra')+'</plugins></group></optionalFileGroups></installStep></installSteps></config>'
payload=package(xml);r=expect(run('five_groups_back_required',components=[comp(payload=payload)],existing={'example.zip':payload},flags=['--interactive'],text='y\n2\n1\n0\n2\n0\n1\n0\n1,2\nback\n1\n0\n0\n2\n1,2\n0\n'),0,['configured']);check('required_conflict_visible','Cannot deselect required' in r['stderr']);check('all_groups_back',r['stderr'].count('Step: Groups'),2);check('at_least_constraint','Select at least one' in r['stderr']);check('all_constraint','All options' in r['stderr'])
xml='<config><moduleName>Visibility</moduleName><installSteps><installStep name="Route"><optionalFileGroups><group name="Route" type="SelectExactlyOne"><plugins>'+plugin('Alpha',flag='on')+plugin('Beta',flag='off')+'</plugins></group></optionalFileGroups></installStep><installStep name="Conditional"><visible><flagDependency flag="route" value="on"/></visible><optionalFileGroups><group name="Extra" type="SelectAny"><plugins>'+plugin('Extra')+'</plugins></group></optionalFileGroups></installStep></installSteps></config>'
payload=package(xml);r=expect(run('visibility_back',components=[comp(payload=payload)],existing={'example.zip':payload},flags=['--interactive'],text='y\n1\nback\n2\n'),0,['configured']);check('visibility_back_shown_once',r['stderr'].count('Step: Conditional'),1);check('visibility_back_route_twice',r['stderr'].count('Step: Route'),2)
expect(run('progress_summary',flags=['--progress-json','--fomod-choices','CHOICES']),0,['configured'])

def terminal(name,steps,expected,pipe=False,automatic=False,two_archives=False):
 c=setup(name,components=two if two_archives else None,existing=second if two_archives else None);args=argv(c,['--record']+([] if automatic else ['--interactive']));before=snapshot(c['case']);env=ENV.copy();shutil.copytree(c['case'],OUT/'fixtures'/name/'before',dirs_exist_ok=True)
 if pipe:readfd,writefd=os.pipe();input_master=None
 else:input_master,readfd=pty.openpty();writefd=input_master
 original_flags=fcntl.fcntl(readfd,fcntl.F_GETFL);original_termios=None if pipe else termios.tcgetattr(readfd)
 if automatic:output_master,output_slave=pty.openpty();outtarget=output_slave
 else:output_master=output_slave=None;outtarget=subprocess.PIPE
 proc=subprocess.Popen(args,stdin=readfd,stdout=outtarget,stderr=subprocess.PIPE,env=env)
 os.set_blocking(proc.stderr.fileno(),False);err=b'';signals=[]
 def until(marker):
  nonlocal err
  deadline=time.monotonic()+10
  while marker.encode() not in err:
   if proc.poll() is not None:raise RuntimeError(f'{name} exited before {marker}: {err!r}')
   ready,_,_=select.select([proc.stderr],[],[],.05)
   if ready:
    chunk=os.read(proc.stderr.fileno(),65536);err+=chunk
   if time.monotonic()>deadline:raise RuntimeError(f'{name} timeout awaiting {marker}: {err!r}')
 for marker,action in steps:
  until(marker)
  if action=='SIGINT':signals.append(time.monotonic());proc.send_signal(signal.SIGINT)
  else:os.write(writefd,action)
 started=time.monotonic();proc.wait(timeout=5);elapsed=time.monotonic()-started
 while True:
  try:chunk=os.read(proc.stderr.fileno(),65536)
  except BlockingIOError:break
  if not chunk:break
  err+=chunk
 if automatic:
  os.close(output_slave);stdout=b''
  while True:
   try:chunk=os.read(output_master,65536)
   except OSError:break
   if not chunk:break
   stdout+=chunk
  os.close(output_master)
 else:stdout=proc.stdout.read()
 restored=fcntl.fcntl(readfd,fcntl.F_GETFL)==original_flags
 terminal_restored=True if pipe else termios.tcgetattr(readfd)==original_termios
 os.close(readfd);os.close(writefd)
 r=retain(name,c,args,env,proc.returncode,stdout.decode(),err.decode(),before,dict(inputActions=[dict(waitFor=marker,stdinHex=action.hex()) if isinstance(action,bytes) else dict(waitFor=marker,signal=action) for marker,action in steps],signals=signals,exitLatencySeconds=elapsed,originalFlags=original_flags,flagsRestored=restored,terminalModeRestored=terminal_restored,pipeWriterHeldOpen=pipe))
 expect(r,130 if signals else (1 if 'error' in expected else 0 if all(x=='configured' for x in expected) else 3),expected);check(name+'_flags_restored',restored);check(name+'_termios_restored',terminal_restored);check(name+'_no_surviving_child',not pathlib.Path(f'/proc/{proc.pid}').exists())
 return r
terminal('pty_cancel_confirmation',[('now?',b'partial'),('now?','SIGINT')],['signalCancelled'])
terminal('pty_cancel_selection',[('now?',b'y\n'),('Numbers separated',b'2'),('Numbers separated','SIGINT')],['signalCancelled'])
terminal('pipe_cancel_confirmation',[('now?',b'partial'),('now?','SIGINT')],['signalCancelled'],pipe=True)
terminal('pipe_cancel_selection',[('now?',b'y\n'),('Numbers separated',b'2'),('Numbers separated','SIGINT')],['signalCancelled'],pipe=True)
terminal('pipe_invalid_utf8',[('now?',b'\xff\n')],['error'],pipe=True)
terminal('pipe_split_utf8',[('now?',b'y\n'),('Numbers separated',b'\xce'),('Numbers separated',b'\xb1\n2\n')],['configured'],pipe=True)
terminal('pty_eof',[('now?',b'\x04')],['eof'])
terminal('pty_editing',[('now?',b'y\n'),('Numbers separated',b'9\x7f2\n')],['configured'])
terminal('automatic_tty',[('now?',b'y\n'),('Numbers separated',b'2\n')],['configured'],automatic=True)
terminal('mixed_signal',[('example.zip now?',b'y\n2\n'),('second.zip now?','SIGINT')],['configured','signalCancelled'],two_archives=True)
# Inherited standalone entry point and choices output still execute real shared session logic.
c=setup('standalone');save=c['case']/'saved.json';args=[str(BIN),'fomod-inspect',str(c['mods']/'example.zip'),'--interactive','--save-choices',str(save),'--extracted-directory','expanded','--scratch-dir',str(c['scratch'])]
r=subprocess.run(args,input='2\n',capture_output=True,text=True,env=ENV,timeout=20);row=dict(argv=args,exit=r.returncode,stdout=r.stdout,stderr=r.stderr,saved=json.loads(save.read_text()) if save.exists() else None);(OUT/'standalone.json').write_text(json.dumps(row,indent=2));check('standalone_exit',r.returncode,0);check('standalone_json',isinstance(json.loads(r.stdout),dict));check('standalone_saved',row['saved'],choices())
srv.shutdown();srv.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',{str(p.relative_to(REPO)):digest(p) for p in sorted((REPO/'src').rglob('*.rs'))},SOURCE_BEFORE)
receipt=dict(binaryBeforeSha256=BINARY_BEFORE,sourceBeforeSha256=SOURCE_BEFORE,leafCertificateCommands=leafargs,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),binary=str(BIN),binarySha256=digest(BIN),sourceSha256={str(p.relative_to(REPO)):digest(p) for p in sorted((REPO/'src').rglob('*.rs'))},cargoSha256={p:digest(REPO/p) for p in ['Cargo.toml','Cargo.lock']},certificateCommand=certargs,requests=REQUESTS,checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),cases=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:receipt[k] for k in ['root','passed','total','cases']},indent=2))
assert all(c['passed'] for c in CHECKS)
