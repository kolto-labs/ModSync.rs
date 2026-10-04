#!/usr/bin/env python3
"""FOMOD gate receipts: the gate collects every in-gate FOMOD issue and every
warning instead of stopping at its first issue (C# FomodConfigurationGate.Validate
walks the whole scope). Real validate (plain and --validation-report) and install in
an isolated network namespace with no routes. Generic fixtures only; no Rust harness."""
import atexit, hashlib, io, json, os, pathlib, shutil, subprocess, tempfile, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fomod-gate-collect'))).resolve()
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
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-gate-collect-'))
if os.environ.get('MODSYNC_PROOF_KEEP')!='1':atexit.register(shutil.rmtree,ROOT,ignore_errors=True)
ENV={k:v for k,v in os.environ.items() if not k.lower().endswith('_proxy') and not any(t in k for t in ['NEXUS','MODSYNC_FOMOD'])}
for k in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
 ENV[k]=str(ROOT/k);pathlib.Path(ENV[k]).mkdir()
CHECKS=[];RUNS=[]
def snapshot(p):return {str(f.relative_to(p)):digest(f) for f in sorted(p.rglob('*')) if f.is_file()}
def check(name,actual,expected=True):
 row=dict(name=name,actual=actual,expected=expected,passed=actual==expected);CHECKS.append(row)
 if not row['passed']:print('FAIL',name,repr(actual)[:400],flush=True)

# Generic installer: one exclusive group, plugin Alpha installs a.txt and Beta b.txt.
def plugin(name,source):
 return f'<plugin name="{name}"><description>Generic option</description><files><file source="{source}" destination="Override"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
def pair(srcs=('a','b')):
 xml='<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'+''.join(plugin(n,f'payload/{s}.txt') for n,s in zip(('Alpha','Beta'),srcs))+'</plugins></group></optionalFileGroups></installStep></installSteps></config>'
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml),*[(f'payload/{s}.txt',f'{s} bytes\n') for s in srcs]]:
   z.writestr(zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0)),value)
 return b.getvalue()
FIRST=pair();SECOND=pair(('c','d'))
def plain_zip():
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:z.writestr(zipfile.ZipInfo('payload/plain.txt',date_time=(2020,1,1,0,0,0)),'plain\n')
 return b.getvalue()
PLAIN=plain_zip()

GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222','cccccccc-3333-4333-8333-333333333333','dddddddd-4444-4444-8444-444444444444','ffffffff-6666-4666-8666-666666666666','abababab-7777-4777-8777-777777777777']
UNKNOWN='eeeeeeee-5555-4555-8555-555555555555'
NAMES=['Example Consumer','Example Dependency','Example Deep Dependency','Example Other','Example Fifth','Example Sixth']
def extract(name):return {'Action':'Extract','Source':[f'<<modDirectory>>/{name}']}
NOTE={'Action':'Copy','Source':['<<modDirectory>>/notes.txt'],'Destination':'<<kotorDirectory>>/Override'}
def sel(plugin):return {'stepName':'Choose','groupName':'Variant','plugins':[plugin]}
def choices(*archives):return {'version':1,'archives':list(archives)}
def entry(filename,plugin='Beta'):return {'archiveFileName':filename,'selections':[sel(plugin)]}
def registry(files,status=None):
 meta={'Files':{f:True for f in files}}
 if status:meta['HandlerMetadata']={'fomodPromptStatus':dict(status)}
 return {'https://example.invalid/'+files[0]:meta}
def comp(i,selected=True,instructions=(NOTE,),saved=None,files=(),status=None,**extra):
 c={'Guid':GUIDS[i],'Name':NAMES[i],'IsSelected':selected,'Instructions':list(instructions),**extra}
 if files:c['ResourceRegistry']=registry(list(files),status)
 if saved is not None:c['FomodChoices']=saved
 return c
def setup(name,components,existing,dirs=()):
 case=ROOT/name;case.mkdir();mods=case/'mods';mods.mkdir();scratch=case/'scratch';scratch.mkdir()
 guide=case/'guide.json';guide.write_text(json.dumps({'Components':components},ensure_ascii=False),encoding='utf-8')
 for d in dirs:(mods/d).mkdir(parents=True,exist_ok=True)
 for filename,data in {'notes.txt':b'notes bytes\n',**existing}.items():
  (mods/filename).parent.mkdir(parents=True,exist_ok=True);(mods/filename).write_bytes(data)
 return dict(name=name,case=case,guide=guide,mods=mods,scratch=scratch,runs=0)
