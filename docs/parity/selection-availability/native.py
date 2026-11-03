#!/usr/bin/env python3
"""Actual Linux GUI clicks plus CLI receipt comparison; no Rust test harness.
Requires Xvfb, xdotool, xclip, ImageMagick import, tesseract on PATH.
MODSYNC_BIN selects the production desktop binary; MODSYNC_PROOF_OUT is optional.
"""
import csv, hashlib, io, json, os, pathlib, subprocess, tempfile, time, zipfile

scenario = os.environ.get('MODSYNC_NATIVE_CASE', 'base')
assert scenario in ('base','nested','receipt-failure','selectors','wizard','aspyr-legacy','aspyr-supported','widescreen'), 'Unknown native case'
wizard_route = scenario in ('wizard','widescreen') or scenario.startswith('aspyr-')
binary = pathlib.Path(os.environ['MODSYNC_BIN']).resolve()
out = pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', tempfile.mkdtemp(prefix='modsync-native-receipt-'))).resolve()
out.mkdir(parents=True, exist_ok=True)
root = pathlib.Path(tempfile.mkdtemp(prefix='modsync-native-inputs-'))
env = os.environ.copy()
for key, directory in [('HOME','home'), ('XDG_CONFIG_HOME','config'), ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache'), ('XDG_RUNTIME_DIR','runtime')]:
    path = root/directory; path.mkdir(mode=0o700); env[key] = str(path)
env.update(DISPLAY=os.environ.get('MODSYNC_DISPLAY', ':96'), LIBGL_ALWAYS_SOFTWARE='1', OMP_THREAD_LIMIT='1')
for directory in ['mods','game','settings']: (root/directory).mkdir()
with zipfile.ZipFile(root/'mods/good.zip','w') as archive: archive.writestr('good.txt','generic fixture')
(root/'mods/broken.zip').write_bytes(b'not an archive')
def component(guid, archive=None, dependencies=()):
    return dict(Guid=guid, Name=guid, IsSelected=False, Dependencies=list(dependencies), Instructions=[dict(Action='Extract',Source=['<<modDirectory>>/'+archive])] if archive else [])
if scenario == 'nested':
    leaf=component('leaf','good.zip'); leaf['IsSelected']=True
    middle=component('middle'); middle.update(IsSelected=True,Options=[leaf],Instructions=[dict(Action='Choose',Source=['leaf'])])
    parent=component('parent'); parent.update(Options=[middle,component('badoption','absent-option.zip')],Instructions=[dict(Action='Choose',Source=['middle','badoption'])])
    components=[parent,component('usesleaf',dependencies=['leaf']),component('independent')]
    (root/'guide.json').write_text(json.dumps({'thisMod':components}))
else:
    (root/'guide.json').write_text(json.dumps({'thisMod':[component('good','good.zip'),component('missing','missing.zip'),component('broken','broken.zip'),component('dependent',dependencies=['missing']),component('transitive',dependencies=['dependent']),component('independent')]}))
if scenario.startswith('aspyr-'):
    (root/'game/swkotor2.exe').write_bytes(b'')
    if scenario == 'aspyr-supported':
        # Empty detection markers, never a claim of runnable game resources.
        for relative in ['override/cntrl_ps3_eng.tga', 'override/cntrl_ps3_fre.tga', 'override/cntrl_ps3_ger.tga', 'override/cntrl_ps3_ita.tga', 'override/cntrl_ps3_spa.tga', 'override/cntrl_xb360_eng.tga', 'override/cntrl_xb360_fre.tga', 'override/cntrl_xb360_ger.tga', 'override/cntrl_xb360_ita.tga', 'override/cntrl_xb360_spa.tga', 'override/cus_button_a.tga', 'override/cus_button_aps.tga', 'override/cus_button_b.tga', 'override/cus_button_bps.tga', 'override/cus_button_x.tga', 'override/cus_button_xps.tga', 'override/cus_button_y.tga', 'override/cus_button_yps.tga', 'override/cus_gpad_bg.tga', 'override/cus_gpad_fper.tga', 'override/cus_gpad_fper2.tga', 'override/cus_gpad_gen.tga', 'override/cus_gpad_gen2.tga', 'override/cus_gpad_hand.tga', 'override/cus_gpad_hand2.tga', 'override/cus_gpad_help.tga', 'override/cus_gpad_help2.tga', 'override/cus_gpad_map.tga', 'override/cus_gpad_map2.tga', 'override/cus_gpad_save.tga', 'override/cus_gpad_save2.tga', 'override/cus_gpad_solo.tga', 'override/cus_gpad_solo2.tga', 'override/cus_gpad_solox.tga', 'override/cus_gpad_solox2.tga', 'override/cus_gpad_ste.tga', 'override/cus_gpad_ste2.tga', 'override/cus_gpad_ste3.tga', 'override/custom.txt', 'override/custpnl_p.gui', 'override/d2xfnt_d16x16b.tga', 'override/d2xfont16x16b_ps.tga', 'override/d2xfont16x16b.tga', 'override/d3xfnt_d16x16b.tga', 'override/d3xfont16x16b_ps.tga', 'override/d3xfont16x16b.tga', 'override/diafnt16x16b_ps.tga', 'override/dialogfont16x16b.tga', 'override/equip_p.gui', 'override/fx_step_splash.MDL', 'override/gamepad.txt', 'override/gui_scroll.wav', 'override/handmaiden.DLG', 'override/lbl_miscroll_op']:
            path=root/'game'/relative; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(b'')
    good=component('aspyrgood','good.zip');good.update(AspyrExclusive=True,IsSelected=True)
    missing=component('aspyrmissing','missing.zip');missing.update(AspyrExclusive=True)
    required=component('requiresaspyr',dependencies=['aspyrgood']);required['IsSelected']=True
    transitive=component('transitiveaspyr',dependencies=['requiresaspyr']);transitive['IsSelected']=True
    (root/'guide.json').write_text(json.dumps({'thisMod':[good,missing,required,transitive,component('independent')],'TargetGame':'KOTOR2','AspyrExclusiveWarningContent':'Fixture Aspyr compatibility notice'}))
if scenario == 'wizard':
    (root/'game/swkotor.exe').write_bytes(b'') # Detection marker only; never execute or install.
if scenario == 'widescreen':
    (root/'game/swkotor.exe').write_bytes(b'')
    (root/'mods/base.txt').write_text('controlled base fixture')
    def copy_component(guid, target):
        item=component(guid)
        item['Instructions']=[dict(Action='Copy',Source=['<<modDirectory>>/base.txt'],Destination='<<kotorDirectory>>/override/'+target)]
        return item
    base=copy_component('base','base.txt'); base['IsSelected']=True
    good=copy_component('widegood','wide.txt'); good['WidescreenOnly']=True
    missing=component('widemissing','absent.zip'); missing['WidescreenOnly']=True
    (root/'guide.json').write_text(json.dumps({'thisMod':[base,good,missing],'WidescreenWarningContent':'Fixture widescreen compatibility notice'}))
if scenario == 'selectors':
    data=json.loads((root/'guide.json').read_text())
    tiers=['Essential','Essential','Recommended','Suggested','Optional','Recommended']
    categories=['Core','Core','Visual','Extras','Extras','Visual']
    for item,tier,category in zip(data['thisMod'],tiers,categories):
        item.update(Tier=tier,Category=[category])
    (root/'guide.json').write_text(json.dumps(data))
if scenario == 'receipt-failure':
    (root/'mods/a-receipt-only.txt').write_text('before')
    with (root/'mods/zz-padding.bin').open('wb') as file: file.truncate(256*1024*1024)
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def inputs(): return {str(p.relative_to(root)):digest(p) for directory in [root/'mods',root/'game'] for p in directory.rglob('*') if p.is_file()} | {'guide.json':digest(root/'guide.json')}
original=inputs(); events=[]; comparisons=[]; gui=None; server=None
(out/'fixture-guide.json').write_bytes((root/'guide.json').read_bytes())

def run(args): return subprocess.run(args,env=env,check=True,capture_output=True,text=True).stdout

def ocr_source(path):
    if scenario.startswith('aspyr-'):
        # Preserve the native screenshot; normalize a disposable OCR-only copy.
        normalized=out/'ocr-only.png'
        run(['convert',str(path),'-colorspace','Gray','-auto-level','-negate',str(normalized)])
        return normalized
    return path

def screen(label):
    path=out/(label+'.png'); run(['import','-window','root',str(path)])
    return run(['tesseract',str(ocr_source(path)),'stdout',*(['--psm','11'] if 'modal' in label or scenario == 'widescreen' or scenario.startswith('aspyr-') else [])])

def await_screen(label, text):
    deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        result=screen(label)
        normalized=lambda value: ''.join(character.lower() for character in value if character.isalnum())
        if normalized(text) in normalized(result): return
        if gui is not None and gui.poll() is not None: raise AssertionError('GUI exited')
        time.sleep(.2)
    raise AssertionError((label,text,result))

def click(label,x,y):
    events.append(dict(action=label,x=x,y=y))
    run(['xdotool','mousemove',str(x),str(y),'click','1'])

def click_label(label, column=None, checkbox=False, bounds=None):
    if scenario.startswith('aspyr-') and label in ('Next','Prepare selection receipt','Copy selection receipt','Select all'):
        screen('before-aspyr-control')
        coordinates={'Next':(1218,741),'Prepare selection receipt':(90,317),'Copy selection receipt':(90,338),'Select all':(43,380)}
        click(label,*coordinates[label])
        return
    run(['import','-window','root',str(out/'before-label-click.png')])
    rows=list(csv.DictReader(io.StringIO(run(['tesseract',str(ocr_source(out/'before-label-click.png')),'stdout','--psm','11','tsv'])),delimiter='\t'))
    words=[r for r in rows if r['text'].strip()]
    wanted=label.lower().split()
    for i in range(len(words)-len(wanted)+1):
        match=words[i:i+len(wanted)]
        if [r['text'].lower().strip('()[]….') for r in match]==wanted and (column is None or (int(match[0]['left'])<640)==(column=='left')) and (bounds is None or (bounds[0] <= int(match[0]['left']) <= bounds[1] and bounds[2] <= int(match[0]['top']) <= bounds[3])):
            first,last=match[0],match[-1]
            x=(int(first['left'])+int(last['left'])+int(last['width']))//2
            y=int(first['top'])+int(first['height'])//2
            click(label,int(first['left'])-(24 if column=='left' else 12) if checkbox else x,y); return
    raise AssertionError(('Visible label not found',label))

def compare_receipt(label, flags, expected):
    if wizard_route: click_label('Prepare selection receipt')
    else: click('Prepare selection receipt',91,266)
    await_screen(label+'-ready','Copy selection receipt')
    if wizard_route: click_label('Copy selection receipt')
    else: click('Copy selection receipt',89,287)
    receipt=json.loads(run(['xclip','-selection','clipboard','-o']))
    cli=json.loads(run([str(binary),'selection',*args,'--json','--receipt',*flags]))
    assert receipt==cli['receipt'],(label,receipt,cli)
    comparisons.append(dict(label=label,cliFlags=flags,selected=expected))
    assert receipt['selected']==expected,(label,receipt)
    assert inputs()==original,'Selection changed guide/mod/game inputs'
    (out/(label+'-gui.json')).write_text(json.dumps(receipt,indent=2)+'\n')
    (out/(label+'-cli.json')).write_text(json.dumps(cli,indent=2)+'\n')

try:
    with (out/'xvfb.log').open('w') as log:
        server=subprocess.Popen(['Xvfb',env['DISPLAY'],'-screen','0','1280x900x24','-nolisten','tcp'],env=env,stdout=log,stderr=subprocess.STDOUT)
    deadline=time.monotonic()+10
    while subprocess.run(['xdotool','getdisplaygeometry'],env=env,capture_output=True).returncode:
        assert time.monotonic()<deadline,'Private X server unavailable'
        time.sleep(.1)
    args=['--input',str(root/'guide.json'),'--source-dir',str(root/'mods'),'--game-dir',str(root/'game')]
    with (out/'gui.log').open('w') as log:
        gui=subprocess.Popen([str(binary),'gui','--allow-multiple-instances','--settings-dir',str(root/'settings'),*args],env=env,stdout=log,stderr=subprocess.STDOUT)
    await_screen('startup','Keep telemetry off')
    click('Keep telemetry off',425,478)
    screen('telemetry-dismissed')
    if wizard_route:
        click('Stop sharing before wizard navigation',415,53)
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('wizard-mod-directory','Mod Workspace Directory')
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('wizard-game-directory','KOTOR Game Directory')
        click_label('Next',bounds=(1100,1280,650,850))
        if scenario.startswith('aspyr-'):
            await_screen('aspyr-notice','Fixture Aspyr compatibility notice')
            click_label('Next',bounds=(1100,1280,650,850))
        await_screen('wizard-selection','Prepare selection receipt')
    else:
        click('Summary / selection',166,32)
        await_screen('pause','Pause sharing to edit')
        click('Pause sharing to edit',78,189)
        await_screen('before-select','Prepare selection receipt')
    if scenario.startswith('aspyr-'):
        supported=scenario=='aspyr-supported'
        saved=['aspyrgood','requiresaspyr','transitiveaspyr'] if supported else []
        expected=[*saved,'independent']
        await_screen('aspyr-saved',f'{len(saved)} of 5 mods selected')
        compare_receipt('aspyr-saved',['--use-file-selection'],saved)
        click_label('Select all')
        await_screen('aspyr-selected',f'{len(expected)} of 5 mods selected')
        compare_receipt('aspyr-selected',[],expected)
        receipt=json.loads((out/'aspyr-selected-gui.json').read_text())
        assert receipt['detectedGame']==('Kotor2Aspyr' if supported else 'Kotor2Legacy'),receipt
        assert inputs()==original
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsUnchanged=True,receiptsEqual=True,scenario=scenario,gameMarkersOnly=True,noInstallOrDownload=True),indent=2)+'\n')
        print('PASS actual Aspyr notice, saved choice/dependent handling, and Select All with matching CLI receipts: '+scenario)
    elif scenario == 'widescreen':
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('downloads','Nothing downloads automatically')
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('validation','Run Validation')
        click_label('Run Validation')
        await_screen('validated','Validation passed')
        assert inputs()==original,'Validation mutated input files'
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('install-start','Next asks you to confirm')
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('install-modal','Install selected mods?')
        click('Create snapshot and install',502,429)
        await_screen('installed','Installation complete')
        screen('installed-state')
        click('Continue',1218,691)
        await_screen('base-complete','Base Installation Complete')
        after_base=inputs()
        expected_after_base=original | {'game/override/base.txt/base.txt':digest(root/'mods/base.txt')}
        assert after_base==expected_after_base,('Unexpected base install output',after_base)
        click('Next after verified base completion',1218,691)
        await_screen('widescreen-notice','Widescreen Mods')
        click('Next to widescreen selection',1218,691)
        await_screen('widescreen-page','Widescreen Mod Selection')
        if 'Pause sharing to edit' in screen('widescreen-edit-state'):
            click('Pause sharing to edit widescreen selection',76,302)
        await_screen('widescreen-selection','Next skips the empty widescreen phase')
        click('Disabled missing-archive widescreen checkbox',23,306)
        await_screen('widescreen-missing','Next skips the empty widescreen phase')
        click('Available widescreen checkbox',23,285)
        await_screen('widescreen-good','Validate the remaining phase against the verified base completion')
        click('Validate remaining phase',169,551)
        await_screen('widescreen-validated','Validation passed')
        assert inputs()==after_base,'Widescreen selection mutated input files'
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,inputHashes=original,finalHashes=inputs(),fixtureGuideSha256=digest(out/'fixture-guide.json'),scenario=scenario,baseInstallOnly=True,selectionInputsUnchanged=True),indent=2)+'\n')
        print('PASS actual base Copy installation and widescreen selections')
    elif scenario == 'wizard':
        click_label('Select all')
        await_screen('wizard-selected','2 of 6 mods selected')
        compare_receipt('wizard-selected',[],['good','independent'])
        click_label('Next',bounds=(1100,1280,650,850))
        await_screen('wizard-download-explanation','Nothing downloads automatically')
        click_label('Back',bounds=(1000,1280,650,850))
        await_screen('wizard-selection-return','2 of 6 mods selected')
        compare_receipt('wizard-selection-return',[],['good','independent'])
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsUnchanged=inputs()==original,receiptsEqual=True,scenario=scenario,gameMarkerOnly=True,noInstallOrDownload=True),indent=2)+'\n')
        print('PASS actual wizard directory navigation, Select All, next/back retention; receipts match CLI; no install/download')
    elif scenario == 'selectors':
        click('Open tier dropdown',65,351)
        click_label('Recommended')
        click_label('Select through tier')
        await_screen('tier-selected','2 of 6 mods selected')
        compare_receipt('tier-selected',['--use-file-selection','--select-through-tier','Recommended'],['good','independent'])
        click('Deselect all after tier',115,329)
        await_screen('tier-deselected','0 of 6 mods selected')
        click_label('Clear filters')
        click('Open category dropdown',294,351)
        click_label('Core')
        click_label('Select category')
        await_screen('category-selected','1 of 6 mods selected')
        compare_receipt('category-selected',['--use-file-selection','--add-category','Core'],['good'])
        click_label('Select multiple categories')
        await_screen('category-modal','Select mods by category')
        click('Modal Core checkbox',552,381)
        click('Modal Visual checkbox',552,423)
        click('Modal Extras checkbox',552,402)
        screen('category-modal-chosen')
        click('Apply category dialog',597,444)
        await_screen('categories-selected','2 of 6 mods selected')
        category_flags=['--use-file-selection','--add-category','Core','--add-category','Visual','--add-category','Extras']
        compare_receipt('categories-selected',category_flags,['good','independent'])
        click_label('Select multiple categories')
        await_screen('category-modal-cancel','Select mods by category')
        click('Modal Core checkbox',552,381)
        click('Cancel category dialog',679,444)
        compare_receipt('category-modal-cancelled',category_flags,['good','independent'])
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsUnchanged=inputs()==original,receiptsEqual=True,scenario=scenario),indent=2)+'\n')
        print('PASS actual tier, single-category, multi-category and cancel controls; receipts match CLI; inputs unchanged')
    elif scenario == 'receipt-failure':
        click('Select all',43,309)
        await_screen('failure-baseline-selected','2 of 6 mods selected')
        compare_receipt('failure-baseline',[],['good','independent'])
        first=root/'mods/a-receipt-only.txt'
        first.chmod(0)
        try:
            click('Prepare receipt with unreadable inventory file',91,266)
            await_screen('permission-error','Permission denied')
            shown=screen('permission-error')
            assert 'Copy selection receipt' not in shown, 'Failed export exposed stale Copy control'
            assert '2 of 6 mods selected' in shown, 'Failed export changed selection'
            cli=subprocess.run([str(binary),'selection',*args,'--json','--receipt'],env=env,capture_output=True,text=True)
            assert cli.returncode!=0 and not cli.stdout.strip() and 'Permission denied' in cli.stderr,(cli.stdout,cli.stderr)
            (out/'permission-error-cli.txt').write_text(cli.stderr)
        finally:
            first.chmod(0o600)
        compare_receipt('permission-repaired',[],['good','independent'])
        import ctypes, select, signal
        libc=ctypes.CDLL(None,use_errno=True)
        fd=libc.inotify_init1(os.O_CLOEXEC|os.O_NONBLOCK)
        assert fd>=0 and libc.inotify_add_watch(fd,os.fsencode(root/'mods/zz-padding.bin'),0x20)>=0
        try:
            click('Prepare receipt with synchronized later mutation',91,266)
            for iteration in range(2):
                assert select.select([fd],[],[],15)[0], 'Receipt did not open final inventory file'
                os.read(fd,4096)
            gui.send_signal(signal.SIGSTOP)
            _,status=os.waitpid(gui.pid,os.WUNTRACED)
            assert os.WIFSTOPPED(status), 'GUI exited before controlled mutation'
            first.write_text('after!')
            gui.send_signal(signal.SIGCONT)
            await_screen('changed-inventory-error','Mod inventory changed while preparing receipt')
            shown=screen('changed-inventory-error')
            assert 'Copy selection receipt' not in shown, 'Changing inventory exposed stale Copy control'
            assert '2 of 6 mods selected' in shown, 'Changing inventory changed selection'
        finally:
            os.close(fd)
            if gui.poll() is None: gui.send_signal(signal.SIGCONT)
            first.write_text('before')
        compare_receipt('changed-inventory-repaired',[],['good','independent'])
        assert inputs()==original
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsRestoredAfterControlledMutation=True,receiptsEqual=True,permissionErrorShown=True,changedInventoryErrorShown=True,staleCopyHidden=True,selectedStatePreserved=True,scenario=scenario),indent=2)+'\n')
        print('PASS actual GUI unreadable/changing-inventory export errors; stale Copy hidden; selections retained; repaired receipts match CLI')
    elif scenario == 'nested':
        click('Select all',43,309)
        await_screen('nested-selected','3 of 3 mods selected')
        compare_receipt('nested-selected',[],['parent','middle','leaf','usesleaf','independent'])
        click_label('Expand all')
        await_screen('nested-expanded','usesleaf')
        click_label('parent',column='left',checkbox=True)
        await_screen('parent-deselected','1 of 3 mods selected')
        compare_receipt('parent-deselected',['--deselect','parent'],['independent'])
        # Dormant choices remain visible. Unchecking the middle removes its subtree.
        click_label('middle',column='right',checkbox=True)
        compare_receipt('middle-deselected',['--deselect','parent','--deselect-option','middle'],['independent'])
        click_label('middle',column='right',checkbox=True)
        await_screen('middle-restored','2 of 3 mods selected')
        compare_receipt('middle-restored',['--deselect','parent','--deselect-option','middle','--option','middle'],['parent','middle','leaf','independent'])
        # Missing option is disabled; a real click cannot change the selected set.
        click('Disabled badoption checkbox',651,638)
        compare_receipt('badoption-refused',['--deselect','parent','--deselect-option','middle','--option','middle','--option','badoption'],['parent','middle','leaf','independent'])
        # A leaf toggle traverses the option ancestor chain and required dependent.
        click_label('leaf',column='right',checkbox=True)
        await_screen('leaf-deselected','1 of 3 mods selected')
        compare_receipt('leaf-deselected',['--deselect','parent','--deselect-option','middle','--option','middle','--option','badoption','--deselect-option','leaf'],['independent'])
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsUnchanged=inputs()==original,receiptsEqual=True,scenario=scenario),indent=2)+'\n')
        print('PASS actual nested parent/subtree/leaf controls and unavailable-option refusal; receipts match CLI; saved choices retained')
    else:
        click('Select all',43,309)
        await_screen('after-select','2 of 6 mods selected')
        click('Prepare selection receipt',91,266)
        await_screen('receipt-ready','Copy selection receipt')
        click('Copy selection receipt',89,287)
        receipt=json.loads(run(['xclip','-selection','clipboard','-o']))
        cli=json.loads(run([str(binary),'selection',*args,'--json','--receipt']))
        assert receipt==cli['receipt'],(receipt,cli)
        comparisons.append(dict(label='select-all',cliFlags=[],selected=['good','independent']))
        assert receipt['selected']==['good','independent'],receipt
        assert {entry['guid']:entry['category'] for entry in receipt['excluded']}=={'missing':'missing_archive','broken':'invalid_archive','dependent':'unavailable_dependency','transitive':'unavailable_dependency'},receipt
        assert inputs()==original,'Selection changed guide/mod/game inputs'
        (out/'gui-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
        (out/'cli.json').write_text(json.dumps(cli,indent=2)+'\n')
        # A later actual selection action must invalidate the cached export.
        click('Deselect all',115,329)
        await_screen('after-deselect','0 of 6 mods selected')
        assert 'Copy selection receipt' not in screen('after-deselect'), 'Stale receipt remained available'
        click_label('Select group')
        await_screen('group-selected','2 of 6 mods selected')
        assert 'Deselect group' in screen('group-selected'), 'Eligible group cannot be deselected'
        compare_receipt('group-selected',[],['good','independent'])
        click_label('Deselect group')
        await_screen('group-deselected','0 of 6 mods selected')
        compare_receipt('group-deselected',['--use-file-selection'],[])
        # Filter first, then press the actual Select All button on the filtered view.
        run(['xdotool','windowfocus',run(['xdotool','search','--name','^ModSync.rs$']).split()[0]])
        click('Search field',300,329)
        run(['xdotool','type','--clearmodifiers','good'])
        await_screen('filtered','Showing 1 of 6 mods')
        click('Filtered Select all',43,329)
        await_screen('filtered-selected','1 of 6 mods selected')
        compare_receipt('filtered-selected',['--use-file-selection','--select-visible-search','good'],['good'])
        (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,comparisons=comparisons,inputHashes=original,fixtureGuideSha256=digest(out/'fixture-guide.json'),inputsUnchanged=True,receiptsEqual=True,staleReceiptCleared=True,eligibleGroupToggle=True,filteredSelectAll=True),indent=2)+'\n')
        print('PASS actual Select All, eligible-group select/deselect, filtered Select All; all receipts match CLI; stale receipt cleared; inputs unchanged')
finally:
    for process in [gui,server]:
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
