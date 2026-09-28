import copy,hashlib,json,os,pathlib,subprocess,tempfile,zipfile
assert os.environ['FOMOD_PARENT_NETNS'] != os.readlink('/proc/self/ns/net')
subprocess.run(['ip','link','set','lo','up'],check=True)
assert not subprocess.check_output(['ip','route'],text=True).strip()
repo=pathlib.Path(__file__).resolve().parents[3]
target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(repo/'target'))).resolve()
binary=str(pathlib.Path(os.environ.get('MODSYNC_BINARY', str(target/'debug/modsync'))).resolve())
binary_before=hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()
out=pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(target/'cloud-proof/inherited'))).resolve();out.mkdir(parents=True,exist_ok=True)
root=pathlib.Path(tempfile.mkdtemp(prefix='fetch-controller-audit-'));mods=root/'mods';mods.mkdir()
env=os.environ.copy()
for key in ['HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME']:
 env[key]=str(root/key);pathlib.Path(env[key]).mkdir()
for key in list(env):
 if key.lower().endswith('_proxy') or 'NEXUS' in key or 'MODSYNC_FOMOD' in key:env.pop(key,None)
base={'Components':[{'Guid':'aaaaaaaa-1111-4111-8111-111111111111','Name':'Example Archive A','IsSelected':False,'Instructions':[],'ResourceRegistry':{'https://example.invalid/example.zip':{'Files':{'example.zip':True}}}},{'Guid':'bbbbbbbb-2222-4222-8222-222222222222','Name':'Example Archive B','IsSelected':True,'Instructions':[],'ResourceRegistry':{'https://example.invalid/second.zip':{'Files':{'second.zip':True}}}}]}
results=[]
def run(name,guide,flags,settings=None,raw=None):
 case=root/name;case.mkdir();inp=case/'guide.json';inp.write_text(json.dumps(guide));sd=case/'settings';sd.mkdir()
 if raw is not None:(sd/'settings.json').write_text(raw)
 elif settings is not None:(sd/'settings.json').write_text(json.dumps(settings))
 before=hashlib.sha256(inp.read_bytes()).hexdigest();beforefiles=sorted(str(p.relative_to(case)) for p in case.rglob('*'))
 args=[binary,'fetch-downloads','--input',str(inp),'--source-dir',str(mods),'--settings-dir',str(sd)]+flags
 r=subprocess.run(args,capture_output=True,text=True,env=env,timeout=30)
 data=None
 try:data=json.loads(r.stdout)
 except:pass
 record={'name':name,'argv':args,'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'data':data,'guideBeforeSha256':before,'guideAfterSha256':hashlib.sha256(inp.read_bytes()).hexdigest(),'newCasePaths':sorted(set(str(p.relative_to(case)) for p in case.rglob('*'))-set(beforefiles))}
 (out/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n');results.append(record);return record
checks=[]
def check(name,actual,expected):checks.append({'name':name,'actual':actual,'expected':expected,'passed':actual==expected})
def targetids(r):return [x['componentGuid'] for x in (r['data'] or {}).get('targets',[])]
a=base['Components'][0]['Guid'];b=base['Components'][1]['Guid']
for name,flags,ids in [('default_all',[],[a,b]),('file_selection',['--use-file-selection'],[b]),('explicit_select',['--select',a],[a]),('deselect',['--deselect',a],[b])]:
 r=run(name,base,['--dry-run','--peer-mode','off']+flags);check(name+'_exit',r['exit'],0);check(name+'_targets',targetids(r),ids);check(name+'_no_guide_write',r['guideBeforeSha256']==r['guideAfterSha256'],True);check(name+'_no_case_write',r['newCasePaths'],[])
for name,settings,mode in [('stored_off',{'peerMode':'off'},'off'),('stored_fallback',{'peerMode':'fallback'},'fallback'),('sharing_false',{'peerMode':'race','peerSharing':False},'off'),('default_race',{},'race')]:
 r=run(name,base,['--dry-run'],settings);check(name+'_exit',r['exit'],0);check(name+'_effective_mode',(r['data'] or {}).get('peerMode'),mode);check(name+'_no_writes',r['newCasePaths'],[])
r=run('sharing_false_override',base,['--dry-run','--peer-mode','race'],{'peerSharing':False});check('sharing_false_override',(r['data'] or {}).get('peerMode'),'off')
r=run('malformed_needed',base,['--dry-run'],raw='{not json');check('malformed_needed_exit',r['exit'],1)
r=run('malformed_off',base,['--dry-run','--peer-mode','off','--nexus-mods-api-key','fixture-key'],raw='{not json');check('malformed_off_exit',r['exit'],0)
r=run('override_no_explicit_key',base,['--dry-run','--peer-mode','off','--nexus-api-base','http://127.0.0.1:19001'],{'nexusModsApiKey':'fixture-stored-key'});check('override_no_explicit_key_exit',r['exit'],1)
with zipfile.ZipFile(mods/'example.zip','w') as z:
 z.writestr('fomod/ModuleConfig.xml',"<config><moduleName>Example Installer</moduleName><requiredInstallFiles><file source='payload.txt' destination='Override/payload.txt'/></requiredInstallFiles></config>")
 z.writestr('payload.txt','fixture payload\n')
sha=hashlib.sha256((mods/'example.zip').read_bytes()).hexdigest()
fomod=copy.deepcopy(base);fomod['Components']=fomod['Components'][:1];fomod['Components'][0]['ResourceRegistry']['https://example.invalid/example.zip']['ContentHashSHA256']=sha
for selected in [False,True]:
 g=copy.deepcopy(fomod);g['Components'][0]['IsSelected']=selected
 r=run('fomod_initially_'+str(selected).lower(),g,['--peer-mode','off','--record']);check('fomod_'+str(selected)+'_exit',r['exit'],3);check('fomod_'+str(selected)+'_followup_count',len((r['data'] or {}).get('fomodFollowups',{}).get('followups',[])),1)
 after=json.loads(pathlib.Path(r['argv'][r['argv'].index('--input')+1]).read_text())
 comps=after.get('Components',after.get('ModSync',[])) if isinstance(after,dict) else after
 check('fomod_'+str(selected)+'_stored_selection',comps[0]['IsSelected'],selected)
g=copy.deepcopy(base);g['Components']=g['Components'][:1]
r=run('all_false_file_selection',g,['--dry-run','--peer-mode','off','--use-file-selection']);check('all_false_file_selection_exit',r['exit'],0);check('all_false_file_selection_targets',targetids(r),[]);check('all_false_file_selection_skipped',(r['data'] or {}).get('skipped'),[])
receipt={'binaryBeforeSha256':binary_before,'root':str(root),'networkNamespace':os.readlink('/proc/self/ns/net'),'ipv4Routes':subprocess.check_output(['ip','route'],text=True),'binary':binary,'binarySha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'sourceSha256':{str(p):hashlib.sha256((repo/p).read_bytes()).hexdigest() for p in ['src/main.rs','src/download_orchestrator.rs','src/peer_acquire.rs','src/download.rs','src/p2p.rs','src/lib.rs']},'checks':checks,'passed':sum(c['passed'] for c in checks),'total':len(checks)}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'passed':receipt['passed'],'total':receipt['total'],'failed':[c for c in checks if not c['passed']],'evidence':str(out)},indent=2))

assert all(c['passed'] for c in checks)
