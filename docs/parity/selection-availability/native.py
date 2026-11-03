#!/usr/bin/env python3
"""Actual Linux GUI clicks plus CLI receipt comparison; no Rust test harness.
Requires Xvfb, xdotool, xclip, ImageMagick import, tesseract on PATH.
MODSYNC_BIN selects the production desktop binary; MODSYNC_PROOF_OUT is optional.
"""
import csv, hashlib, io, json, os, pathlib, subprocess, tempfile, time, zipfile

scenario = os.environ.get('MODSYNC_NATIVE_CASE', 'base')
assert scenario in ('base','nested'), 'Unknown native case'
binary = pathlib.Path(os.environ['MODSYNC_BIN']).resolve()
out = pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', tempfile.mkdtemp(prefix='modsync-native-receipt-'))).resolve()
out.mkdir(parents=True, exist_ok=True)
root = pathlib.Path(tempfile.mkdtemp(prefix='modsync-native-inputs-'))
env = os.environ.copy()
for key, directory in [('HOME','home'), ('XDG_CONFIG_HOME','config'), ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache'), ('XDG_RUNTIME_DIR','runtime')]:
    path = root/directory; path.mkdir(mode=0o700); env[key] = str(path)
env.update(DISPLAY=os.environ.get('MODSYNC_DISPLAY', ':96'), LIBGL_ALWAYS_SOFTWARE='1')
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
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def inputs(): return {str(p.relative_to(root)):digest(p) for directory in [root/'mods',root/'game'] for p in directory.rglob('*') if p.is_file()} | {'guide.json':digest(root/'guide.json')}
original=inputs(); events=[]; comparisons=[]; gui=None; server=None
(out/'fixture-guide.json').write_bytes((root/'guide.json').read_bytes())

def run(args): return subprocess.run(args,env=env,check=True,capture_output=True,text=True).stdout

def screen(label):
    path=out/(label+'.png'); run(['import','-window','root',str(path)])
    return run(['tesseract',str(path),'stdout'])

def await_screen(label, text):
    deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        result=screen(label)
        if text in result: return
        if gui is not None and gui.poll() is not None: raise AssertionError('GUI exited')
        time.sleep(.2)
    raise AssertionError((label,text,result))

def click(label,x,y):
    events.append(dict(action=label,x=x,y=y))
    run(['xdotool','mousemove',str(x),str(y),'click','1'])

def click_label(label, column=None, checkbox=False):
    screen('before-label-click')
    rows=list(csv.DictReader(io.StringIO(run(['tesseract',str(out/'before-label-click.png'),'stdout','--psm','11','tsv'])),delimiter='\t'))
    words=[r for r in rows if r['text'].strip()]
    wanted=label.lower().split()
    for i in range(len(words)-len(wanted)+1):
        match=words[i:i+len(wanted)]
        if [r['text'].lower().strip('()[]') for r in match]==wanted and (column is None or (int(match[0]['left'])<640)==(column=='left')):
            first,last=match[0],match[-1]
            x=(int(first['left'])+int(last['left'])+int(last['width']))//2
            y=int(first['top'])+int(first['height'])//2
            click(label,int(first['left'])-(24 if column=='left' else 12) if checkbox else x,y); return
    raise AssertionError(('Visible label not found',label))

def compare_receipt(label, flags, expected):
    click('Prepare selection receipt',91,266)
    await_screen(label+'-ready','Copy selection receipt')
    click('Copy selection receipt',89,287)
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
    click('Summary / selection',166,32)
    await_screen('pause','Pause sharing to edit')
    click('Pause sharing to edit',78,189)
    await_screen('before-select','Prepare selection receipt')
    if scenario == 'nested':
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
