#!/usr/bin/env python3
"""FOMOD residuals production receipts: gate hard-dependency scope, the missing
registered-archive soft warning, archive-suffixed folder sources and per-archive
run-choice preservation. Real validate/install (plain and --validation-report) and
--help in an isolated network namespace with no routes. Generic fixtures only; no
Rust harness."""
import atexit, hashlib, io, json, os, pathlib, shutil, subprocess, tempfile, zipfile
REPO=pathlib.Path(__file__).resolve().parents[3]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/fomod-gate-scope'))).resolve()
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
ROOT=pathlib.Path(tempfile.mkdtemp(prefix='fomod-gate-scope-'))
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

# 1. Gate scope (C# FomodConfigurationGate.Validate -> ExpandWithHardDependencies):
# the selected seeds plus their hard dependencies, transitively, selected or not. An
# unselected dependency's registered archives are gated, and its FOMOD finding is
# reported instead of being masked by Rust's stricter "requires selected component"
# error. The CLI selects every component by default, so these cases keep the guide's
# own selection (--use-file-selection), as a profile or saved selection would.
FILESEL=['--use-file-selection']
dep=[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',))]
c=setup('dep_unconfigured',dep,{'example.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);s=stage(rv)
check('dep_report_exit',rv['exit'],1);check('dep_report_fomod_failed',s['passed'],False)
check('dep_report_fomod_names_archive',any("FOMOD archive 'example.zip'" in m and NO_CHOICES in m for m in s['messages']))
check('dep_report_fomod_not_order_error',any(ORDER_ERROR in m for m in s['messages']),False)
rv=validate(c,'plain',report=False,flags=FILESEL);check('dep_plain_exit',rv['exit'],1);check('dep_plain_gate_names_archive',NO_CHOICES in rv['stderr'] and 'FOMOD configuration check failed' in rv['stderr'])
ri=install(c,'install',FILESEL);check('dep_install_exit',ri['exit'],1);check('dep_install_gate_names_archive',NO_CHOICES in ri['stderr']);check('dep_install_game_untouched',ri['game'],{});check('dep_install_mods_unchanged',ri['mods_unchanged'])
# Transitive: consumer -> dependency -> deep dependency (both unselected); the deep one registers it.
deep=[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,Dependencies=[GUIDS[2]]),comp(2,selected=False,files=('deep.zip',))]
c=setup('dep_transitive',deep,{'deep.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);s=stage(rv);check('dep_transitive_fomod_failed',s['passed'],False)
check('dep_transitive_names_archive',[("FOMOD archive 'deep.zip'" in m and NO_CHOICES in m) for m in s['messages']],[True])
# A stale saved choice on the unselected dependency is found the same way.
c=setup('dep_stale',[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',),saved=choices(entry('example.zip','Unknown')),status={'example.zip':'configured'})],{'example.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);check('dep_stale_names_archive',[('do not apply to the current archive' in m) for m in stage(rv)['messages']],[True])
# Controls, identical at both builds: once the dependency's FOMOD archive is configured
# (or is not a FOMOD), Rust's stricter selection rule still fails the gate; an unknown
# dependency GUID and a selected restriction conflict fail it as before.
okdep=[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',),saved=choices(entry('example.zip')),status={'example.zip':'configured'})]
c=setup('dep_configured',okdep,{'example.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);s=stage(rv);check('dep_configured_order_error',(rv['exit'],s['passed'],[ORDER_ERROR in m for m in s['messages']]),(1,False,[True]))
c=setup('dep_plain_archive',[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',))],{'example.zip':PLAIN})
rv=validate(c,'report',flags=FILESEL);s=stage(rv);check('dep_plain_archive_order_error',(s['passed'],[ORDER_ERROR in m for m in s['messages']]),(False,[True]))
c=setup('dep_unknown',[comp(0,Dependencies=[UNKNOWN],files=('example.zip',),saved=choices(entry('example.zip')),status={'example.zip':'configured'},instructions=(extract('example.zip'),))],{'example.zip':FIRST})
rv=validate(c,'report');s=stage(rv);check('dep_unknown_fails',(rv['exit'],s['passed'],[UNKNOWN in m for m in s['messages']]),(1,False,[True]))
c=setup('restriction',[comp(0,Restrictions=[GUIDS[3]],instructions=(extract('example.zip'),),saved=choices(entry('example.zip'))),comp(3)],{'example.zip':FIRST})
rv=validate(c,'report');s=stage(rv);check('restriction_fails',(rv['exit'],s['passed'],['conflicts with' in m for m in s['messages']]),(1,False,[True]))
# A selected dependency is already in scope (default selection selects it).
c=setup('dep_selected',[comp(0,Dependencies=[GUIDS[1]]),comp(1,selected=False,files=('example.zip',))],{'example.zip':FIRST})
rv=validate(c,'report');s=stage(rv);check('dep_selected_fomod_names_archive',(s['passed'],[NO_CHOICES in m for m in s['messages']]),(False,[True]))
good=[comp(0,Dependencies=[GUIDS[1]]),comp(1,files=('example.zip',),saved=choices(entry('example.zip')),status={'example.zip':'configured'},instructions=(extract('example.zip'),))]
c=setup('dep_selected_ok',good,{'example.zip':FIRST})
rv=validate(c,'report',flags=FILESEL);check('dep_selected_ok_exit',rv['exit'],0)
ri=install(c,'install',FILESEL);check('dep_selected_ok_install',(ri['exit'],ri['game']),(0,files('b','notes')))

# 2. Missing registered archive without prompt state: C#'s soft warning text, one per
# registering component ("WARNING: <component>: ..."). With state it stays a hard error.
def missing(name):return f"WARNING: {name}: Registered archive 'gone.zip' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install."
c=setup('missing_one',[comp(0,files=('gone.zip',))],{})
rv=validate(c,'report');s=stage(rv)
check('missing_report_exit',rv['exit'],0)
check('missing_report_stage',(s['passed'],s['hasWarnings'],s['summary'],s['messages']),(True,True,'All detected FOMOD archives are configured (1 warning(s)).',[missing('Example Consumer')]))
rv=validate(c,'plain',report=False);check('missing_plain_exit',rv['exit'],0);check('missing_plain_event',sum(l==' / 0: '+missing('Example Consumer') for l in rv['stdout'].splitlines()),1)
ri=install(c,'install');check('missing_install',(ri['exit'],ri['game']),(0,files('notes')));check('missing_install_event',missing('Example Consumer') in ri['stdout'])
# Two components registering the same missing archive: one warning each, as in C#.
c=setup('missing_two',[comp(0,files=('gone.zip',)),comp(3,files=('gone.zip',))],{})
rv=validate(c,'report');s=stage(rv);check('missing_two_messages',s['messages'],[missing('Example Consumer'),missing('Example Other')])
# Controls: recorded prompt state fails closed; a missing non-archive name is not gated.
c=setup('missing_state',[comp(0,files=('gone.zip',),status={'gone.zip':'dismissed'})],{})
rv=validate(c,'plain',report=False);check('missing_state_plain',(rv['exit'],'missing from the mod directory' in rv['stderr']),(1,True))
c=setup('missing_non_archive',[comp(0,files=('readme.txt',))],{})
rv=validate(c,'report');check('missing_non_archive_stage',(stage(rv)['passed'],stage(rv)['messages']),(True,[]))

# 3. Folder sources named like archives (rooted run-choice ownership): a directory
# `Pack.zip` is a folder; it owns only the archives it really contains.
pack=[comp(0,instructions=(extract('Pack.zip'),))]
c=setup('folder_suffix',pack,{'Pack.zip/inner.zip':FIRST})
ri=install(c,'no_file');check('folder_suffix_no_file',(ri['exit'],NO_CHOICES in ri['stderr'],ri['game']),(1,True,{}))
ri=install(c,'file',sidecar(c,choices(entry('inner.zip'))));check('folder_suffix_install',(ri['exit'],ri['game']),(0,files('b')))
rv=validate(c,'report',flags=sidecar(c,choices(entry('inner.zip'))));check('folder_suffix_report',(rv['exit'],stage(rv)['passed']),(0,True))
rv=validate(c,'plain',report=False,flags=sidecar(c,choices(entry('inner.zip'))));check('folder_suffix_plain',rv['exit'],0)
# An entry named like the folder itself configures nothing inside it.
ri=install(c,'folder_name_entry',sidecar(c,choices(entry('Pack.zip'))));check('folder_suffix_folder_name_entry',(ri['exit'],NO_CHOICES in ri['stderr'],ri['game']),(1,True,{}))
# Ownership: an unrelated archive-suffixed folder holding a different archive does not
# take the entry, so its unparseable FomodChoices do not fail the run.
UNPARSEABLE={'version':2,'archives':[entry('other.zip','Alpha')]}
own=[comp(0,instructions=(extract('Pack.zip'),)),comp(3,instructions=(NOTE,extract('Other.zip')),saved=UNPARSEABLE)]
c=setup('folder_ownership',own,{'Pack.zip/inner.zip':FIRST,'Other.zip/plain.zip':PLAIN})
ri=install(c,'install',sidecar(c,choices(entry('inner.zip'))));check('folder_ownership_install',(ri['exit'],ri['game']),(0,files('b','notes')))
# Control: the same folder does hold inner.zip, so its unparseable choices fail with the repair hint.
own_bad=[comp(0,instructions=(extract('Pack.zip'),),saved=UNPARSEABLE)]
c=setup('folder_ownership_bad',own_bad,{'Pack.zip/inner.zip':FIRST})
ri=install(c,'install',sidecar(c,choices(entry('inner.zip'))));check('folder_ownership_bad',(ri['exit'],"Repair this component's FomodChoices" in ri['stderr'],ri['game']),(1,True,{}))

# 4. Per-archive preservation of embedded choices with run choices (S2b merge) through a
# folder source: the entry replaces inner.zip's choice; second.zip keeps its own.
EMBEDDED=choices(entry('inner.zip','Alpha'),entry('second.zip','Alpha'))
c=setup('preserve',[comp(0,instructions=(extract('Pack.zip'),extract('second.zip')),saved=EMBEDDED)],{'Pack.zip/inner.zip':FIRST,'second.zip':SECOND})
g0=digest(c['guide'])
ri=install(c,'embedded');check('preserve_embedded',(ri['exit'],ri['game']),(0,files('a','c')))
ri=install(c,'one_entry',sidecar(c,choices(entry('INNER.ZIP'))));check('preserve_one_entry',(ri['exit'],ri['game']),(0,files('b','c')))
ri=install(c,'other_entry',sidecar(c,choices(entry('second.zip'))));check('preserve_other_entry',(ri['exit'],ri['game']),(0,files('a','d')))
rv=validate(c,'report',flags=sidecar(c,choices(entry('inner.zip'))));check('preserve_report',(rv['exit'],stage(rv)['passed']),(0,True))
check('preserve_guide_never_written',digest(c['guide']),g0)

# 5. The run-choices help no longer implies whole-file replacement.
for verb in ('install','validate'):
 r=subprocess.run([str(BIN),verb,'--help'],capture_output=True,text=True,env=ENV)
 text=' '.join(r.stdout.split())
 check(f'help_{verb}_per_archive',(r.returncode,'merged per archive' in text and 'other archives keep theirs' in text),(0,True))

check('binary_unchanged',digest(BIN),BINARY_BEFORE)
check('source_unchanged',sources(),SOURCE_BEFORE)
binding=dict(binary=str(BIN),binarySha256=BINARY_BEFORE,binaryMtime=BIN.stat().st_mtime,newestSourceMtime=NEWEST,binaryNewerThanSources=BIN.stat().st_mtime>=NEWEST,gitHead=git('rev-parse','HEAD'),gitPorcelain=git('status','--porcelain'),sourceSha256=SOURCE_BEFORE)
receipt=dict(binding=binding,root=str(ROOT),networkNamespace=os.readlink('/proc/self/ns/net'),checks=CHECKS,passed=sum(c['passed'] for c in CHECKS),total=len(CHECKS),invocations=len(RUNS))
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:receipt[k] for k in ['root','passed','total','invocations']}|{'binaryNewerThanSources':binding['binaryNewerThanSources'],'gitHead':binding['gitHead']},indent=2))
if os.environ.get('FOMOD_GATE_SCOPE_ALLOW_UNBOUND')!='1':assert binding['binaryNewerThanSources'],'binary is older than its sources; rebuild before collecting receipts'
assert all(c['passed'] for c in CHECKS)
