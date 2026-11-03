#!/usr/bin/env python3
"""Actual Linux GUI clicks plus CLI receipt comparison; no Rust test harness.
Requires Xvfb, xdotool, xclip, ImageMagick import, tesseract on PATH.
MODSYNC_BIN selects the production desktop binary; MODSYNC_PROOF_OUT is optional.
"""
import hashlib, json, os, pathlib, subprocess, tempfile, time, zipfile

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
(root/'guide.json').write_text(json.dumps({'thisMod':[component('good','good.zip'),component('missing','missing.zip'),component('broken','broken.zip'),component('dependent',dependencies=['missing']),component('transitive',dependencies=['dependent']),component('independent')]}))
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def inputs(): return {str(p.relative_to(root)):digest(p) for directory in [root/'mods',root/'game'] for p in directory.rglob('*') if p.is_file()} | {'guide.json':digest(root/'guide.json')}
original=inputs(); events=[]; gui=None; server=None

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
    click('Select all',43,309)
    await_screen('after-select','2 of 6 mods selected')
    click('Prepare selection receipt',91,266)
    await_screen('receipt-ready','Copy selection receipt')
    click('Copy selection receipt',89,287)
    receipt=json.loads(run(['xclip','-selection','clipboard','-o']))
    cli=json.loads(run([str(binary),'selection',*args,'--json','--receipt']))
    assert receipt==cli['receipt'],(receipt,cli)
    assert receipt['selected']==['good','independent'],receipt
    assert {entry['guid']:entry['category'] for entry in receipt['excluded']}=={'missing':'missing_archive','broken':'invalid_archive','dependent':'unavailable_dependency','transitive':'unavailable_dependency'},receipt
    assert inputs()==original,'Selection changed guide/mod/game inputs'
    (out/'gui-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    (out/'cli.json').write_text(json.dumps(cli,indent=2)+'\n')
    # A later actual selection action must invalidate the cached export.
    click('Deselect all',115,329)
    await_screen('after-deselect','0 of 6 mods selected')
    assert 'Copy selection receipt' not in screen('after-deselect'), 'Stale receipt remained available'
    (out/'proof.json').write_text(json.dumps(dict(binarySha256=digest(binary),actions=events,inputsUnchanged=True,receiptsEqual=True,staleReceiptCleared=True),indent=2)+'\n')
    print('PASS actual Select All and receipt-copy buttons; exact CLI/GUI receipt match; deselection invalidates receipt; inputs unchanged')
finally:
    for process in [gui,server]:
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
