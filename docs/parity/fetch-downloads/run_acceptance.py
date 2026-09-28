#!/usr/bin/env python3
"""Production CLI acceptance, synthetic archives, one isolated network namespace.
Run: FETCH_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fetch-downloads/run_acceptance.py
No Rust tests or alternate acquisition implementations are used.
"""
import base64, copy, hashlib, http.server, json, os, pathlib, select, signal
import socket, struct, subprocess, tempfile, threading, time, zipfile

REPO = pathlib.Path(__file__).resolve().parents[3]
BIN = pathlib.Path('/home/brunner56/.cache/modsync-rs-target-lead/debug/modsync')
OUT = REPO/'docs/parity/fetch-downloads/acceptance'
OUT.mkdir(parents=True, exist_ok=True)
ROOT = pathlib.Path(tempfile.mkdtemp(prefix='fetch-acceptance-'))
ENV = os.environ.copy()
for k in list(ENV):
    if k.lower().endswith('_proxy') or 'NEXUS' in k: ENV.pop(k)
for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
    ENV[k] = str(ROOT/k); pathlib.Path(ENV[k]).mkdir()
checks=[]; runs=[]; connects=[]; requests=[]; children=[]; throttle=threading.Event()

def check(name, actual, expected=True):
    checks.append(dict(name=name, actual=actual, expected=expected, passed=actual==expected))
    if actual != expected: print('FAIL',name,repr(actual),flush=True)

def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def snapshot(path):
    return {str(p.relative_to(path)): digest(p) if p.is_file() else 'directory' for p in sorted(path.rglob('*'))}
def recv_n(sock,n):
    data=b''
    while len(data)<n:
        chunk=sock.recv(n-len(data))
        if not chunk: raise OSError('short read')
        data+=chunk
    return data

def pump(a,b,slow):
    try:
        while True:
            data=a.recv(2048 if slow else 65536)
            if not data: break
            if slow and throttle.is_set(): time.sleep(.12)
            b.sendall(data)
    except OSError: pass
    finally:
        for s in (a,b):
            try: s.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            s.close()

def relay(conn):
    try:
        version,n=recv_n(conn,2); recv_n(conn,n); conn.sendall(b'\x05\x00')
        _,cmd,_,kind=recv_n(conn,4)
        if kind==1: host=socket.inet_ntoa(recv_n(conn,4))
        elif kind==3: host=recv_n(conn,recv_n(conn,1)[0]).decode()
        else: raise OSError('unsupported address')
        port=struct.unpack('!H',recv_n(conn,2))[0]
        connects.append(dict(host=host,port=port,time=time.monotonic()))
        if cmd != 1 or host != '127.0.0.1': raise OSError('fixture only accepts loopback CONNECT')
        upstream=socket.create_connection((host,port),timeout=10); upstream.settimeout(None)
        conn.sendall(b'\x05\x00\x00\x01'+socket.inet_aton('127.0.0.1')+b'\x00\x00')
        threading.Thread(target=pump,args=(conn,upstream,False),daemon=True).start()
        threading.Thread(target=pump,args=(upstream,conn,True),daemon=True).start()
    except OSError: conn.close()

def relay_loop(server):
    while True:
        try: conn,_=server.accept()
        except OSError: return
        threading.Thread(target=relay,args=(conn,),daemon=True).start()

class API(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        requests.append(self.path)
        state=self.server.state
        if self.path.endswith('/users/validate.json'):
            code=500 if state=='error' else 200
            body=json.dumps({'name':'fixture','is_premium':False}).encode() if code==200 else b'fixture error'
        else: code=404; body=b'not expected'
        self.send_response(code); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self,*args): pass

def bencode(v):
    if isinstance(v,int): return b'i'+str(v).encode()+b'e'
    if isinstance(v,bytes): return str(len(v)).encode()+b':'+v
    return b'd'+b''.join(bencode(k)+bencode(v[k]) for k in sorted(v))+b'e'

