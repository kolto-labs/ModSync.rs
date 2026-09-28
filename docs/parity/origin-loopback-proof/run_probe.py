#!/usr/bin/env python3
"""Production-only acceptance; run with LOOPBACK_PARENT_NETNS set, unshare -Urnm.
Every server, relay, seed and fetch runs in this one disposable namespace.
"""
import copy, hashlib, http.server, importlib.util, json, os, pathlib, signal
import socket, ssl, subprocess, sys, threading, time, traceback, shutil

sys.dont_write_bytecode=True
REPO=pathlib.Path(__file__).resolve().parents[3]
OUT=pathlib.Path(__file__).resolve().parent/'receipt'
OUT.mkdir(exist_ok=True)
assert os.environ.get('LOOPBACK_PARENT_NETNS') != os.readlink('/proc/self/ns/net')
assert os.environ.get('LOOPBACK_PARENT_NETNS')
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
spec=importlib.util.spec_from_file_location('accepted_fixture',REPO/'docs/parity/fetch-downloads/run_acceptance.py')
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
f.OUT=OUT
ROOT=f.ROOT
hosts=ROOT/'hosts';hosts.write_text('127.0.0.1 localhost\n192.0.2.1 public.example localhost.evil\n::1 ip6-localhost\n')
subprocess.run(['mount','--bind',str(hosts),'/etc/hosts'],check=True)
CHECKS=[];RUNS=[];TIMELINE=[];CONNECTS=[];SERVERS=[];OPEN=[]
SOURCE={str(p.relative_to(REPO)):f.digest(p) for p in sorted((REPO/'src').rglob('*.rs'))}
BINARY=f.digest(f.BIN)
PROOF_SOURCES={str(p.relative_to(REPO)):f.digest(p) for p in [pathlib.Path(__file__),REPO/'docs/parity/fetch-downloads/run_acceptance.py',REPO/'Cargo.toml',REPO/'Cargo.lock']}
ENV=f.ENV.copy()

def check(name, actual, expected=True):
    row=dict(name=name,actual=actual,expected=expected,passed=actual==expected);CHECKS.append(row)
    if not row['passed']: print('FAIL',name,repr(actual),flush=True)

def stamp(kind,**kw):
    row=dict(kind=kind,time=time.monotonic(),**kw);TIMELINE.append(row);return row

def certificate(name):
    key=ROOT/(name+'.key');cert=ROOT/(name+'.pem')
    cmd=['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','1','-subj','/CN=Loopback Fixture','-addext','subjectAltName=DNS:localhost,DNS:localhost.evil,DNS:public.example,IP:127.0.0.1,IP:192.0.2.1,IP:::1','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign']
    result=subprocess.run(cmd,capture_output=True,text=True,check=True)
    (OUT/(name+'.openssl.json')).write_text(json.dumps(dict(argv=cmd,exit=result.returncode),indent=2))
    return key,cert

