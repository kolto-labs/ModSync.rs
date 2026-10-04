#!/usr/bin/env python3
"""FOMOD gate receipts: soft gate warnings survive a later hard failure of the
same gate (C# InstallationValidationPipeline reports gate warnings alongside its
findings). Real validate (plain and --validation-report) and install in an isolated
network namespace with no routes. Generic fixtures only; no Rust harness."""
import atexit, hashlib, io, json, os, pathlib, shutil, subprocess, tempfile, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fomod-gate-warnings'))).resolve()
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
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-gate-warnings-'))
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

GUIDS=['aaaaaaaa-1111-4111-8111-111111111111','bbbbbbbb-2222-4222-8222-222222222222','cccccccc-3333-4333-8333-333333333333','dddddddd-4444-4444-8444-444444444444']
UNKNOWN='eeeeeeee-5555-4555-8555-555555555555'
NAMES=['Example Consumer','Example Dependency','Example Deep Dependency','Example Other']
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
def bare(name,flags=''):
 # A plugin that installs no files (optionally setting condition flags).
 return f'<plugin name="{name}"><description>Generic option</description>{flags}<typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
def group(plugins):
 return '<config><moduleName>Example Installer</moduleName><installSteps><installStep name="Choose"><optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'+''.join(plugins)+'</plugins></group></optionalFileGroups></installStep></installSteps>'
# No required files, no plugin files, no conditional patterns: the C# mapper gives no
# archive-scoped instruction for any choice.
EMPTY=module_zip(group([bare('Alpha'),bare('Beta')])+'</config>')
# Only steps-free metadata: no steps, no files at all.
NOSTEPS=module_zip('<config><moduleName>Example Installer</moduleName></config>')
# The only file sits in a conditional pattern no single plugin can satisfy: C# skips it.
UNATTACHED=module_zip(group([bare('Alpha'),bare('Beta')])+'<conditionalFileInstalls><patterns><pattern><dependencies operator="And"><flagDependency flag="extra" value="On"/></dependencies><files><file source="payload/a.txt" destination="Override"/></files></pattern></patterns></conditionalFileInstalls></config>')
# Control: the same conditional pattern attached to Alpha (which sets the flag) is mapped.
ATTACHED=module_zip(group([bare('Alpha','<conditionFlags><flag name="extra">On</flag></conditionFlags>'),bare('Beta')])+'<conditionalFileInstalls><patterns><pattern><dependencies operator="And"><flagDependency flag="extra" value="On"/></dependencies><files><file source="payload/a.txt" destination="Override"/></files></pattern></patterns></conditionalFileInstalls></config>')
# Control: an unchosen plugin (Alpha) has a file, the chosen one (Beta) has none. C#
# keeps every plugin's option on the component, so its instructions satisfy the check.
PARTIAL=module_zip(group([plugin('Alpha','payload/a.txt'),bare('Beta')])+'</config>')

def r1(name,archive='empty.zip'):return f"WARNING: {name}: FOMOD archive '{archive}' is marked configured but has no matching archive-scoped instructions. Its installer maps no files for any choice, so this configuration installs nothing from the archive."
R1="has no matching archive-scoped instructions"
OK='All detected FOMOD archives are configured.'
def warned(n):return f'All detected FOMOD archives are configured ({n} warning(s)).'
def configured(i,archive='empty.zip',plugin='Beta',instructions=None,status='configured',saved=True):
 return comp(i,instructions=(extract(archive),) if instructions is None else instructions,files=(archive,),saved=choices(entry(archive,plugin)) if saved else None,status={archive:status} if status else None)

# C# InstallationValidationPipeline.RunFomodConfigurationStage prints every gate warning,
# then the issues, in one failed stage. Rust's gate adds stricter readiness checks after
# its FOMOD findings (unselected dependency, restriction conflict) and stops at its first
# issue. Soft warnings collected before such a failure must survive on every surface:
# the --validation-report stage, plain validate events and install events.
FILESEL=['--use-file-selection']
GATE='FOMOD configuration check failed'
BLOCKED='Installation blocked before any write'
def missing(name,archive='gone.zip'):return f"WARNING: {name}: Registered archive '{archive}' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install."
def route(archive,*names):return f"WARNING: FOMOD archive '{archive}' has valid choices, but no selected Extract instruction installs it; configuration alone installs no files. Registered by "+', '.join(f"'{n}'" for n in names)+'.'
ORDER=f'Example Consumer requires selected component {GUIDS[1]}'
def event(w):return ' / 0: '+w
def failed(label,rv,warnings,error):
 s=stage(rv);d=rv['data'] or {}
 check(label+'_report_exit',rv['exit'],1)
 check(label+'_report_stage',(s['passed'],s['hasWarnings'],s['summary'],s['messages']),(False,True,'Failed',[*warnings,error]))
 check(label+'_report_counts',(d.get('success'),d.get('errorCount'),d.get('warningCount')),(False,1,len(warnings)))
def plain(label,rv,warnings,error):
 lines=rv['stdout'].splitlines()
 check(label+'_plain_exit',rv['exit'],1)
 check(label+'_plain_events',[sum(l==event(w) for l in lines) for w in warnings],[1]*len(warnings))
 check(label+'_plain_error',(GATE in rv['stderr'],error in rv['stderr']),(True,True))