def make_seed():
    archive=ROOT/'example.zip'
    with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_STORED) as z:
        z.writestr('payload/example.bin',bytes(range(256))*4096)
    payload=archive.read_bytes(); piece=65536
    info=bencode({b'name':b'example.zip',b'length':len(payload),b'piece length':piece,b'pieces':b''.join(hashlib.sha1(payload[i:i+piece]).digest() for i in range(0,len(payload),piece))})
    magnet='magnet:?xt=urn:btih:'+hashlib.sha1(info).hexdigest()+'&dn=example.zip'
    encoded=base64.b64encode(info).decode()
    argv=[str(BIN),'torrent-seed',str(archive),'--no-proxy','--authored-magnet',magnet,'--torrent-info',encoded,'--share-ratio','0','--upload-kibps','10000']
    err=open(OUT/'seed.stderr','w'); proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=err,text=True,env=ENV);children.append(proc)
    ready,_,_=select.select([proc.stdout],[],[],30)
    if not ready: raise RuntimeError('seed did not report readiness')
    first=proc.stdout.readline(); (OUT/'seed.first.json').write_text(first)
    seed=json.loads(first); assert seed['peer'].startswith('127.0.0.1:')
    (OUT/'seed.argv.json').write_text(json.dumps(argv,indent=2))
    return archive,magnet,encoded,seed['peer']

def component(metadata=None,name='Example A',guid='aaaaaaaa-1111-4111-8111-111111111111',url='https://mega.nz/file/example'):
    return dict(Guid=guid,Name=name,IsSelected=True,Instructions=[],ResourceRegistry={url:copy.deepcopy(metadata or {})})

