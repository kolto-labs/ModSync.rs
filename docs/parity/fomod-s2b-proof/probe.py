#!/usr/bin/env python3
"""FOMOD S2b production receipts: run-scoped per-archive choices, Unicode name matching,
the FOMOD gate on plain validate/install and mapper fallback warnings. Real
fetch-downloads/fomod-inspect/validate/install in one isolated network namespace with a
loopback HTTPS origin. Generic fixtures only; no Rust harness."""
import atexit, ctypes, hashlib, http.server, io, json, os, pathlib, shutil, ssl, subprocess, tempfile, threading, time, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fomod-s2b'))).resolve()
_MARK=OUT/'.modsync-proof-out'
if OUT.exists() and any(OUT.iterdir()) and not _MARK.exists() and (TARGET/'cloud-proof').resolve() not in OUT.parents:raise SystemExit(f'Refusing to delete {OUT}: not an empty or earlier probe output directory (set MODSYNC_PROOF_OUT to a dedicated folder)')
shutil.rmtree(OUT,ignore_errors=True);OUT.mkdir(parents=True);_MARK.write_text('')
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
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-s2b-'))
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

# Generic installers. Destinations are directories unless a case needs a rename.
def plugin(name,source,dest='Override',flags=()):
 f=''.join(f'<flag name="{n}">{v}</flag>' for n,v in flags)
 return f'<plugin name="{name}"><description>Generic option</description><files><file source="{source}" destination="{dest}"/></files>'+(f'<conditionFlags>{f}</conditionFlags>' if f else '')+'<typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
def step(name,group,kind,plugins,visible=''):
 return f'<installStep name="{name}">{visible}<optionalFileGroups><group name="{group}" type="{kind}"><plugins>{"".join(plugins)}</plugins></group></optionalFileGroups></installStep>'
def config(steps,patterns=''):
 cond=f'<conditionalFileInstalls><patterns>{patterns}</patterns></conditionalFileInstalls>' if patterns else ''
 return '<config><moduleName>Example Installer</moduleName><installSteps>'+''.join(steps)+'</installSteps>'+cond+'</config>'
def pack(xml,payload):
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml),*sorted(payload.items())]:
   z.writestr(zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0)),value)
 return b.getvalue()
def pair(names=('Alpha','Beta'),srcs=('a','b'),stepname='Choose',group='Variant',extra=()):
 plugins=[plugin(n,f'payload/{s}.txt') for n,s in zip(names,srcs)]+[plugin(n,f'payload/{s}.txt') for n,s in extra]
 return pack(config([step(stepname,group,'SelectExactlyOne',plugins)]),{f'payload/{s}.txt':f'{s} bytes\n' for s in list(srcs)+[s for _,s in extra]})
FIRST=pair();SECOND=pair(srcs=('c','d'))
def plain_zip():
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:z.writestr(zipfile.ZipInfo('payload/plain.txt',date_time=(2020,1,1,0,0,0)),'plain\n')
 return b.getvalue()
PLAIN=plain_zip()
# Unicode names: C# OrdinalIgnoreCase matches Ä/ä, Ö/ö, Ü/ü; ß never matches SS.
UNICODE=pair(names=('Ämber','Übel'),stepname='Ärger',group='Ölgruppe',extra=(('Straße','s'),))
# Flag names ignore case (Unicode); values stay case-sensitive.
def flags_archive(value):
 alpha=plugin('Alpha','payload/a.txt',flags=(('Ä','an'),))
 visible=f'<visible><dependencies operator="And"><flagDependency flag="ä" value="{value}"/></dependencies></visible>'
 pattern=f'<pattern><dependencies operator="And"><flagDependency flag="ä" value="{value}"/></dependencies><files><file source="payload/f.txt" destination="Override"/></files></pattern>'
 xml=config([step('Choose','Variant','SelectExactlyOne',[alpha,plugin('Beta','payload/b.txt')]),step('Extra','More','SelectAll',[plugin('Mehr','payload/e.txt')],visible)],pattern)
 return pack(xml,{f'payload/{s}.txt':f'{s} bytes\n' for s in 'abef'})
FLAGS=flags_archive('an');FLAGS_VALUE=flags_archive('AN')
# Every C# FomodToComponentMapper fallback: unevaluated file/game dependencies, both
# skipped conditional-pattern shapes and a dropped destination rename.
FALLBACK_XML=config([
 step('Choose','Variant','SelectExactlyOne',[plugin('Alpha','payload/a.txt','Override/renamed.txt'),plugin('Beta','payload/b.txt',flags=(('Picked','yes'),))]),
 step('Hidden','Extra','SelectAll',[plugin('Mehr','payload/e.txt')],'<visible><dependencies operator="And"><fileDependency file="Example.esp" state="Active"/></dependencies></visible>')],
 '<pattern><dependencies operator="Or"><flagDependency flag="Picked" value="yes"/><flagDependency flag="Other" value="yes"/></dependencies><files><file source="payload/o.txt" destination="Override"/></files></pattern>'
 '<pattern><dependencies operator="And"><flagDependency flag="Missing" value="on"/></dependencies><files><file source="payload/m.txt" destination="Override"/></files></pattern>'
 '<pattern><dependencies operator="And"><gameDependency version="1.0"/><flagDependency flag="Picked" value="yes"/></dependencies><files><file source="payload/g.txt" destination="Override"/></files></pattern>')