class Origin(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        mode=self.path.split('?',1)[0].split('/')[-1]
        self.server.counts[mode]=self.server.counts.get(mode,0)+1
        stamp('request',server=self.server.label,path=self.path,headers=dict(self.headers))
        code=200;headers={};data=PAYLOAD;delay=0
        if self.path.startswith('/v1/'):
            headers['Content-Type']='application/json'
            if self.path.endswith('/users/validate.json'): data=json.dumps({'name':'fixture','is_premium':True}).encode()
            elif self.path.endswith('/download_link.json'): data=json.dumps([{'URI':BASE+'/archive'}]).encode()
            else: data=json.dumps({'file_name':'example.zip'}).encode()
        elif mode in ['redirect','chain','downgrade','escape','deceptive','loop','credentials-redirect','redirect-localhost','redirect-ipv6']:
            targets={'redirect':BASE+'/archive','chain':BASE+'/redirect','downgrade':HTTP+'/archive','escape':OUTSIDE+'/archive','deceptive':OUTSIDE.replace('192.0.2.1','localhost.evil')+'/archive','loop':BASE+'/loop','credentials-redirect':OUTSIDE+'/archive?token=fixture-redact','redirect-localhost':BASE.replace('127.0.0.1','localhost')+'/archive','redirect-ipv6':IPV6+'/archive'}
            code=302;headers['Location']=targets[mode];data=b'<html>redirect fixture</html>';headers['Content-Type']='text/html'
        elif mode.startswith('gate-'):
            data={'gate-login':b'<html><form><input type="password"></form></html>' ,'gate-body':b'<html>g-recaptcha verify you are human</html>','gate-header':b'challenge','gate-wait':b'<html><div id="countdown">please wait</div></html>' }[mode]
            headers['Content-Type']='text/html' if mode != 'gate-header' else 'application/octet-stream'
            if mode=='gate-header': headers['cf-mitigated']='challenge'
        elif mode=='retry429':
            if self.server.counts[mode] < 3: code=429;data=b'retry';headers['Retry-After']='0'
        elif mode=='retry503': code=503;data=b'retry'
        elif mode=='backoff': code=429;data=b'retry';headers['Retry-After']='600'
        elif mode=='corrupt': data=b'PK\x03\x04corrupted archive'
        elif mode=='unknown': headers['Content-Disposition']='attachment; filename="example.zip"'
        elif mode=='unsafe': headers['Content-Disposition']='attachment; filename="../escape.zip"'
        elif mode=='unknown-empty': headers['Content-Disposition']='attachment; filename=""'
        elif mode in ['slow','peer-win','cancel-body','cancel-race']: delay=.12
        elif mode=='origin-win': delay=.015
        headers.setdefault('Content-Type','application/octet-stream')
        self.send_response(code)
        for k,v in headers.items(): self.send_header(k,v)
        self.send_header('Content-Length',str(len(data)));self.end_headers()
        try:
            for i in range(0,len(data),16384):
                self.wfile.write(data[i:i+16384]);self.wfile.flush()
                if delay: time.sleep(delay)
            stamp('body-complete',server=self.server.label,path=self.path)
        except (OSError,ssl.SSLError): stamp('body-disconnected',server=self.server.label,path=self.path)
    def log_message(self,*args): pass

def server(address,port,label,ctx=None):
    class IPv6Server(http.server.ThreadingHTTPServer): address_family=socket.AF_INET6
    srv=(IPv6Server if ':' in address else http.server.ThreadingHTTPServer)((address,port),Origin);srv.daemon_threads=True;srv.label=label;srv.counts={}
    if ctx: srv.socket=ctx.wrap_socket(srv.socket,server_side=True)
    threading.Thread(target=srv.serve_forever,daemon=True).start();SERVERS.append(srv);return srv

class ConnectProxy(http.server.BaseHTTPRequestHandler):
    def do_CONNECT(self):
        CONNECTS.append(stamp('https-proxy-connect',target=self.path))
        host,port=self.path.rsplit(':',1)
        if host not in ['127.0.0.1','localhost','192.0.2.1']: self.send_error(403);return
        try:
            upstream=socket.create_connection((host,int(port)),timeout=5)
            self.send_response(200);self.end_headers();self.wfile.flush()
            a=threading.Thread(target=f.pump,args=(self.connection,upstream,False),daemon=True)
            a.start();f.pump(upstream,self.connection,False);a.join(2)
        except OSError: pass
    def log_message(self,*args): pass

def execute(name,path='archive',mode='off',ca='correct',metadata=None,flags=(),env_extra=None,cancel=None,url=None):
    case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();scratch=case/'scratch';scratch.mkdir();settings=case/'settings';settings.mkdir()
    stored=copy.deepcopy(SETTINGS);stored['peerMode']=mode;stored['nexusModsApiKey']='fixture-stored-secret'
    (settings/'settings.json').write_text(json.dumps(stored));guide=case/'guide.json'
    meta=copy.deepcopy(META if metadata is None else metadata)
    guide.write_text(json.dumps({'Components':[f.component(meta,url=url or BASE+'/'+path)]}))
    argv=[str(f.BIN),'fetch-downloads','--input',str(guide),'--source-dir',str(mods),'--scratch-dir',str(scratch),'--settings-dir',str(settings),'--peer-mode',mode,'--progress-json','--retries','2']
    if ca is not None: argv+=['--extra-ca-file',str(CERT if ca=='correct' else WRONG if ca=='wrong' else INVALID if ca=='invalid' else MALFORMED if ca=='malformed' else ROOT/'missing.pem')]
    argv+=list(flags);env=ENV.copy();env.update(env_extra or {})
    before=f.snapshot(case);envbefore={k:f.snapshot(pathlib.Path(ENV[k])) for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']}
    cb=len(f.connects);tb=len(TIMELINE);pb=len(CONNECTS);events=[];lines=[]
    started=time.monotonic();proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env);f.children.append(proc)
    def read():
        for line in proc.stdout:
            lines.append(line)
            try:
                obj=json.loads(line)
                if obj.get('type')=='acquisition': events.append(dict(time=time.monotonic(),event=obj['event']))
            except ValueError: pass
    reader=threading.Thread(target=read);reader.start()
    if cancel:
        deadline=time.monotonic()+15
        while proc.poll() is None and time.monotonic()<deadline:
            if cancel=='backoff' and any(r.get('path','').endswith('/backoff') for r in TIMELINE[tb:]): time.sleep(.3);break
            if cancel=='body' and any(e['event']['phase']['phase']=='transferring' for e in events):break
            if cancel=='race' and len(f.connects)>cb: break
            time.sleep(.02)
        stamp('sigint',case=name,pid=proc.pid);proc.send_signal(signal.SIGINT)
    try: code=proc.wait(timeout=35)
    except subprocess.TimeoutExpired: proc.kill();proc.wait();raise RuntimeError(name+' timed out')
    reader.join();stderr=proc.stderr.read();stdout=''.join(lines);elapsed=time.monotonic()-started
    objects=[json.loads(line) for line in lines];summary=objects[-1].get('summary') if objects else None
    row=dict(name=name,guideInput=json.loads(guide.read_text()),settingsInput=stored,argv=argv,environment={k:env[k] for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']},envOverrides=env_extra or {},exit=code,stdout=stdout,stderr=stderr,elapsed=elapsed,events=events,summary=summary,serverTimeline=copy.deepcopy(TIMELINE[tb:]),peerConnections=copy.deepcopy(f.connects[cb:]),httpsProxyConnections=copy.deepcopy(CONNECTS[pb:]),before=before,after=f.snapshot(case),envBefore=envbefore,envAfter={k:f.snapshot(pathlib.Path(ENV[k])) for k in envbefore})
    (OUT/(name+'.json')).write_text(json.dumps(row,indent=2)+'\n');RUNS.append(row)
    check(name+'_scratch_clean',list(scratch.iterdir()),[])
    check(name+'_no_staging',[str(p.relative_to(mods)) for p in mods.rglob('*') if p.name.startswith(('.part','.tmp','.peer'))],[])
    check(name+'_settings_unchanged',before['settings/settings.json'],row['after']['settings/settings.json'])
    check(name+'_no_ca_persistence',any('BEGIN CERTIFICATE' in p.read_text(errors='ignore') for p in settings.rglob('*') if p.is_file()),False)
    if summary:
        check(name+'_summary_last',objects[-1]['type'],'summary')
        check(name+'_event_envelopes',all(o.get('type')=='acquisition' and 'event' in o for o in objects[:-1]))
        for index,e in enumerate(events):
            check(name+'_identity_'+str(index),e['event']['componentGuid'],'aaaaaaaa-1111-4111-8111-111111111111')
            check(name+'_index_'+str(index),e['event']['targetIndex'],0)
            if e['event']['filename'] is not None:check(name+'_filename_'+str(index),e['event']['filename'],'example.zip')
    row['mods']=mods
    return row

def published(r,winner=None):
    check(r['name']+'_exit',r['exit'],0);check(r['name']+'_files',sorted(p.name for p in r['mods'].iterdir()),['example.zip'])
    check(r['name']+'_hash',f.digest(r['mods']/'example.zip') if (r['mods']/'example.zip').exists() else None,f.digest(ARCHIVE))
    check(r['name']+'_completed_once',sum(e['event']['phase']['phase']=='completed' for e in r['events']),1)
    if winner: check(r['name']+'_winner',r['summary']['report']['outcomes'][0]['source']['kind'],winner)

def unpublished(r):check(r['name']+'_unpublished',list(r['mods'].iterdir()),[])
def nopeer(r):
    check(r['name']+'_zero_peer_connections',r['peerConnections'],[])
    check(r['name']+'_zero_peer_events',[e for e in r['events'] if e['event']['source']['kind']=='peer'],[])
def ordering(r):
    times=[e['time'] for e in r['events'] if e['event']['source']['kind']=='origin' and e['event']['phase']['phase']=='transferring']
    check(r['name']+'_real_peer_connected',bool(r['peerConnections']))
    check(r['name']+'_gate_clear_before_peer',bool(times) and bool(r['peerConnections']) and min(times)<=min(c['time'] for c in r['peerConnections']))

def main():
    global CERT,WRONG,INVALID,MALFORMED,PAYLOAD,BASE,OUTSIDE,HTTP,SETTINGS,META,ARCHIVE,IPV6
    error=None
    try:
        key,CERT=certificate('correct');_,WRONG=certificate('wrong');INVALID=ROOT/'invalid.pem';INVALID.write_text('not a certificate');MALFORMED=ROOT/'malformed.pem';MALFORMED.write_text('-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n')
        leafkey=ROOT/'leaf.key';leaf=ROOT/'leaf.pem';csr=ROOT/'leaf.csr';ext=ROOT/'leaf.ext'
        ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,DNS:localhost.evil,DNS:public.example,IP:127.0.0.1,IP:192.0.2.1,IP:::1\n')
        commands=[['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(leafkey),'-out',str(csr),'-subj','/CN=Loopback Server'],['openssl','x509','-req','-in',str(csr),'-CA',str(CERT),'-CAkey',str(key),'-CAcreateserial','-out',str(leaf),'-days','1','-extfile',str(ext)]]
        for cmd in commands:subprocess.run(cmd,check=True,capture_output=True)
        (OUT/'leaf.openssl.json').write_text(json.dumps(commands,indent=2))
        ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(leaf,leafkey)
        relay=socket.socket();relay.bind(('127.0.0.1',0));relay.listen(64);OPEN.append(relay)
        threading.Thread(target=f.relay_loop,args=(relay,),daemon=True).start()
        ARCHIVE,magnet,info,peer=f.make_seed();PAYLOAD=ARCHIVE.read_bytes()
        subprocess.run(['ip','addr','add','192.0.2.1/32','dev','lo'],check=True)
        SETTINGS={'peerSharing':True,'peerMode':'race','peerProxies':['socks5://127.0.0.1:'+str(relay.getsockname()[1])],'peerProxyListUrl':'','peerTrackerListUrl':None,'peerTrackers':[],'peerExtraPeers':[peer]}
        META={'Files':{'example.zip':True},'ContentHashSHA256':f.digest(ARCHIVE),'Magnet':magnet,'TorrentInfo':info}
        origin=server('127.0.0.1',0,'loopback',ctx);BASE='https://127.0.0.1:'+str(origin.server_port)
        ipv6=server('::1',0,'ipv6',ctx);IPV6='https://[::1]:'+str(ipv6.server_port)
        outside=server('192.0.2.1',0,'nonloopback',ctx);OUTSIDE='https://192.0.2.1:'+str(outside.server_port)
        plain=server('127.0.0.1',0,'http');HTTP='http://127.0.0.1:'+str(plain.server_port)
        outside_plain=server('192.0.2.1',0,'nonloopback-http');outside_http='http://192.0.2.1:'+str(outside_plain.server_port)
        proxy=http.server.ThreadingHTTPServer(('127.0.0.1',0),ConnectProxy);SERVERS.append(proxy);threading.Thread(target=proxy.serve_forever,daemon=True).start()
        # Reachability is established with normal certificate verification, before negatives.
        import urllib.request
        trusted=ssl.create_default_context(cafile=str(CERT));opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=trusted))
        for host in ['192.0.2.1','public.example','localhost.evil']:
            url=OUTSIDE.replace('192.0.2.1',host)+'/archive';data=opener.open(url,timeout=5).read();check('reachable_'+host,hashlib.sha256(data).hexdigest(),f.digest(ARCHIVE))
        for host in ['public.example','localhost.evil']:
            check('reachable_http_'+host,hashlib.sha256(opener.open(outside_http.replace('192.0.2.1',host)+'/archive',timeout=5).read()).hexdigest(),f.digest(ARCHIVE))
        check('reachable_http',hashlib.sha256(opener.open(HTTP+'/archive',timeout=5).read()).hexdigest(),f.digest(ARCHIVE))
        for ca in [None,'wrong','invalid','missing','malformed']:
            r=execute('ca_'+str(ca),ca=ca);check(r['name']+'_failed',r['exit']!=0);unpublished(r)
            if ca in [None,'wrong']:check(r['name']+'_certificate_failure','invalid peer certificate' in r['stdout'])
        r=execute('ca_correct');published(r)
        r=execute('ipv6_loopback',url='https://[::1]:'+str(ipv6.server_port)+'/archive');published(r)
        r=execute('localhost_correct',url=BASE.replace('127.0.0.1','localhost')+'/archive');published(r)
        # A malicious hosts mapping cannot move localhost's override away from loopback.
        hosts.write_text('192.0.2.1 localhost public.example localhost.evil\n::1 ip6-localhost\n')
        check('localhost_system_resolution_is_nonloopback',socket.gethostbyname('localhost'),'192.0.2.1')
        r=execute('localhost_pinned',url=BASE.replace('127.0.0.1','localhost')+'/archive');published(r)
        for name,url in [('initial_public',OUTSIDE.replace('192.0.2.1','public.example')+'/archive'),('initial_ip',OUTSIDE+'/archive'),('initial_deceptive',OUTSIDE.replace('192.0.2.1','localhost.evil')+'/archive'),('initial_http',HTTP+'/archive'),('initial_public_http',outside_http.replace('192.0.2.1','public.example')+'/archive'),('initial_deceptive_http',outside_http.replace('192.0.2.1','localhost.evil')+'/archive')]:
            r=execute(name,url=url);unpublished(r);check(name+'_no_request',r['serverTimeline'],[]);check(name+'_rejected',r['exit']!=0)
        for path in ['redirect','chain','redirect-localhost','redirect-ipv6']:
            r=execute(path,path);published(r)
        for path in ['downgrade','escape','deceptive','loop','credentials-redirect']:
            r=execute(path,path);unpublished(r);check(path+'_failed',r['exit']!=0);check(path+'_no_outside_request',any(x.get('server') in ['nonloopback','http'] for x in r['serverTimeline']),False)
            check(path+'_not_manual',r['summary']['report']['outcomes'][0]['status'],'failed')
            if path=='credentials-redirect':check('redirect_error_redacted','fixture-redact' in r['stdout']+r['stderr'],False)
        p_env={'HTTPS_PROXY':'http://127.0.0.1:'+str(proxy.server_port),'NO_PROXY':'','no_proxy':''}
        r=execute('proxy_control',ca=None,env_extra=p_env);check('ordinary_proxy_contact',bool(r['httpsProxyConnections']));unpublished(r)
        r=execute('proxy_override',env_extra=p_env);published(r);check('override_proxy_zero',r['httpsProxyConnections'],[])
        for mode in ['race','fallback']:
            for path,reason in [('gate-login','loginRequired'),('gate-body','captcha'),('gate-header','captcha'),('gate-wait','freeUserWait')]:
                r=execute(path+'_'+mode,path,mode);check(r['name']+'_exit',r['exit'],3);unpublished(r);nopeer(r)
                check(r['name']+'_reason',r['summary']['report']['outcomes'][0].get('manual') and r['summary']['report']['outcomes'][0]['manual'].get('reason'),reason)
        f.throttle.set();r=execute('origin_wins','origin-win','race');f.throttle.clear();published(r,'origin');ordering(r)
        r=execute('peer_wins','peer-win','race');published(r,'peer');ordering(r)
        r=execute('retry429','retry429');published(r);check('retry429_attempts',len([x for x in r['serverTimeline'] if x['kind']=='request']),3)
        r=execute('retry503','retry503');unpublished(r);check('retry503_attempts',len([x for x in r['serverTimeline'] if x['kind']=='request']),3);check('retry503_bounded',r['elapsed']<10)
        for name,path,mode,when in [('cancel_body','cancel-body','off','body'),('cancel_backoff','backoff','off','backoff'),('cancel_race','cancel-race','race','race')]:
            f.throttle.set();r=execute(name,path,mode,cancel=when);f.throttle.clear();unpublished(r);check(name+'_exit',r['exit'],130);check(name+'_cancelled_report',r['summary']['report']['cancelled'],True);check(name+'_prompt',r['elapsed']<15)
            if mode=='race': ordering(r)
        unknown=copy.deepcopy(META);unknown.pop('Files')
        r=execute('unknown_name','unknown',metadata=unknown);published(r)
        for path in ['unsafe','unknown-empty']:
            r=execute(path,path,metadata=unknown);unpublished(r);check(path+'_failed',r['exit']!=0)
        bad=copy.deepcopy(META);bad['ContentHashSHA256']='0'*64
        r=execute('bad_hash',metadata=bad);unpublished(r);check('bad_hash_failed',r['exit']!=0)
        corrupt=copy.deepcopy(META);corrupt.pop('ContentHashSHA256')
        r=execute('corrupt','corrupt',metadata=corrupt);unpublished(r);check('corrupt_failed',r['exit']!=0)
        r=execute('budget',flags=['--max-expanded-bytes','1']);unpublished(r);check('budget_failed',r['exit']!=0)
        for invalid_ca in ['invalid','missing','malformed']:
            r=execute('dry_run_'+invalid_ca,ca=invalid_ca,flags=['--dry-run']);check(r['name']+'_failed',r['exit']!=0);check(r['name']+'_no_network',r['serverTimeline'],[]);check(r['name']+'_no_writes',r['before'],r['after'])
        r=execute('dry_run','archive','race',flags=['--dry-run','--record']);check('dry_run_exit',r['exit'],0);check('dry_run_case_unchanged',r['before'],r['after']);check('dry_run_environment_unchanged',r['envBefore'],r['envAfter']);check('dry_run_no_network',r['serverTimeline'],[]);nopeer(r)
        r=execute('api_no_explicit_key',url='https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1',flags=['--nexus-api-base',HTTP+'/v1/'],env_extra={'NEXUS_MODS_API_KEY':'fixture-environment-secret','KOTOR_MODSYNC_NEXUS_API_KEY':'fixture-environment-secret'});check('api_override_requires_explicit',r['exit'],1);check('api_override_no_requests',r['serverTimeline'],[]);unpublished(r)
        r=execute('api_explicit_key',url='https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1',flags=['--nexus-api-base',HTTP+'/v1/','--nexus-mods-api-key','fixture-explicit-key'],env_extra={'NEXUS_MODS_API_KEY':'fixture-environment-secret','KOTOR_MODSYNC_NEXUS_API_KEY':'fixture-environment-secret'});published(r);check('api_explicit_no_secrets_to_archive',any('fixture-' in str(x.get('headers',{})) for x in r['serverTimeline'] if x.get('server')=='loopback'),False)
        api_requests=[x for x in r['serverTimeline'] if x['kind']=='request' and x['path'].startswith('/v1/')]
        check('api_explicit_positive_control',len(api_requests),3)
        check('api_only_explicit_key',all(x['headers'].get('apikey')=='fixture-explicit-key' for x in api_requests))
        check('api_no_stored_environment_secrets',any('fixture-stored-secret' in str(x['headers']) or 'fixture-environment-secret' in str(x['headers']) for x in api_requests),False)
    except Exception:
        error=traceback.format_exc();print(error,flush=True)
    finally:
        for p in f.children:
            if p.poll() is None:
                p.terminate()
                try:p.wait(timeout=8)
                except subprocess.TimeoutExpired:p.kill();p.wait()
        if f.children and f.children[0].stdout:
            (OUT/'seed.stdout').write_text((OUT/'seed.first.json').read_text()+f.children[0].stdout.read())
        for s in SERVERS:s.shutdown();s.server_close()
        for s in OPEN:s.close()
        check('source_frozen',SOURCE,{str(p.relative_to(REPO)):f.digest(p) for p in sorted((REPO/'src').rglob('*.rs'))})
        check('proof_sources_frozen',PROOF_SOURCES,{path:f.digest(REPO/path) for path in PROOF_SOURCES});check('binary_frozen',BINARY,f.digest(f.BIN));check('children_reaped',all(p.poll() is not None for p in f.children))
        receipt=dict(fixtureArchiveManifest=dict(sha256=f.digest(ARCHIVE),bytes=ARCHIVE.stat().st_size,zipMembers=[dict(name=i.filename,size=i.file_size,crc32=i.CRC) for i in f.zipfile.ZipFile(ARCHIVE).infolist()]) if 'ARCHIVE' in globals() else None,caseNames=[r['name'] for r in RUNS],proofSourceSha256=PROOF_SOURCES,binary=str(f.BIN),binarySha256=BINARY,sourceSha256=SOURCE,namespace=os.readlink('/proc/self/ns/net'),parentNamespace=os.environ['LOOPBACK_PARENT_NETNS'],addresses=subprocess.check_output(['ip','-j','addr'],text=True),routes=subprocess.check_output(['ip','route'],text=True),root=str(ROOT),checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),error=error,children=[dict(pid=p.pid,exit=p.returncode) for p in f.children],timeline=TIMELINE,peerConnections=f.connects,httpsProxyConnections=CONNECTS)
        (OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,default=str)+'\n')
        print(json.dumps(dict(passed=receipt['passed'],total=receipt['total'],error=error,failed=[c['name'] for c in CHECKS if not c['passed']])),flush=True)
        # Namespace dies with this process. Remove only this fixture's own temporary data.
        subprocess.run(['umount','/etc/hosts'],check=True);shutil.rmtree(ROOT)
    return 0 if error is None and all(c['passed'] for c in CHECKS) else 1
if __name__=='__main__':sys.exit(main())
