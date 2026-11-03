import io, json, os, pathlib, subprocess, tempfile, zipfile
binary = os.environ.get('MODSYNC_BIN', str(pathlib.Path(__file__).resolve().parents[3] / 'target/debug/modsync'))
with tempfile.TemporaryDirectory(prefix='modsync-selection-') as tmp:
    root=pathlib.Path(tmp); mods=root/'mods'; game=root/'game'; mods.mkdir(); game.mkdir()
    with zipfile.ZipFile(mods/'good.zip','w') as archive: archive.writestr('file.txt','valid data')
    (mods/'broken.zip').write_bytes(b'not an archive')
    def component(guid, archive=None, deps=(), selected=False):
        return dict(Guid=guid, Name=guid, IsSelected=selected, Dependencies=list(deps), Instructions=([dict(Action='Extract', Source=['<<modDirectory>>/'+archive])] if archive else []))
    components=[component('good','good.zip'),component('missing','missing.zip'),component('broken','broken.zip'),component('dependent',deps=['missing']),component('transitive',deps=['dependent']),component('missing-ref',deps=['absent']),component('independent')]
    guide=root/'guide.json'; guide.write_text(json.dumps({'thisMod':components}))
    original=guide.read_bytes()
    def run(*args):
        p=subprocess.run([binary,'selection','--input',str(guide),'--source-dir',str(mods),'--game-dir',str(game),'--json',*args],text=True,capture_output=True)
        assert p.returncode==0,p.stderr
        return json.loads(p.stdout)
    result=run(); assert set(result['selected'])=={'good','independent'},result
    unavailable={entry['guid']:entry['reason'] for entry in result['availability']['unavailable']}
    assert set(unavailable)=={'missing','broken','dependent','transitive','missing-ref'},unavailable
    assert 'missing' in unavailable['transitive'],unavailable
    assert guide.read_bytes()==original
    assert list(game.iterdir())==[]
    print('PASS available-only selection, corrupt/missing inputs, transitive dependency reasons, unknown dependency, read-only guide/game')
    categories={entry['guid']:entry['category'] for entry in result['availability']['unavailable']}
    assert categories=={'missing':'missing_archive','broken':'invalid_archive','dependent':'unavailable_dependency','transitive':'unavailable_dependency','missing-ref':'missing_dependency'},categories
    receipt=run('--receipt')['receipt']
    assert receipt['schemaVersion']==1 and receipt['selected']==result['selected'],receipt
    assert {entry['guid']:entry['category'] for entry in receipt['excluded']}==categories,receipt
    import hashlib, shutil
    assert {entry['path']:entry['sha256'] for entry in receipt['modInventory']}=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in mods.iterdir()},receipt
    relocated=root/'relocated'; shutil.copytree(mods,relocated)
    p=subprocess.run([binary,'selection','--input',str(guide),'--source-dir',str(relocated),'--game-dir',str(game),'--json','--receipt'],text=True,capture_output=True)
    assert p.returncode==0,p.stderr
    relocated_receipt=json.loads(p.stdout)['receipt']
    for key in ['guideDefinitionSha256','modInventorySha256','modInventory','selected']:
        assert receipt[key]==relocated_receipt[key],(key,receipt,relocated_receipt)
    (mods/'unused.txt').write_text('inventory change')
    changed=run('--receipt')['receipt']
    assert changed['modInventorySha256']!=receipt['modInventorySha256'],changed
    assert changed['guideDefinitionSha256']==receipt['guideDefinitionSha256'],changed
    (mods/'unused.txt').unlink()
    components[0]['Name']='changed definition'; guide.write_text(json.dumps({'thisMod':components}))
    changed=run('--receipt')['receipt']
    assert changed['guideDefinitionSha256']!=receipt['guideDefinitionSha256'],changed
    components[0]['Name']='good'; guide.write_bytes(original)
    unchecked=run('--use-file-selection','--receipt')['receipt']
    assert unchecked['guideDefinitionSha256']==receipt['guideDefinitionSha256'],unchecked
    assert unchecked['selected']==[],unchecked
    assert next(entry for entry in unchecked['excluded'] if entry['guid']=='good')['category']=='not_selected',unchecked
    assert guide.read_bytes()==original and list(game.iterdir())==[]
    print('PASS normalized reasons and content receipts: file hashes verified, relocation stable, changed inputs detected, choices separated')
    components[1]['IsSelected']=True; guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    result=run('--use-file-selection'); assert result['selected']==[],result
    assert any('missing' in outcome['autoDeselected'] for outcome in result['outcomes']),result
    assert guide.read_bytes()==original
    print('PASS saved unavailable selection cleared without rewriting source guide')
    components[1]['IsSelected']=False
    components.append(component('conflict',selected=True)); components[1]['Restrictions']=['conflict']
    guide.write_text(json.dumps({'thisMod':components})); result=run('--use-file-selection','--select-visible-search','missing')
    assert result['selected']==['conflict'],result
    print('PASS rejected selection preserves saved conflicting choice')

    # An unavailable unchosen option cannot disable a valid saved option or parent.
    parent=component('options-parent')
    parent['Instructions']=[dict(Action='Choose',Source=['saved-option','missing-option'])]
    parent['Options']=[component('saved-option','good.zip',selected=True),component('missing-option','absent-option.zip')]
    components=[parent,component('option-dependent',deps=['missing-option'])]
    guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    result=run(); assert set(result['selected'])=={'options-parent','saved-option'},result
    unavailable={entry['guid']:entry['reason'] for entry in result['availability']['unavailable']}
    assert 'missing-option' in unavailable and 'option-dependent' in unavailable,result
    assert guide.read_bytes()==original
    print('PASS option choices retained, unchosen missing option isolated, option dependency blocked')

    # A dependency cycle with valid local inputs is not itself unavailable.
    components=[component('cycle-a','good.zip',deps=['cycle-b']),component('cycle-b',deps=['cycle-a'])]
    guide.write_text(json.dumps({'thisMod':components})); result=run()
    assert set(result['selected'])=={'cycle-a','cycle-b'},result
    assert result['availability']['unavailable']==[],result
    print('PASS available required dependency cycle')

    # Refresh is a real disk inspection, not a GUID prefilter.
    (mods/'good.zip').write_bytes(b'now corrupt')
    result=run(); assert result['selected']==[],result
    assert len(result['availability']['unavailable'])==2,result
    print('PASS changed archive invalidates its transitive selection')
    with zipfile.ZipFile(mods/'good.zip','w') as archive: archive.writestr('file.txt','restored')
    result=run(); assert set(result['selected'])=={'cycle-a','cycle-b'},result
    print('PASS repaired archive becomes selectable')

    # A nested archive is generated by its earlier Extract, not a missing download.
    nested=io.BytesIO()
    with zipfile.ZipFile(nested,'w') as archive: archive.writestr('nested.txt','valid')
    with zipfile.ZipFile(mods/'outer.zip','w') as archive: archive.writestr('inner.zip',nested.getvalue())
    parent=component('nested','outer.zip')
    parent['Instructions'].append(dict(Action='Extract',Source=['<<modDirectory>>/outer/inner.zip']))
    guide.write_text(json.dumps({'thisMod':[parent]})); result=run()
    assert result['selected']==['nested'],result
    assert list(game.iterdir())==[]
    assert not (mods/'outer').exists()
    print('PASS nested archive input projection without extraction')

    # The incoming option-identity fix survives integration with archive eligibility.
    parent=component('identity-parent')
    parent['Instructions']=[dict(Action='Choose',Source=['identity-a','identity-b'])]
    parent['Options']=[component('identity-a','good.zip'),component('identity-b','good.zip',selected=True)]
    guide.write_text(json.dumps({'thisMod':[parent]})); result=run()
    assert set(result['selected'])=={'identity-parent','identity-b'},result
    print('PASS identical referenced options retain the saved identity')

    guide.write_text(json.dumps({'thisMod':[component('missing','missing.zip')]}))
    p=subprocess.run([binary,'validate','--input',str(guide),'--source-dir',str(mods),'--game-dir',str(game),'--json'],text=True,capture_output=True)
    assert p.returncode!=0
    assert 'No components are selected for validation' in p.stdout+p.stderr,(p.stdout,p.stderr)
    assert 'Missing required archive' in p.stderr,p.stderr
    assert not list(game.iterdir())
    print('PASS rooted validate uses native eligibility before pipeline')

    # A saved unavailable chain is cleared, while healthy saved choices survive.
    components=[component('saved-broken','broken.zip',selected=True),component('saved-dependent',deps=['saved-broken'],selected=True),component('saved-transitive',deps=['saved-dependent'],selected=True),component('healthy','good.zip',selected=True)]
    guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    result=run('--use-file-selection')
    assert result['selected']==['healthy'],result
    assert guide.read_bytes()==original
    print('PASS saved corrupt archive and transitive dependents cleared; healthy choice retained')

    parent=component('saved-parent',selected=True)
    parent['Instructions']=[dict(Action='Choose',Source=['healthy-option','bad-option'])]
    parent['Options']=[component('healthy-option','good.zip',selected=True),component('bad-option','broken.zip',selected=True)]
    guide.write_text(json.dumps({'thisMod':[parent]})); original=guide.read_bytes()
    result=run('--use-file-selection')
    assert set(result['selected'])=={'saved-parent','healthy-option'},result
    assert guide.read_bytes()==original
    print('PASS bad saved option cleared without losing healthy saved option or parent')

    # Parent deselection invalidates required nested options, even when their
    # saved flags remain true for a later deliberate re-selection.
    parent=component('nested-parent',selected=True)
    middle=component('middle',selected=True)
    middle['Options']=[component('leaf',selected=True)]
    parent['Options']=[middle]
    components=[parent,component('needs-leaf',deps=['leaf'],selected=True),component('needs-dependent',deps=['needs-leaf'],selected=True),component('independent',selected=True)]
    guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    result=run('--use-file-selection','--deselect','nested-parent')
    assert result['selected']==['independent'],result
    assert any('needs-leaf' in outcome['autoDeselected'] for outcome in result['outcomes']),result
    assert guide.read_bytes()==original
    result=run('--use-file-selection','--deselect','nested-parent','--select-visible-search','nested-parent')
    assert set(result['selected'])=={'nested-parent','middle','leaf','independent'},result
    print('PASS parent deselection clears nested-option dependents transitively, hides dormant flags, and retains option preferences')

    # Deselecting an option subtree must also remove users of its descendants.
    result=run('--use-file-selection','--deselect-option','middle')
    assert result['selected']==['independent'],result
    print('PASS option subtree deselection clears nested required dependents')

    # A missing nested option propagates through required edges at every depth.
    middle['Instructions']=[dict(Action='Choose',Source=['leaf'])]
    middle['Options'][0]['Instructions']=[dict(Action='Extract',Source=['<<modDirectory>>/missing-nested.zip'])]
    parent['Instructions']=[dict(Action='Choose',Source=['middle'])]
    guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    result=run('--use-file-selection')
    assert set(result['selected'])=={'nested-parent','middle','independent'},result
    assert {'leaf','needs-leaf','needs-dependent'} <= {entry['guid'] for entry in result['availability']['unavailable']},result
    assert guide.read_bytes()==original
    print('PASS missing nested option and transitive required dependents are unavailable')

    # Uncertain I/O must fail the entire scan rather than publish deselections.
    components=[component('saved-missing','missing.zip',selected=True),component('unreadable','good.zip',selected=True)]
    guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
    (mods/'good.zip').chmod(0)
    try:
        p=subprocess.run([binary,'selection','--input',str(guide),'--source-dir',str(mods),'--game-dir',str(game),'--use-file-selection','--json'],text=True,capture_output=True)
        assert p.returncode!=0,(p.stdout,p.stderr)
        assert 'inspection is incomplete' in p.stderr,(p.stdout,p.stderr)
        assert not p.stdout.strip(),p.stdout
        assert guide.read_bytes()==original
    finally:
        (mods/'good.zip').chmod(0o600)
    result=run('--use-file-selection'); assert result['selected']==['unreadable'],result
    print('PASS permission failure publishes no selection changes; successful retry clears only confirmed missing item')

    # Synchronize a metadata change with an actual archive open on Linux.
    # The owned CLI is stopped before mutation and always resumed/terminated.
    import sys
    if sys.platform.startswith('linux'):
        import ctypes, select, signal
        archive_path=mods/'changing.zip'
        with zipfile.ZipFile(archive_path,'w',compression=zipfile.ZIP_DEFLATED) as archive:
            with archive.open('payload.bin','w') as member:
                block=b'0'*(1024*1024)
                for _ in range(128): member.write(block)
        components=[component('saved-missing','missing.zip',selected=True),component('changing','changing.zip',selected=True)]
        guide.write_text(json.dumps({'thisMod':components})); original=guide.read_bytes()
        libc=ctypes.CDLL(None,use_errno=True)
        fd=libc.inotify_init1(os.O_CLOEXEC|os.O_NONBLOCK)
        assert fd>=0,ctypes.get_errno()
        assert libc.inotify_add_watch(fd,os.fsencode(archive_path),0x20)>=0,ctypes.get_errno() # IN_OPEN
        process=None
        try:
            process=subprocess.Popen([binary,'selection','--input',str(guide),'--source-dir',str(mods),'--game-dir',str(game),'--use-file-selection','--json'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            assert select.select([fd],[],[],10)[0], 'CLI did not open the watched archive'
            os.read(fd,4096)
            process.send_signal(signal.SIGSTOP)
            _,status=os.waitpid(process.pid,os.WUNTRACED)
            assert os.WIFSTOPPED(status),'CLI completed before the controlled mutation'
            metadata=archive_path.stat()
            os.utime(archive_path,ns=(metadata.st_atime_ns,metadata.st_mtime_ns+1_000_000_000))
            process.send_signal(signal.SIGCONT)
            stdout,stderr=process.communicate(timeout=30)
            assert process.returncode!=0,(stdout,stderr)
            assert 'changed during inspection' in stderr,(stdout,stderr)
            assert not stdout.strip(),stdout
            assert guide.read_bytes()==original
        finally:
            os.close(fd)
            if process is not None and process.poll() is None:
                process.send_signal(signal.SIGCONT)
                process.kill()
                process.communicate()
        print('PASS archive mutation during inspection aborts without publishing deselections')

        # Receipt-only files are not opened by archive inspection. Synchronize
        # with the final file's second hash pass, then mutate an earlier file
        # whose bytes have already been read. A per-file immediate check misses it.
        receipt_mods=root/'receipt-mods'; receipt_mods.mkdir()
        first=receipt_mods/'a-state.txt'; first.write_text('before')
        last=receipt_mods/'zz-padding.bin'
        with last.open('wb') as file: file.truncate(256*1024*1024)
        guide.write_text(json.dumps({'thisMod':[component('independent',selected=True)]})); original=guide.read_bytes()
        receipt_args=[binary,'selection','--input',str(guide),'--source-dir',str(receipt_mods),'--game-dir',str(game),'--use-file-selection','--json','--receipt']
        receipt_env=dict(os.environ,XDG_CONFIG_HOME=str(root/'receipt-config'))
        fd=libc.inotify_init1(os.O_CLOEXEC|os.O_NONBLOCK)
        assert fd>=0 and libc.inotify_add_watch(fd,os.fsencode(last),0x20)>=0
        process=None
        try:
            process=subprocess.Popen(receipt_args,env=receipt_env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            for iteration in range(2):
                assert select.select([fd],[],[],15)[0], 'Receipt did not open final inventory file'
                os.read(fd,4096)
            process.send_signal(signal.SIGSTOP)
            _,status=os.waitpid(process.pid,os.WUNTRACED)
            assert os.WIFSTOPPED(status),'Receipt completed before controlled mutation'
            first.write_text('after!')
            process.send_signal(signal.SIGCONT)
            stdout,stderr=process.communicate(timeout=30)
            assert process.returncode!=0,(stdout,stderr)
            assert 'Mod inventory changed while preparing receipt' in stderr,(stdout,stderr)
            assert not stdout.strip(),stdout
            assert guide.read_bytes()==original and list(game.iterdir())==[]
        finally:
            os.close(fd)
            if process is not None and process.poll() is None:
                process.send_signal(signal.SIGCONT); process.kill(); process.communicate()
        print('PASS receipt rejects an earlier-file mutation during the later-file final hash pass; no partial JSON')

        first.chmod(0)
        try:
            p=subprocess.run(receipt_args,env=receipt_env,text=True,capture_output=True)
            assert p.returncode!=0 and not p.stdout.strip(),(p.stdout,p.stderr)
            assert 'Permission denied' in p.stderr,p.stderr
            assert guide.read_bytes()==original
        finally:
            first.chmod(0o600)
        p=subprocess.run(receipt_args,env=receipt_env,text=True,capture_output=True)
        assert p.returncode==0,p.stderr
        receipt=json.loads(p.stdout)['receipt']
        entry=next(entry for entry in receipt['modInventory'] if entry['path']=='a-state.txt')
        assert entry['sha256']==hashlib.sha256(first.read_bytes()).hexdigest(),receipt
        assert receipt['selected']==['independent'],receipt
        assert guide.read_bytes()==original and list(game.iterdir())==[]
        print('PASS unreadable receipt-only file aborts export; repaired stable retry produces current content hash')