FALLBACK=pack(FALLBACK_XML,{f'payload/{s}.txt':f'{s} bytes\n' for s in 'abegmo'})
FALLBACK_WARNINGS=["fileDependency 'Example.esp' is not evaluated; treating as satisfied.","gameDependency '1.0' is not evaluated; treating as satisfied.",
 'Skipping a conditionalFileInstalls pattern: only a flat And of flagDependency elements is supported in this slice.',
 'Skipping a conditionalFileInstalls pattern: its flag dependencies are not all set by a single plugin.',
 "FOMOD wants to install 'a.txt' as 'renamed.txt'; renames are not supported by the Copy action, the source file name is kept."]
MAPPER='[Warning] [FomodToComponentMapper] '
SERVE={'/fallback.zip':FALLBACK,'/unicode.zip':UNICODE,'/example.zip':FIRST}

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
def extract(name):return {'Action':'Extract','Source':[f'<<modDirectory>>/{name}']}
NOTE={'Action':'Copy','Source':['<<modDirectory>>/notes.txt'],'Destination':'<<kotorDirectory>>/Override'}
def sel(plugin,stepname='Choose',group='Variant'):return {'stepName':stepname,'groupName':group,'plugins':[plugin]}
def archive_choices(filename,*selections):return {'archiveFileName':filename,'selections':list(selections)}
def choices(*archives):return {'version':1,'archives':list(archives)}
def entry(filename,plugin='Beta'):return archive_choices(filename,sel(plugin))
def registry(files,status=None,url=None):
 meta={'Files':{f:True for f in files},'custom':'keep'}
 if status:meta['HandlerMetadata']={'fomodPromptStatus':dict(status),'unrelated':'keep'}
 return {url or (BASE+'/'+files[0]):meta}
def comp(i=0,instructions=(),saved=None,files=(),status=None,url=None,**extra):
 c={'Guid':GUIDS[i],'Name':f'Example Consumer {i+1}','IsSelected':True,'Instructions':list(instructions),'CustomField':'keep',**extra}
 if files:c['ResourceRegistry']=registry(list(files),status,url)
 if saved is not None:c['FomodChoices']=saved
 return c
def setup(name,components,existing):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();sd=case/'settings';sd.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components},ensure_ascii=False),encoding='utf-8')
 (sd/'settings.json').write_text(json.dumps({'peerSharing':False,'usePublicPeerList':False,'telemetryOptIn':False}))
 for filename,data in existing.items():(mods/filename).write_bytes(data)
 return dict(name=name,case=case,guide=guide,mods=mods,settings=sd,scratch=scratch,runs=0)
def components(c):
 data=json.loads(c['guide'].read_text(encoding='utf-8'));return data['Components'] if isinstance(data,dict) else data
def execute(c,label,args,text=''):
 c['runs']+=1;name=f"{c['name']}_{c['runs']:02d}_{label}";before=snapshot(c['case'])
 shutil.copytree(c['case'],OUT/'fixtures'/name/'before',dirs_exist_ok=True)
 r=subprocess.run(args,input=text,capture_output=True,text=True,env=ENV,timeout=60)
 shutil.copytree(c['case'],OUT/'fixtures'/name/'after',dirs_exist_ok=True)
 try:data=json.loads(r.stdout)
 except Exception:data=None
 row=dict(name=name,argv=args,exit=r.returncode,stdout=r.stdout,stderr=r.stderr,data=data,before=before,after=snapshot(c['case']))
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');RUNS.append(row);return row
def sidecar(c,value):
 path=c['case']/f"choices-{c['runs']+1:02d}.json";path.write_text(json.dumps(value,ensure_ascii=False),encoding='utf-8');return ['--fomod-choices',str(path)]