def execute(c,label,args):
 c['runs']+=1;name=f"{c['name']}_{c['runs']:02d}_{label}";before=snapshot(c['case'])
 r=subprocess.run(args,capture_output=True,text=True,env=ENV,timeout=120)
 try:data=json.loads(r.stdout)
 except Exception:data=None
 row=dict(name=name,argv=args,exit=r.returncode,stdout=r.stdout,stderr=r.stderr,data=data,before=before,after=snapshot(c['case']))
 (OUT/(name+'.json')).write_text(json.dumps(row,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');RUNS.append(row);return row
def sidecar(c,value):
 path=c['case']/f"choices-{c['runs']+1:02d}.json";path.write_text(json.dumps(value,ensure_ascii=False),encoding='utf-8');return ['--fomod-choices',str(path)]
def kept(t):return {k:v for k,v in t.items() if not k.startswith('scratch/')}
def validate(c,label,report=True,flags=()):
 game=c['case']/'unused-game';(game/'Override').mkdir(parents=True,exist_ok=True)
 r=execute(c,label,[str(BIN),'validate','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(c['mods']),'--scratch-dir',str(c['scratch']),*(['--validation-report'] if report else []),*flags])
 # Validation is pure: guide, mod and game trees are byte-identical (scratch aside).
 check(r['name']+'_trees_unchanged',kept(r['after']),kept(r['before']))
 return r
def stage(r,name='FomodConfiguration'):return {'messages':[],'hasWarnings':False,'passed':None,'summary':None}|next((s for s in (r['data'] or {}).get('stages',[]) if s['stage']==name),{})
def install(c,label,flags=()):
 k=c['runs']+1;mods=c['case']/f'install-{k:02d}-mods';shutil.copytree(c['mods'],mods);game=c['case']/f'install-{k:02d}-game';game.mkdir()
 mods_before=snapshot(mods)
 r=execute(c,label,[str(BIN),'install','--input',str(c['guide']),'--game-dir',str(game),'--source-dir',str(mods),'--scratch-dir',str(c['scratch']),*flags])
 r['game']={str(p.relative_to(game)):p.read_text() for p in sorted(game.rglob('*')) if p.is_file() and '.modsync' not in p.parts}
 r['mods_unchanged']=snapshot(mods)==mods_before
 check(r['name']+'_guide_unchanged',r['after'].get('guide.json'),r['before'].get('guide.json'))
 return r
def files(*names):return {f'Override/{n}.txt':f'{n} bytes\n' for n in names}
NO_CHOICES="has no explicit FomodChoices configuration"
ORDER_ERROR="requires selected component"
ROUTE="no selected Extract instruction installs it; configuration alone installs no files."

def module_zip(xml,payload=('a','b')):
 b=io.BytesIO()
 with zipfile.ZipFile(b,'w') as z:
  for name,value in [('fomod/ModuleConfig.xml',xml),*[(f'payload/{s}.txt',f'{s} bytes\n') for s in payload]]:
   z.writestr(zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0)),value)
 return b.getvalue()
def bare(name):return f'<plugin name="{name}"><description>Generic option</description><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
# An installer that maps no files for any choice: C#'s R1 "configured but has no
# matching archive-scoped instructions" warning.
EMPTY=module_zip('<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'+bare('Alpha')+bare('Beta')+'</plugins></group></optionalFileGroups></installStep></installSteps></config>')
BROKEN=b'this is not a zip archive\n'*4
def configured(i,archive,plugin='Beta',saved=True,status='configured'):
 return comp(i,instructions=(extract(archive),),files=(archive,),saved=choices(entry(archive,plugin)) if saved else None,status={archive:status} if status else None)
def missing(name,archive):return f"WARNING: {name}: Registered archive '{archive}' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install."
def r1(name,archive):return f"WARNING: {name}: FOMOD archive '{archive}' is marked configured but has no matching archive-scoped instructions. Its installer maps no files for any choice, so this configuration installs nothing from the archive."
# Distinctive fragments of each Rust issue type (texts unchanged by this slice).
def no_choices(archive):return f"FOMOD archive '{archive}' is marked configured in handlerMetadata.fomodPromptStatus but has no explicit FomodChoices configuration."
def lost(archive):return f"Registered FOMOD archive '{archive}' is missing from the mod directory after configuration state was recorded"
def stale(archive):return f"Saved FOMOD choices for '{archive}' do not apply to the current archive"
def unreadable(archive):return f"Registered archive '{archive}' cannot be inspected"
GATE='FOMOD configuration check failed'
BLOCKED='Installation blocked before any write'
FILESEL=['--use-file-selection']
def event(w):return ' / 0: '+w
def errors(s):return [m for m in s['messages'] if not m.startswith('WARNING:')]
def matches(messages,fragments):return len(messages)==len(fragments) and all(f in m for m,f in zip(messages,fragments))
def numbered(text,fragments):
 # "<GATE>: <n> issues: [1] <first>; [2] <second>; ..." in gate order, each fragment in its slot.
 head=f'{GATE}: {len(fragments)} issues: '
 if head not in text:return False
 rest=text[text.index(head)+len(head):]
 marks=[rest.find(f'[{i+1}] ') for i in range(len(fragments))]
 if any(m<0 for m in marks) or marks!=sorted(marks) or marks[0]!=0:return False
 parts=[rest[a:b] for a,b in zip(marks,marks[1:]+[len(rest)])]
 return all(f in p for p,f in zip(parts,fragments))
def report(label,rv,warnings,issues):
 s=stage(rv);d=rv['data'] or {}
 check(label+'_report_exit',rv['exit'],1)
 check(label+'_report_stage',(s['passed'],s['hasWarnings'] if warnings else False,s['summary']),(False,bool(warnings),'Failed'))
 # All warnings first, in gate order, then every issue in gate order.
 check(label+'_report_warnings',s['messages'][:len(warnings)],warnings)
 check(label+'_report_errors',matches(s['messages'][len(warnings):],issues),True)
 check(label+'_report_counts',(d.get('success'),d.get('errorCount'),d.get('warningCount')),(False,len(issues),len(warnings)))
def plain(label,rv,warnings,issues):
 lines=rv['stdout'].splitlines()
 check(label+'_plain_exit',rv['exit'],1)
 check(label+'_plain_events',[sum(l==event(w) for l in lines) for w in warnings],[1]*len(warnings))
 check(label+'_plain_error',numbered(rv['stderr'],issues),True)
def installed(label,ri,warnings,issues):
 check(label+'_install',(ri['exit'],ri['game'],ri['mods_unchanged']),(1,{},True))
 check(label+'_install_events',[event(w) in ri['stdout'].splitlines() for w in warnings],[True]*len(warnings))
 check(label+'_install_error',(BLOCKED in ri['stderr'],numbered(ri['stderr'],issues)),(True,True))
def surfaces(label,c,warnings,issues,flags=()):
 report(label,validate(c,'report',flags=flags),warnings,issues)
 plain(label,validate(c,'plain',report=False,flags=flags),warnings,issues)
 installed(label,install(c,'install',flags),warnings,issues)

# 1. Six components with interleaved warnings and issues of every in-gate type except
# unreadable: missing archive (warning), configured without saved choices (issue),
# missing archive with prompt state (issue), R1 (warning), stale saved choices (issue),
# missing archive (warning). At d78aea2 the gate stopped at the first issue.
c=setup('mixed',[comp(0,files=('gone.zip',)),configured(1,'empty.zip',saved=False),comp(2,files=('lost.zip',),status={'lost.zip':'configured'}),configured(3,'other.zip'),configured(4,'pair.zip',plugin='Gamma'),comp(5,files=('gone2.zip',))],{'empty.zip':EMPTY,'other.zip':EMPTY,'pair.zip':FIRST})
surfaces('mixed',c,[missing('Example Consumer','gone.zip'),r1('Example Other','other.zip'),missing('Example Sixth','gone2.zip')],[no_choices('empty.zip'),lost('lost.zip'),stale('pair.zip')])

# 2. Issues first, warnings only on later components: C# still reports both.
c=setup('issues_then_warnings',[configured(0,'empty.zip',saved=False),configured(1,'pair.zip',plugin='Gamma'),configured(2,'other.zip'),comp(3,files=('gone.zip',))],{'empty.zip':EMPTY,'other.zip':EMPTY,'pair.zip':FIRST})
surfaces('issues_then_warnings',c,[r1('Example Deep Dependency','other.zip'),missing('Example Other','gone.zip')],[no_choices('empty.zip'),stale('pair.zip')])

# 3. An unreadable registered archive (C# GateIssue ArchiveUnreadable) is an issue, and
# the gate goes on to the next component's issue.
c=setup('unreadable_then_issue',[comp(0,files=('broken.zip',)),configured(1,'empty.zip',saved=False)],{'broken.zip':BROKEN,'empty.zip':EMPTY})
surfaces('unreadable_then_issue',c,[],[unreadable('broken.zip'),no_choices('empty.zip')])

# 4. FOMOD issues still come before Rust's stricter readiness checks: with two issues and
# an unselected hard dependency, the order error is not reached (as before).
ORDER=f'Example Consumer requires selected component {GUIDS[1]}'
c=setup('issues_before_order',[configured(0,'empty.zip',saved=False)|{'Dependencies':[GUIDS[1]]},comp(1,selected=False),configured(2,'pair.zip',plugin='Gamma')],{'empty.zip':EMPTY,'pair.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);report('issues_before_order',rv,[],[no_choices('empty.zip'),stale('pair.zip')])
check('issues_before_order_no_order_error',any(ORDER in m for m in stage(rv)['messages']),False)
plain('issues_before_order',validate(c,'plain',report=False,flags=FILESEL),[],[no_choices('empty.zip'),stale('pair.zip')])

# 5. Controls, identical at both builds. A single issue keeps exactly its old text (no
# count, no numbering) on every surface; an issue shared by two components through one
# archive is listed once; a warning-only gate passes.
c=setup('single_issue',[comp(0,files=('gone.zip',)),configured(1,'empty.zip',saved=False)],{'empty.zip':EMPTY})
rv=validate(c,'report');s=stage(rv)
check('single_issue_report',(rv['exit'],s['messages'][0],matches(errors(s),[no_choices('empty.zip')]),(rv['data'] or {}).get('errorCount')),(1,missing('Example Consumer','gone.zip'),True,1))
rv=validate(c,'plain',report=False)
check('single_issue_plain',(rv['exit'],f'{GATE}: {no_choices("empty.zip")}' in rv['stderr'],' issues: [1] ' in rv['stderr']),(1,True,False))
ri=install(c,'install')
check('single_issue_install',(ri['exit'],ri['game'],f'{BLOCKED}: {GATE}: {no_choices("empty.zip")}' in ri['stderr'],' issues: [1] ' in ri['stderr']),(1,{},True,False))
shared=registry(['empty.zip'],{'empty.zip':'configured'})
c=setup('shared_archive',[comp(0,instructions=(extract('empty.zip'),),ResourceRegistry=shared),comp(1,instructions=(extract('empty.zip'),),ResourceRegistry=shared)],{'empty.zip':EMPTY})
rv=validate(c,'report');s=stage(rv)
check('shared_archive_report',(rv['exit'],matches(s['messages'],[no_choices('empty.zip')]),(rv['data'] or {}).get('errorCount')),(1,True,1))
rv=validate(c,'plain',report=False);check('shared_archive_plain',(rv['exit'],' issues: [1] ' in rv['stderr'],no_choices('empty.zip') in rv['stderr']),(1,False,True))
c=setup('warnings_only',[comp(0,files=('gone.zip',)),configured(1,'other.zip')],{'other.zip':EMPTY})
rv=validate(c,'report');s=stage(rv)
check('warnings_only_report',(rv['exit'],s['passed'],s['summary'],s['messages'],(rv['data'] or {}).get('warningCount')),(0,True,'All detected FOMOD archives are configured (2 warning(s)).',[missing('Example Consumer','gone.zip'),r1('Example Dependency','other.zip')],2))

check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FOMOD_GATE_COLLECT_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