def installed(label,ri,warnings,error):
 check(label+'_install',(ri['exit'],ri['game'],ri['mods_unchanged']),(1,{},True))
 check(label+'_install_events',[w in ri['stdout'] for w in warnings],[True]*len(warnings))
 check(label+'_install_error',(BLOCKED in ri['stderr'],GATE in ri['stderr'],error in ri['stderr']),(True,True,True))

# 1. The Rust-only no-route warning for an unselected, configured hard dependency, then
# Rust's stricter "requires selected component" error.
c=setup('route_then_order',[comp(0,Dependencies=[GUIDS[1]]),configured(1,instructions=(NOTE,),archive='example.zip',plugin='Beta')|{'IsSelected':False}],{'example.zip':FIRST})
w=[route('example.zip','Example Dependency')]
failed('route_then_order',validate(c,'report',flags=FILESEL),w,ORDER)
plain('route_then_order',validate(c,'plain',report=False,flags=FILESEL),w,ORDER)
installed('route_then_order',install(c,'install',FILESEL),w,ORDER)

# 2. C#'s missing-registered-archive warning on the consumer, then the same order error.
c=setup('missing_then_order',[comp(0,Dependencies=[GUIDS[1]],files=('gone.zip',)),comp(1,selected=False)],{})
w=[missing('Example Consumer')]
failed('missing_then_order',validate(c,'report',flags=FILESEL),w,ORDER)
plain('missing_then_order',validate(c,'plain',report=False,flags=FILESEL),w,ORDER)
installed('missing_then_order',install(c,'install',FILESEL),w,ORDER)

# 3. C#'s "configured without archive-scoped instructions" warning, then the order error.
c=setup('r1_then_order',[configured(0)|{'Dependencies':[GUIDS[1]]},comp(1,selected=False)],{'empty.zip':EMPTY})
w=[r1('Example Consumer')]
failed('r1_then_order',validate(c,'report',flags=FILESEL),w,ORDER)
plain('r1_then_order',validate(c,'plain',report=False,flags=FILESEL),w,ORDER)
installed('r1_then_order',install(c,'install',FILESEL),w,ORDER)

# 4. Two warnings (missing and R1), then a restriction conflict with a selected component.
CONFLICT=f'Example Consumer conflicts with {GUIDS[3]}'
c=setup('two_then_restriction',[configured(0)|{'Restrictions':[GUIDS[3]],'ResourceRegistry':registry(['empty.zip','gone.zip'],{'empty.zip':'configured'})},comp(3)],{'empty.zip':EMPTY})
# Registry order: the registered empty.zip comes before gone.zip.
w=[r1('Example Consumer'),missing('Example Consumer')]
rv=validate(c,'report');s=stage(rv)
check('two_then_restriction_report',(rv['exit'],s['passed'],s['hasWarnings'],s['messages'][:2],len(s['messages']),CONFLICT in s['messages'][-1]),(1,False,True,w,3,True))
check('two_then_restriction_counts',((rv['data'] or {}).get('errorCount'),(rv['data'] or {}).get('warningCount')),(1,2))
rv=validate(c,'plain',report=False);check('two_then_restriction_plain',(rv['exit'],[sum(l==event(x) for l in rv['stdout'].splitlines()) for x in w],CONFLICT in rv['stderr']),(1,[1,1],True))

# 5. A warning collected before an in-gate FOMOD issue (configured status without saved
# choices on a later component) survives that issue too.
c=setup('missing_then_issue',[comp(0,files=('gone.zip',)),configured(3,saved=False)],{'empty.zip':EMPTY})
rv=validate(c,'report');s=stage(rv)
check('missing_then_issue_report',(rv['exit'],s['passed'],s['hasWarnings'],s['messages'][:1],len(s['messages']),NO_CHOICES in s['messages'][-1]),(1,False,True,[missing('Example Consumer')],2,True))
rv=validate(c,'plain',report=False);check('missing_then_issue_plain',(rv['exit'],sum(l==event(missing('Example Consumer')) for l in rv['stdout'].splitlines()),NO_CHOICES in rv['stderr']),(1,1,True))

# 6. Controls, identical at both builds: the same failures without a warning keep exactly
# their error; a warning-only gate still passes; the error text and exit are unchanged.
c=setup('order_only',[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',))],{'example.zip':PLAIN})
rv=validate(c,'report',flags=FILESEL);s=stage(rv)
check('order_only_report',(rv['exit'],s['passed'],s['hasWarnings'],s['messages'],(rv['data'] or {}).get('warningCount')),(1,False,False,[ORDER],0))
rv=validate(c,'plain',report=False,flags=FILESEL);check('order_only_plain',(rv['exit'],'WARNING' in rv['stdout'],ORDER in rv['stderr']),(1,False,True))
c=setup('warning_only',[comp(0,files=('gone.zip',))],{})
rv=validate(c,'report');s=stage(rv)
check('warning_only_report',(rv['exit'],s['passed'],s['hasWarnings'],s['summary'],s['messages']),(0,True,True,'All detected FOMOD archives are configured (1 warning(s)).',[missing('Example Consumer')]))
c=setup('issue_only',[configured(0,saved=False)],{'empty.zip':EMPTY})
rv=validate(c,'report');s=stage(rv);check('issue_only_report',(rv['exit'],s['passed'],s['hasWarnings'],len(s['messages']),NO_CHOICES in s['messages'][0]),(1,False,False,1,True))

check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FOMOD_GATE_WARNINGS_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