def kept(t):return {k:v for k,v in t.items() if not k.startswith('scratch/')}
def validate(c,label,report=True,flags=()):
 game=c['case']/'unused-game';(game/'Override').mkdir(parents=True,exist_ok=True)
 args=[str(BIN),'validate','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(c['mods']),'--scratch-dir',str(c['scratch']),*(['--validation-report'] if report else []),*flags]
 r=execute(c,label,args)
 # Validation is pure: the guide, mod and game trees are byte-identical (scratch aside).
 check(r['name']+'_trees_unchanged',kept(r['after']),kept(r['before']))
 return r
def stage(r):return {'messages':[],'hasWarnings':False,'passed':None}|next((s for s in (r['data'] or {}).get('stages',[]) if s['stage']=='FomodConfiguration'),{})
def install(c,label,flags=(),game=None,mods=None):
 # Install into private copies; the guide is never written by validate/install.
 k=c['runs']+1
 if mods is None:mods=c['case']/f'install-{k:02d}-mods';shutil.copytree(c['mods'],mods)
 if game is None:game=c['case']/f'install-{k:02d}-game';game.mkdir()
 mods_before=snapshot(mods)
 r=execute(c,label,[str(BIN),'install','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(mods),'--scratch-dir',str(c['scratch']),*flags])
 r['game']={str(p.relative_to(game)):p.read_text() for p in sorted(game.rglob('*')) if p.is_file() and '.modsync' not in p.parts}
 r['mods_unchanged']=snapshot(mods)==mods_before
 check(r['name']+'_guide_unchanged',r['after'].get('guide.json'),r['before'].get('guide.json'))
 return r
def fetch(c,label,flags=()):
 args=[str(BIN),'fetch-downloads','--input',str(c['guide']),'--source-dir',str(c['mods']),'--settings-dir',str(c['settings']),'--scratch-dir',str(c['scratch']),'--peer-mode','off','--retries','0','--extra-ca-file',str(cert),*flags]
 r=execute(c,label,args)
 r['consumers']=[v['outcome'] for v in (r['data'] or {}).get('fomodConfiguration',{}).get('consumers',[])]
 return r
def mapper_lines(text):return [l for l in text.splitlines() if MAPPER in l]
# C#'s exact text: the tag and the warning, with no archive prefix.
def warned(text):return [w for w in FALLBACK_WARNINGS if any(l.endswith(MAPPER+w) for l in text.splitlines())]
def files(*names):return {f'Override/{n}.txt':f'{n} bytes\n' for n in names}

# 1. Run-scoped --fomod-choices merges per archive (Rust extension; C# validate/install
# take no choices file). A file entry replaces only that archive's embedded choices.
EMBEDDED=choices(entry('example.zip','Alpha'),entry('second.zip','Alpha'))
BOTH={'example.zip':FIRST,'second.zip':SECOND}
merge=[comp(instructions=(extract('example.zip'),extract('second.zip')),saved=EMBEDDED,files=('example.zip','second.zip'),status={'example.zip':'configured','second.zip':'configured'})]
c=setup('merge',merge,BOTH);g0=digest(c['guide'])
ri=install(c,'control_embedded');check('merge_control_exit',ri['exit'],0);check('merge_control_bytes',ri['game'],files('a','c'))
ri=install(c,'one_entry',sidecar(c,choices(entry('example.zip'))));check('merge_install_exit',ri['exit'],0);check('merge_install_bytes',ri['game'],files('b','c'))
rv=validate(c,'one_entry_report',flags=sidecar(c,choices(entry('example.zip'))));check('merge_report_exit',rv['exit'],0);check('merge_report_stage_passed',stage(rv)['passed'],True)
rv=validate(c,'one_entry_plain',report=False,flags=sidecar(c,choices(entry('example.zip'))));check('merge_plain_exit',rv['exit'],0)
# Archive-name matching of the file entry ignores case: it replaces, never duplicates.
ri=install(c,'entry_case',sidecar(c,choices(entry('EXAMPLE.ZIP'))));check('merge_case_exit',ri['exit'],0);check('merge_case_bytes',ri['game'],files('b','c'))
ri=install(c,'both_entries',sidecar(c,choices(entry('example.zip'),entry('second.zip'))));check('merge_both_exit',ri['exit'],0);check('merge_both_bytes',ri['game'],files('b','d'))
# Negative: an invalid entry for one archive fails the run; nothing is installed.
ri=install(c,'invalid_entry',sidecar(c,choices(entry('example.zip','Unknown'))));check('merge_invalid_exit',ri['exit'],1);check('merge_invalid_game_untouched',ri['game'],{})
check('merge_invalid_actionable',"FOMOD plugin 'Unknown'" in ri['stderr'] and 'do not apply to the current archive' in ri['stderr'])
rv=validate(c,'invalid_entry_report',flags=sidecar(c,choices(entry('example.zip','Unknown'))));check('merge_invalid_report_exit',rv['exit'],1)
check('merge_guide_never_persisted',digest(c['guide']),g0);check('merge_saved_unchanged',components(c)[0]['FomodChoices'],EMBEDDED)
# Unparseable embedded choices are refused with the repair hint, never silently replaced.
REPAIR="Repair this component's FomodChoices in the guide"
bad=[comp(instructions=(extract('example.zip'),),saved={'version':2,'archives':[entry('example.zip','Alpha')]})]
c=setup('merge_unparseable',bad,{'example.zip':FIRST})
ri=install(c,'unparseable_with_file',sidecar(c,choices(entry('example.zip'))));check('unparseable_install_exit',ri['exit'],1);check('unparseable_install_repair_hint',REPAIR in ri['stderr']);check('unparseable_install_game_untouched',ri['game'],{})
rv=validate(c,'unparseable_report',flags=sidecar(c,choices(entry('example.zip'))));check('unparseable_report_exit',rv['exit'],1)
# Positive control: the same guide with parseable embedded choices takes the file entry.
c=setup('merge_parseable',[comp(instructions=(extract('example.zip'),),saved=choices(entry('example.zip','Alpha')))],{'example.zip':FIRST})
ri=install(c,'parseable_with_file',sidecar(c,choices(entry('example.zip'))));check('parseable_install_exit',ri['exit'],0);check('parseable_install_bytes',ri['game'],files('b'))
# The file applies only to components that register or extract a named archive: an
# unrelated component's unparseable FomodChoices are left alone and do not fail the run.
UNPARSEABLE={'version':2,'archives':[entry('other.zip','Alpha')]}
scope=[comp(instructions=(extract('example.zip'),),saved=choices(entry('example.zip','Alpha'))),comp(1,instructions=(NOTE,),saved=UNPARSEABLE)]
c=setup('scope_unrelated',scope,{'example.zip':FIRST,'notes.txt':b'notes bytes\n'})
ri=install(c,'install',sidecar(c,choices(entry('example.zip'))));check('scope_unrelated_install_exit',ri['exit'],0);check('scope_unrelated_install_bytes',ri['game'],files('b','notes'))
rv=validate(c,'report',flags=sidecar(c,choices(entry('example.zip'))));check('scope_unrelated_report_exit',rv['exit'],0)
rv=validate(c,'plain',report=False,flags=sidecar(c,choices(entry('example.zip'))));check('scope_unrelated_plain_exit',rv['exit'],0)
# Controls: a registering-only component still takes the file entry (its stale embedded
# choice is replaced), and an unrelated one keeps its own entries untouched.
reg=[comp(instructions=(NOTE,),saved=choices(entry('example.zip','Unknown')),files=('example.zip',),status={'example.zip':'configured'})]
c=setup('scope_registered',reg,{'example.zip':FIRST,'notes.txt':b'notes bytes\n'})
rv=validate(c,'plain_embedded',report=False);check('scope_registered_embedded_exit',rv['exit'],1)
rv=validate(c,'plain_file',report=False,flags=sidecar(c,choices(entry('example.zip'))));check('scope_registered_file_exit',rv['exit'],0)
other=[comp(instructions=(extract('example.zip'),),saved=choices(entry('example.zip','Alpha'))),comp(1,instructions=(extract('second.zip'),),saved=choices(entry('second.zip','Alpha')))]
c=setup('scope_other_archive',other,BOTH)
ri=install(c,'install',sidecar(c,choices(entry('example.zip'))));check('scope_other_archive_exit',ri['exit'],0);check('scope_other_archive_bytes',ri['game'],files('b','c'))
# Scope follows the engine's source matching: a folder source extracts every archive
# below it, so the component takes the file entry. At c28dee0 only the last source
# component was compared and it did not. `[` and `]` are literal in a Source, as in
# C# PathHelper.WildcardPathMatch (see ../delete-directory-contract.md), so
# `[e]xample.zip` names no file: the scope still admits the entry (the FOMOD stage
# passes), and the run is refused as a missing source before any write. The
# reference refuses the same Extract with MissingSourceFiles
# (../delete-directory-proof, extract_bracket_literal).
# An archive literally named `[e]xample.zip` and named literally by the Source is in
# scope and installs (`bracket_literal`). It extracts to `[e]xample/`, next to the
# archive (an Extract Destination is never used; ../extract-destination-contract.md),
# so the mapped copies read from a folder named with brackets too.
for label,source,where,ok in [('folder','Pack','Pack/example.zip',True),('char_class','[e]xample.zip','example.zip',False),('bracket_literal','[e]xample.zip','[e]xample.zip',True)]:
 extract_step={'Action':'Extract','Source':[f'<<modDirectory>>/{source}']}
 c=setup(f'scope_{label}',[comp(instructions=(extract_step,))],{})
 (c['mods']/where).parent.mkdir(parents=True,exist_ok=True);(c['mods']/where).write_bytes(FIRST);n=f'scope_{label}'
 archive=where.rsplit('/',1)[-1]
 ri=install(c,'no_file');check(n+'_needs_choices_exit',ri['exit'],1);check(n+'_needs_choices_game_untouched',ri['game'],{})
 ri=install(c,'install',sidecar(c,choices(entry(archive))));check(n+'_install_exit',ri['exit'],0 if ok else 1);check(n+'_install_bytes',ri['game'],files('b') if ok else {})
 rv=validate(c,'report',flags=sidecar(c,choices(entry(archive))));check(n+'_report_exit',rv['exit'],0 if ok else 1);check(n+'_report_stage_passed',stage(rv)['passed'],True)
 rv=validate(c,'plain',report=False,flags=sidecar(c,choices(entry(archive))));check(n+'_plain_exit',rv['exit'],0 if ok else 1)
# Phased: base and remaining reconstruct the completed definitions from the same file;
# a different file changes them and is refused before any write.
wide=comp(1,instructions=(NOTE,),WidescreenOnly=True)
c=setup('merge_phased',[merge[0],wide],BOTH|{'notes.txt':b'notes bytes\n'})
pmods=c['case']/'phased-mods';shutil.copytree(c['mods'],pmods);pgame=c['case']/'phased-game';pgame.mkdir()
F=sidecar(c,choices(entry('example.zip')));F_path=F[1]
ri=install(c,'base',['--phase','base',*F],game=pgame,mods=pmods);check('phased_base_exit',ri['exit'],0);check('phased_base_bytes',ri['game'],files('b','c'))
ri=install(c,'remaining_other_file',['--phase','remaining',*sidecar(c,choices(entry('example.zip','Alpha')))],game=pgame,mods=pmods);check('phased_other_file_exit',ri['exit'],1);check('phased_other_file_game_unchanged',ri['game'],files('b','c'))
ri=install(c,'remaining_same_file',['--phase','remaining','--fomod-choices',F_path],game=pgame,mods=pmods);check('phased_remaining_exit',ri['exit'],0);check('phased_remaining_bytes',ri['game'],files('b','c','notes'))

# 2. Unicode name matching (C# OrdinalIgnoreCase) at every level: archive, step, group, plugin.
EXACT={'archive':'Ärchive.zip','step':'Ärger','group':'Ölgruppe','plugin':'Übel'}
FOLDED={'archive':'ärchive.zip','step':'äRGER','group':'öLGRUPPE','plugin':'üBEL'}
def unicode_choices(level=None,**override):
 n=dict(EXACT);n.update({level:FOLDED[level]} if level else {});n.update(override)
 return choices(archive_choices(n['archive'],sel(n['plugin'],n['step'],n['group'])))
for level in ['archive','step','group','plugin']:
 c=setup(f'unicode_{level}',[comp(instructions=(extract('Ärchive.zip'),),saved=unicode_choices(level))],{'Ärchive.zip':UNICODE})
 rv=validate(c,'report');check(f'unicode_{level}_report_exit',rv['exit'],0)
 rv=validate(c,'plain',report=False);check(f'unicode_{level}_plain_exit',rv['exit'],0)
 ri=install(c,'install');check(f'unicode_{level}_install_exit',ri['exit'],0);check(f'unicode_{level}_install_bytes',ri['game'],files('b'))
# Positive control: exact authored names pass everywhere.
c=setup('unicode_exact',[comp(instructions=(extract('Ärchive.zip'),),saved=unicode_choices())],{'Ärchive.zip':UNICODE})
ri=install(c,'install');check('unicode_exact_install_exit',ri['exit'],0);check('unicode_exact_install_bytes',ri['game'],files('b'))
# Negative controls: a different letter never matches, and ß is not SS (simple case mapping).
for label,override,needle in [('other_letter',{'plugin':'öBEL'},"FOMOD plugin 'öBEL'"),('sharp_s',{'plugin':'STRASSE'},"FOMOD plugin 'STRASSE'")]:
 c=setup(f'unicode_{label}',[comp(instructions=(extract('Ärchive.zip'),),saved=unicode_choices(**override))],{'Ärchive.zip':UNICODE})
 rv=validate(c,'report');check(f'unicode_{label}_report_exit',rv['exit'],1);check(f'unicode_{label}_actionable',any(needle in m for m in stage(rv)['messages']))
 ri=install(c,'install');check(f'unicode_{label}_install_exit',ri['exit'],1);check(f'unicode_{label}_game_untouched',ri['game'],{})
c=setup('unicode_sharp_s_exact',[comp(instructions=(extract('Ärchive.zip'),),saved=unicode_choices(plugin='STRAßE'))],{'Ärchive.zip':UNICODE})
ri=install(c,'install');check('unicode_sharp_s_exact_exit',ri['exit'],0);check('unicode_sharp_s_exact_bytes',ri['game'],files('s'))
# Greek iota-subscript letters: .NET's simple uppercase is the titlecase letter
# (U+1F80 -> U+1F88, U+1FB3 -> U+1FBC); Rust's full mapping would expand them.
IOTA=pair(names=('\u1f80\u03bb\u03c6\u03b1','\u1fb3\u03b2'))
for label,name,expected in [('iota_row','\u1f88\u039b\u03a6\u0391','a'),('iota_single','\u1fbc\u0392','b')]:
 c=setup(f'unicode_{label}',[comp(instructions=(extract('iota.zip'),),saved=choices(entry('iota.zip',name)))],{'iota.zip':IOTA})
 rv=validate(c,'report');check(f'unicode_{label}_report_exit',rv['exit'],0)
 ri=install(c,'install');check(f'unicode_{label}_install_exit',ri['exit'],0);check(f'unicode_{label}_install_bytes',ri['game'],files(expected))
# A run-scoped file entry with a folded archive name replaces the embedded entry.
c=setup('unicode_file_entry',[comp(instructions=(extract('Ärchive.zip'),),saved=unicode_choices(plugin='Ämber'))],{'Ärchive.zip':UNICODE})
ri=install(c,'embedded');check('unicode_embedded_bytes',ri['game'],files('a'))
ri=install(c,'folded_entry',sidecar(c,choices(archive_choices('ÄRCHIVE.ZIP',sel('üBEL','äRGER','öLGRUPPE')))));check('unicode_file_entry_exit',ri['exit'],0);check('unicode_file_entry_bytes',ri['game'],files('b'))
# Flag names ignore case for step visibility and conditional patterns; values do not.
c=setup('unicode_flags',[comp(instructions=(extract('flags.zip'),),saved=choices(entry('flags.zip','Alpha')))],{'flags.zip':FLAGS})
ri=install(c,'install');check('unicode_flags_exit',ri['exit'],0);check('unicode_flags_bytes',ri['game'],files('a','e','f'))
c=setup('unicode_flag_value',[comp(instructions=(extract('flags.zip'),),saved=choices(entry('flags.zip','Alpha')))],{'flags.zip':FLAGS_VALUE})
ri=install(c,'install');check('unicode_flag_value_exit',ri['exit'],0);check('unicode_flag_value_bytes',ri['game'],files('a'))
# Prompt status keys match the registered archive name case-insensitively.
def status_case(name,archive,key):
 c=setup(name,[comp(files=(archive,),status={key:'configured'})],{archive:UNICODE})
 rv=validate(c,'report');check(name+'_exit',rv['exit'],1);check(name+'_status_read',any('marked configured' in m for m in stage(rv)['messages']))
status_case('unicode_status','Ärchive.zip','ärchive.zip')
status_case('ascii_status','Example.zip','example.zip')
# Fetch matches a folded choices entry, and records the status in place under the
# existing (differently cased) key instead of adding a twin.
for label,name in [('folded_entry','ärchive.zip'),('exact_entry','Ärchive.zip')]:
 c=setup(f'unicode_fetch_{label}',[comp(instructions=(extract('Ärchive.zip'),),files=('Ärchive.zip',),status={'ärchive.zip':'dismissed'},url=BASE+'/unicode.zip')],{'Ärchive.zip':UNICODE})
 r=fetch(c,'record',['--record',*sidecar(c,unicode_choices(archive=name))]);n=f'unicode_fetch_{label}'
 check(n+'_exit',r['exit'],0);check(n+'_outcomes',r['consumers'],['configured'])
 meta=list(components(c)[0]['ResourceRegistry'].values())[0]
 check(n+'_status_in_place',meta['HandlerMetadata']['fomodPromptStatus'],{'ärchive.zip':'configured'})
 check(n+'_saved',[a['archiveFileName'] for a in components(c)[0].get('FomodChoices',{}).get('archives',[])],['Ärchive.zip'])
 ri=install(c,'install');check(n+'_install_exit',ri['exit'],0);check(n+'_install_bytes',ri['game'],files('b'))

# 3. The registered-archive FOMOD gate runs on plain validate and install (C#
# InstallationValidationPipeline and InstallationService), before any write.
def gate_case(name,saved=None,status=None,registered=('example.zip',),existing=None,extract_names=()):
 existing={'example.zip':FIRST,'notes.txt':b'notes bytes\n'} if existing is None else existing
 return setup(name,[comp(instructions=(NOTE,*map(extract,extract_names)),saved=saved,files=registered,status=status)],existing)
for name,saved,status,needle in [('gate_missing',None,None,'no explicit FomodChoices'),('gate_stale',choices(entry('example.zip','Unknown')),{'example.zip':'configured'},'do not apply to the current archive'),('gate_configured_only',None,{'example.zip':'configured'},'marked configured')]:
 c=gate_case(name,saved,status)
 rv=validate(c,'plain',report=False);check(name+'_plain_exit',rv['exit'],1);check(name+'_plain_actionable',needle in rv['stderr'] and '--fomod-choices' in rv['stderr'])
 ri=install(c,'install');check(name+'_install_exit',ri['exit'],1);check(name+'_install_actionable',needle in ri['stderr']);check(name+'_install_game_untouched',ri['game'],{});check(name+'_install_mods_unchanged',ri['mods_unchanged'])
# Missing registered archive: recorded prompt state fails closed; without state it passes.
c=gate_case('gate_archive_missing',status={'example.zip':'configured'},existing={'notes.txt':b'notes bytes\n'})
rv=validate(c,'plain',report=False);check('gate_archive_missing_plain_exit',rv['exit'],1);check('gate_archive_missing_actionable','missing from the mod directory' in rv['stderr'])
ri=install(c,'install');check('gate_archive_missing_install_exit',ri['exit'],1);check('gate_archive_missing_game_untouched',ri['game'],{})
c=gate_case('gate_archive_absent_no_state',existing={'notes.txt':b'notes bytes\n'})
rv=validate(c,'plain',report=False);check('gate_absent_no_state_plain_exit',rv['exit'],0)
ri=install(c,'install');check('gate_absent_no_state_install_exit',ri['exit'],0);check('gate_absent_no_state_bytes',ri['game'],files('notes'))
# Positive controls: valid registered-only choices pass with the no-route warning on the
# ordinary event stream; a registered non-FOMOD archive is not gated.
ROUTE="no selected Extract instruction installs it; configuration alone installs no files."
c=gate_case('gate_valid',choices(entry('example.zip')),{'example.zip':'configured'})
rv=validate(c,'plain',report=False);check('gate_valid_plain_exit',rv['exit'],0);check('gate_valid_plain_warning',sum(ROUTE in l and l.startswith(' / 0: WARNING: ') for l in rv['stdout'].splitlines()),1)
ri=install(c,'install');check('gate_valid_install_exit',ri['exit'],0);check('gate_valid_install_bytes',ri['game'],files('notes'));check('gate_valid_install_warning',ROUTE in ri['stdout'])
c=gate_case('gate_valid_extract',choices(entry('example.zip')),{'example.zip':'configured'},extract_names=('example.zip',))
rv=validate(c,'plain',report=False);check('gate_extract_plain_exit',rv['exit'],0);check('gate_extract_no_warning','WARNING' in rv['stdout'],False)
c=gate_case('gate_plain_archive',existing={'example.zip':PLAIN,'notes.txt':b'notes bytes\n'})
rv=validate(c,'plain',report=False);check('gate_plain_archive_exit',rv['exit'],0)
ri=install(c,'install');check('gate_plain_archive_install_exit',ri['exit'],0);check('gate_plain_archive_bytes',ri['game'],files('notes'))

# 4. Mapper fallbacks are logged where C# FomodToComponentMapper logs warnings, on stderr
# for every verb that maps; reports and stdout JSON are unchanged.
fb=[comp(instructions=(extract('fallback.zip'),),saved=choices(entry('fallback.zip')),files=('fallback.zip',),status={'fallback.zip':'configured'})]
c=setup('fallback',fb,{'fallback.zip':FALLBACK})
# Since f68dcfc (docs/parity/fomod-visible-fallbacks.md) validate/install report the mapper
# fallbacks as FomodConfiguration warnings, once each, prefixed with the archive name.
# fomod-inspect and Fetch still log the C# mapper lines on stderr (checked below).
def reported(lines):return [w for w in FALLBACK_WARNINGS if any(l.endswith("WARNING: FOMOD archive 'fallback.zip': "+w) for l in lines)]
rv=validate(c,'report');check('fallback_report_exit',rv['exit'],0);check('fallback_report_warnings',reported(stage(rv)['messages']),FALLBACK_WARNINGS)
check('fallback_report_once',len(stage(rv)['messages']),len(FALLBACK_WARNINGS));check('fallback_report_counted',(rv['data']['warningCount'],stage(rv)['hasWarnings']),(len(FALLBACK_WARNINGS),True))
rv=validate(c,'plain',report=False);check('fallback_plain_exit',rv['exit'],0);check('fallback_plain_warnings',reported(rv['stdout'].splitlines()),FALLBACK_WARNINGS)
ri=install(c,'install');check('fallback_install_exit',ri['exit'],0);check('fallback_install_warnings',reported(ri['stdout'].splitlines()),FALLBACK_WARNINGS)
# Beta: its flag attaches the game-dependency pattern; the hidden step shows (file dependency satisfied).
check('fallback_install_bytes',ri['game'],files('b','e','g'))
r=execute(c,'inspect',[str(BIN),'fomod-inspect',str(c['mods']/'fallback.zip'),'--scratch-dir',str(c['scratch'])])
check('fallback_inspect_exit',r['exit'],0);check('fallback_inspect_stdout_json',r['data'] is not None);check('fallback_inspect_warnings',warned(r['stderr']),FALLBACK_WARNINGS)
cf=setup('fallback_fetch',[comp(instructions=(extract('fallback.zip'),),files=('fallback.zip',),url=BASE+'/fallback.zip')],{})
r=fetch(cf,'configure',['--record',*sidecar(cf,choices(entry('fallback.zip')))]);check('fallback_fetch_exit',r['exit'],0);check('fallback_fetch_outcomes',r['consumers'],['configured'])
check('fallback_fetch_warnings',warned(r['stderr']),FALLBACK_WARNINGS);check('fallback_fetch_stdout_json',r['data'] is not None)
# Resolved on the next Fetch: nothing is configured, so nothing is mapped or logged.
r=fetch(cf,'resolved',['--non-interactive']);check('fallback_fetch_resolved_outcomes',r['consumers'],['resolved']);check('fallback_fetch_resolved_quiet',mapper_lines(r['stderr']),[])
# Control: an installer without fallbacks logs nothing on any verb.
c=setup('fallback_clean',[comp(instructions=(extract('example.zip'),),saved=choices(entry('example.zip')))],{'example.zip':FIRST})
rv=validate(c,'report');check('clean_report_quiet',(rv['exit'],mapper_lines(rv['stderr'])),(0,[]))
rv=validate(c,'plain',report=False);check('clean_plain_quiet',(rv['exit'],mapper_lines(rv['stderr'])),(0,[]))
ri=install(c,'install');check('clean_install_quiet',(ri['exit'],mapper_lines(ri['stderr'])),(0,[]))
r=execute(c,'inspect',[str(BIN),'fomod-inspect',str(c['mods']/'example.zip'),'--scratch-dir',str(c['scratch'])]);check('clean_inspect_quiet',(r['exit'],mapper_lines(r['stderr'])),(0,[]))

# 5. The gate inspects each archive once, and spools only archives whose verified
# listing names fomod/ModuleConfig.xml. Unpacks are counted as archive entries created
# in --scratch-dir (inotify IN_CREATE on modsync-entry-* temporaries).
LIBC=ctypes.CDLL(None,use_errno=True)
def spooled(c,label,args):
 fd=LIBC.inotify_init1(os.O_NONBLOCK);assert fd>=0
 assert LIBC.inotify_add_watch(fd,str(c['scratch']).encode(),0x100)>=0
 r=execute(c,label,args);names=[]
 while True:
  try:buf=os.read(fd,65536)
  except BlockingIOError:break
  i=0
  while i<len(buf):
   n=int.from_bytes(buf[i+12:i+16],'little');names.append(buf[i+16:i+16+n].rstrip(b'\0').decode());i+=16+n
 os.close(fd);return r,sum(n.startswith('modsync-entry-') for n in names)
def plain_validate(c):
 game=c['case']/'unused-game';(game/'Override').mkdir(parents=True,exist_ok=True)
 return [str(BIN),'validate','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(c['mods']),'--scratch-dir',str(c['scratch'])]
def shared(name,data,saved=None,status=None,extract_names=()):
 # Both components register the archive and copy a note; only the first extracts.
 second={**NOTE,'Source':['<<modDirectory>>/other.txt']}
 return setup(name,[comp(i,instructions=(NOTE,*map(extract,extract_names)) if i==0 else (second,),saved=saved,files=('example.zip',),status=status) for i in (0,1)],{'example.zip':data,'notes.txt':b'notes bytes\n','other.txt':b'other bytes\n'})
# Non-FOMOD archive registered by two components: listed, never spooled (spooled once
# per registrant at 37cd4cc: 2).
c=shared('unpack_plain_shared',PLAIN);r,n=spooled(c,'validate',plain_validate(c))
check('unpack_plain_shared_exit',r['exit'],0);check('unpack_plain_shared_spooled',n,0)
# FOMOD archive (3 entries) registered by two components: spooled once (twice at 37cd4cc: 6).
c=shared('unpack_fomod_shared',FIRST,choices(entry('example.zip')),{'example.zip':'configured'});r,n=spooled(c,'validate',plain_validate(c))
check('unpack_fomod_shared_exit',r['exit'],0);check('unpack_fomod_shared_spooled',n,3)
# Registered by both, extracted by the first: the gate spools it once (3) and dry-run
# simulation spools the selected Extract once (3). At 37cd4cc: 3 + 3 + 3.
c=shared('unpack_fomod_extract',FIRST,choices(entry('example.zip')),{'example.zip':'configured'},('example.zip',));r,n=spooled(c,'validate',plain_validate(c))
check('unpack_fomod_extract_exit',r['exit'],0);check('unpack_fomod_extract_spooled',n,6)
# Non-FOMOD archive registered by both, extracted by the first: only the simulation
# spools it (1). At 37cd4cc the gate spooled it three times (both registrants and the
# unchecked Extract): 3 + 1.
c=shared('unpack_plain_extract',PLAIN,extract_names=('example.zip',));r,n=spooled(c,'validate',plain_validate(c))
check('unpack_plain_extract_exit',r['exit'],0);check('unpack_plain_extract_spooled',n,1)

srv.shutdown();srv.server_close()
check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),routes=subprocess.check_output(['ip','route'],text=True),certificateCommand=certargs,leafCertificateCommands=leafargs,requests=REQUESTS,checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FOMOD_S2B_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