def execute(name, guide, flags=(), settings=None, raw_settings=None, existing=None, markdown=False, cancel=False):
    case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir(); sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
    path=case/('guide.md' if markdown else 'guide.json')
    text=json.dumps(guide); path.write_text('```json\n'+text+'\n```\n' if markdown else text)
    if raw_settings is not None: (sd/'settings.json').write_text(raw_settings)
    elif settings is not None: (sd/'settings.json').write_text(json.dumps(settings))
    if existing:
        for name_,data in existing.items(): (mods/name_).write_bytes(data)
    before=snapshot(case); env_before={k:snapshot(pathlib.Path(ENV[k])) for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']}; cb=len(connects); rb=len(requests)
    argv=[str(BIN),'fetch-downloads','--input',str(path),'--source-dir',str(mods),'--settings-dir',str(sd),'--scratch-dir',str(scratch)]+list(flags)
    if cancel:
        proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=ENV);children.append(proc)
        deadline=time.monotonic()+20
        while len(connects)==cb and proc.poll() is None and time.monotonic()<deadline: time.sleep(.05)
        time.sleep(.8); proc.send_signal(signal.SIGINT)
        try: stdout,stderr=proc.communicate(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill();stdout,stderr=proc.communicate();raise RuntimeError('cancel did not stop production process')
        code=proc.returncode
    else:
        r=subprocess.run(argv,capture_output=True,text=True,env=ENV,timeout=60);stdout,stderr,code=r.stdout,r.stderr,r.returncode
    data=None;events=[]
    try:
        if '--progress-json' in flags and stdout:
            objects=[json.loads(line) for line in stdout.splitlines()]
            check(name+'_ndjson_summary_last',objects[-1].get('type'),'summary')
            check(name+'_ndjson_event_envelopes',all(o.get('type')=='acquisition' and 'event' in o for o in objects[:-1]))
            data=objects[-1]['summary'];events=[o['event'] for o in objects[:-1]]
        else: data=json.loads(stdout)
    except (ValueError,KeyError): pass
    record=dict(name=name,argv=argv,exit=code,stdout=stdout,stderr=stderr,data=data,events=events,connections=connects[cb:],apiRequests=requests[rb:],before=before,after=snapshot(case),guideAfter=path.read_text(),envBefore=env_before,envAfter={k:snapshot(pathlib.Path(ENV[k])) for k in env_before})
    if '--dry-run' in flags:
        check(name+'_no_env_cache_write',record['envBefore'],record['envAfter']);check(name+'_no_api',record['apiRequests'],[])
    (OUT/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n');runs.append(record)
    record['mods']=mods;record['scratch']=scratch
    return record

def no_transfer(r):
    check(r['name']+'_no_proxy',r['connections'],[])
    check(r['name']+'_no_peer_events',any(e['source']['kind']=='peer' for e in r['events']),False)
def clean(r):
    check(r['name']+'_no_staging', [str(p.relative_to(r['mods'])) for p in r['mods'].rglob('*') if p.name.startswith(('.peer-','.tmp'))],[])
    check(r['name']+'_scratch_clean',list(r['scratch'].iterdir()),[])
def success(r,names,archive):
    check(r['name']+'_exit',r['exit'],0)
    for n in names: check(r['name']+'_bytes_'+n,digest(r['mods']/n) if (r['mods']/n).exists() else None,digest(archive))
    for e in r['events']:
        if e['source']['kind']=='peer':
            target=r['data']['report']['outcomes'][e['targetIndex']]
            check(r['name']+'_event_identity_'+str(e['targetIndex']),e['componentGuid'],target['componentGuid'])
            if r['name'] not in ['unknown','unknown_magnet'] or e['phase']['phase'] in ['transferring','verifying']:
                check(r['name']+'_event_filename_'+str(e['targetIndex']),e['filename'],target['filename'])
    clean(r)

def main():
    subprocess.run(['ip','link','set','lo','up'],check=True)
    assert not subprocess.check_output(['ip','route'],text=True).strip(), 'isolated namespace required'
    assert os.environ.get('FETCH_PARENT_NETNS') and os.readlink('/proc/self/ns/net') != os.environ['FETCH_PARENT_NETNS'], 'set FETCH_PARENT_NETNS before unshare; do not run in host namespace'
    source_paths=[REPO/'src'/n for n in ['main.rs','download_orchestrator.rs','peer_acquire.rs','model.rs','markdown.rs','download.rs','p2p.rs','lib.rs']]
    source_before={str(p.relative_to(REPO)):digest(p) for p in source_paths};binary_before=digest(BIN)
    server=socket.socket();server.bind(('127.0.0.1',0));server.listen(64)
    threading.Thread(target=relay_loop,args=(server,),daemon=True).start()
    api=http.server.ThreadingHTTPServer(('127.0.0.1',0),API);api.state='free';threading.Thread(target=api.serve_forever,daemon=True).start()
    try:
        archive,magnet,info,peer=make_seed()
        settings={'peerSharing':True,'peerMode':'fallback','peerProxies':['socks5://127.0.0.1:'+str(server.getsockname()[1])],'peerProxyListUrl':'','peerTrackerListUrl':None,'peerTrackers':[],'peerExtraPeers':[peer]}
        metadata={'Files':{'example.zip':True},'ContentHashSHA256':digest(archive),'Magnet':magnet,'TorrentInfo':info}
        guide={'Components':[component(metadata)]}
        # Real peer, known and unknown names, plus magnet metadata discovery.
        for name,remove in [('known',()),('unknown',('Files',)),('unknown_magnet',('Files','TorrentInfo'))]:
            g=copy.deepcopy(guide)
            for key in remove: g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example'].pop(key)
            r=execute(name,g,['--progress-json'],settings);success(r,['example.zip'],archive);check(name+'_proxy_used',len(r['connections'])>0)
        # Shared per-run pool: two targets, one configured relay, workers serialized.
        g=copy.deepcopy(guide);other=component(metadata,'Example B','bbbbbbbb-2222-4222-8222-222222222222');other['ResourceRegistry']['https://mega.nz/file/example']['Files']={'second.zip':True};g['Components'].append(other)
        r=execute('shared_pool',g,['--workers','1','--progress-json'],settings);success(r,['example.zip','second.zip'],archive)
        check('shared_pool_two_completed',len(r['data']['report']['outcomes']),2);check('shared_pool_observed_single_endpoint',set((c['host'],c['port']) for c in r['connections'])=={('127.0.0.1',int(peer.split(':')[1]))})
        # Nexus gates must never call Peer or its policy.
        for state in ['free','error']:
            api.state=state
            for mode in ['race','fallback']:
                g={'Components':[component(metadata,url='https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1')]}
                r=execute('nexus_'+state+'_'+mode,g,['--peer-mode',mode,'--nexus-mods-api-key','fixture-key','--nexus-api-base','http://127.0.0.1:'+str(api.server_port)+'/v1/','--progress-json'],settings)
                check(r['name']+'_exit',r['exit'],3 if state=='free' else 1);no_transfer(r)
                check(r['name']+'_validate_only',r['apiRequests'],['/v1/users/validate.json'])
                check(r['name']+'_status',r['data']['report']['outcomes'][0]['status'],'manualRequired' if state=='free' else 'failed')
                if state=='error': check(r['name']+'_terminal',all(not e['phase'].get('will_fallback',False) for e in r['events']))
                clean(r)
        # Invalid authored metadata is rejected before proxy bootstrap.
        for name,value in [('empty',''),('nonstring',123),('null',None),('base64','%%%'),('infohash',base64.b64encode(b'd4:name5:x.zipe').decode())]:
            g=copy.deepcopy(guide);g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example']['TorrentInfo']=value
            r=execute('bad_'+name,g,['--progress-json'],settings);check(r['name']+'_exit',r['exit'],3);check(r['name']+'_source_failed',any(e['phase']['phase']=='sourceFailed' for e in r['events']));check(r['name']+'_no_proxy',r['connections'],[]);check(r['name']+'_unpublished',list(r['mods'].iterdir()),[]);clean(r)
        g=copy.deepcopy(guide);g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example']['Files']={'../escape.zip':True}
        r=execute('bad_filename',g,['--progress-json'],settings);check('bad_filename_no_proxy',r['connections'],[]);check('bad_filename_unpublished',list(r['mods'].iterdir()),[])
        for name,flags,bad_hash in [('budget',['--max-expanded-bytes','1'],False),('hash',[],True)]:
            g=copy.deepcopy(guide)
            if bad_hash:g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example']['ContentHashSHA256']='0'*64
            r=execute(name,g,['--progress-json']+flags,settings);check(name+'_exit',r['exit'],3);check(name+'_source_failed',any(e['phase']['phase']=='sourceFailed' for e in r['events']));check(name+'_unpublished',list(r['mods'].iterdir()),[]);clean(r)
        throttle.set()
        r=execute('cancel',guide,['--progress-json'],settings,cancel=True);check('cancel_exit',r['exit'],130);check('cancel_proxy_started',len(r['connections'])>0);check('cancel_unpublished',list(r['mods'].iterdir()),[]);clean(r);throttle.clear()
        # Dry-run selection/settings cases preserve every byte and create nothing.
        g={'Components':[component(metadata),component(metadata,'Example B','bbbbbbbb-2222-4222-8222-222222222222')]};g['Components'][0]['IsSelected']=False
        for name,flags,count in [('default',[],2),('file',['--use-file-selection'],1),('select',['--select',g['Components'][0]['Guid']],1),('deselect',['--deselect',g['Components'][0]['Guid']],1)]:
            r=execute('dry_'+name,g,['--dry-run','--peer-mode','off']+flags,settings);check(r['name']+'_targets',len(r['data']['targets']),count);check(r['name']+'_no_writes',r['before'],r['after']);no_transfer(r)
        # Explicit option/filter selection must not inherit unrelated default selection.
        g=copy.deepcopy(guide);g['Components'][0]['Category']=['Examples']
        opt=component({},'Example Option','cccccccc-3333-4333-8333-333333333333');opt['IsSelected']=False
        g['Components'][0]['Options']=[opt]
        g['Components'].append(component(metadata,'Unrelated','bbbbbbbb-2222-4222-8222-222222222222'))
        for name,flags in [('option',['--option',opt['Guid']]),('category',['--add-category','Examples']),('search',['--select-visible-search','Example A'])]:
            r=execute('selector_'+name,g,['--dry-run','--peer-mode','off']+flags,settings)
            check(r['name']+'_only_parent',[t['componentGuid'] for t in r['data']['targets']],[g['Components'][0]['Guid']]);check(r['name']+'_no_writes',r['before'],r['after'])
        # All disabled file facts omit the resource; no screen size means no implicit resolution choice.
        g=copy.deepcopy(guide);g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example']['Files']={'example.zip':False}
        r=execute('disabled_files',g,['--dry-run','--peer-mode','off'],settings);check('disabled_files_no_targets',r['data']['targets'],[]);check('disabled_files_reason',r['data']['skipped'][0]['reason'],'allFilesFlaggedFalse');check('disabled_files_no_writes',r['before'],r['after'])
        g=copy.deepcopy(guide);g['Components'][0]['ResourceRegistry']['https://mega.nz/file/example']['Files']={'example-1920x1080.zip':True,'example-2560x1440.zip':True}
        r=execute('resolution_unspecified',g,['--dry-run','--peer-mode','off'],settings);check('resolution_unspecified_preserves_both',len(r['data']['targets']),2);check('resolution_no_writes',r['before'],r['after'])
        for name,stored,flags,mode in [('off',{'peerMode':'off'},[],'off'),('fallback',{'peerMode':'fallback'},[],'fallback'),('race',{},[],'race'),('disabled',{'peerSharing':False},['--peer-mode','race'],'off'),('override',{'peerMode':'off'},['--peer-mode','fallback'],'fallback')]:
            r=execute('settings_'+name,guide,['--dry-run']+flags,stored);check(r['name']+'_mode',r['data']['peerMode'],mode);check(r['name']+'_no_writes',r['before'],r['after']);no_transfer(r)
        for name,flags,expected in [('needed',[],1),('bypass',['--peer-mode','off','--nexus-mods-api-key','fixture-key'],0)]:
            r=execute('malformed_'+name,guide,['--dry-run']+flags,raw_settings='{bad');check(r['name']+'_exit',r['exit'],expected);no_transfer(r)
        r=execute('override_no_key',guide,['--dry-run','--peer-mode','off','--nexus-api-base','http://127.0.0.1:'+str(api.server_port)+'/v1/'],{'nexusModsApiKey':'stored'});check('override_no_key_exit',r['exit'],1);check('override_no_key_no_request',r['apiRequests'],[])
        for md in [False,True]:
            g=copy.deepcopy(guide);g['Components'][0]['IsSelected']=False
            r=execute('all_false_'+('md' if md else 'json'),g,['--dry-run','--peer-mode','off','--use-file-selection'],settings,markdown=md);check(r['name']+'_no_targets',r['data']['targets'],[]);check(r['name']+'_no_skips',r['data']['skipped'],[]);check(r['name']+'_no_writes',r['before'],r['after'])
        g=copy.deepcopy(guide);g['Components'][0]['IsSelected']=False
        r=execute('record_false',g,['--peer-mode','off','--record','--progress-json'],settings,existing={'example.zip':archive.read_bytes()});check('record_false_exit',r['exit'],0);check('record_false_preserved',json.loads(r['guideAfter'])[0]['IsSelected'],False);no_transfer(r)
        g=copy.deepcopy(guide);g['Components'][0]['IsSelected']=False;other=component(metadata,'Excluded','bbbbbbbb-2222-4222-8222-222222222222');other['IsSelected']=True;g['Components'].append(other)
        r=execute('record_excluded',g,['--peer-mode','off','--record','--select',g['Components'][0]['Guid']],settings,existing={'example.zip':archive.read_bytes()});check('record_excluded_exit',r['exit'],0);check('record_excluded_selection',[c['IsSelected'] for c in json.loads(r['guideAfter'])],[False,True]);no_transfer(r)
        # A real FOMOD archive is reported as manual followup, not auto-installed.
        fomod=ROOT/'fomod.zip'
        with zipfile.ZipFile(fomod,'w') as z:
            z.writestr('fomod/ModuleConfig.xml',"<config><moduleName>Example Installer</moduleName><requiredInstallFiles><file source='payload.txt' destination='Override/payload.txt'/></requiredInstallFiles></config>");z.writestr('payload.txt','fixture payload')
        g={'Components':[component({'Files':{'fomod.zip':True},'ContentHashSHA256':digest(fomod)})]};g['Components'][0]['IsSelected']=False
        r=execute('fomod',g,['--peer-mode','off','--record','--progress-json'],settings,existing={'fomod.zip':fomod.read_bytes()});check('fomod_exit',r['exit'],3);check('fomod_followup',len(r['data']['fomodFollowups']['followups']),1);check('fomod_selection_preserved',json.loads(r['guideAfter'])[0]['IsSelected'],False);no_transfer(r);clean(r)
    finally:
        for p in children:
            if p.poll() is None:
                p.terminate()
                try:p.wait(timeout=10)
                except subprocess.TimeoutExpired:p.kill();p.wait()
        if children and children[0].stdout:
            (OUT/'seed.stdout').write_text((OUT/'seed.first.json').read_text()+children[0].stdout.read())
        api.shutdown();api.server_close();server.close()
        source_after={str(p.relative_to(REPO)):digest(p) for p in source_paths};check('sources_unchanged_during_run',source_before,source_after);check('binary_unchanged_during_run',binary_before,digest(BIN))
        receipt={'binaryBeforeSha256':binary_before,'sourceBeforeSha256':source_before,'root':str(ROOT),'namespace':os.readlink('/proc/self/ns/net'),'routes':subprocess.check_output(['ip','route'],text=True),'binary':str(BIN),'binarySha256':digest(BIN),'sourceSha256':{str(p.relative_to(REPO)):digest(p) for p in [REPO/'src'/n for n in ['main.rs','download_orchestrator.rs','peer_acquire.rs','model.rs','markdown.rs','download.rs','p2p.rs','lib.rs']]},'checks':checks,'passed':sum(c['passed'] for c in checks),'total':len(checks),'connections':connects,'apiRequests':requests,'childExits':{str(p.pid):p.returncode for p in children}}
        # Checks compare booleans/hashes, never dump archive payloads into receipts.
        (OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,default=str)+'\n')
        print(json.dumps({'passed':receipt['passed'],'total':receipt['total'],'failed':[c['name'] for c in checks if not c['passed']],'root':str(ROOT)}),flush=True)
    return 0 if all(c['passed'] for c in checks) else 1

if __name__=='__main__': raise SystemExit(main())
