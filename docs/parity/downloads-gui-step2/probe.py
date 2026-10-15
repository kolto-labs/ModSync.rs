#!/usr/bin/env python3
"""Native step-2 acceptance; no builds, Rust tests, or synthetic GUI messages.

DOWNLOADS_GUI_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn \
  python3 docs/parity/downloads-gui-step2/probe.py

Requires the production binary, Xvfb, xdotool, ImageMagick, tesseract, openssl, ip.
MODSYNC_BINARY, MODSYNC_PROOF_OUT and MODSYNC_DISPLAY override paths/display.
MODSYNC_UI_MAP optionally names a JSON map {action: [x,y]} for calibrated clicks;
otherwise OCR locates actual visible labels. All actions and OCR are retained.
"""
import csv
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
import ssl
import subprocess as sp
import tempfile
import threading
import time
import zipfile

REPO = Path(__file__).resolve().parents[3]
BIN = Path(os.environ.get('MODSYNC_BINARY', str(REPO / 'target/debug/modsync'))).resolve()
OUT = Path(os.environ.get('MODSYNC_PROOF_OUT', str(Path(__file__).parent))).resolve()
ROOT = Path(tempfile.mkdtemp(prefix='modsync-step2-'))
DISPLAY = os.environ.get('MODSYNC_DISPLAY', ':94')
MAP = json.loads(Path(os.environ['MODSYNC_UI_MAP']).read_text()) if os.environ.get('MODSYNC_UI_MAP') else {}
# Recalibrated 2026-10-03 (runtime-revalidation-2026-10-03b.md): the completed-sharing
# strip and FOMOD fallback warnings moved these controls; OCR also matched prose.
CALIBRATED = {'Choose Beta at 600x400': [60, 278], 'Apply choices at 600x400': [125, 316],
              'Dismiss second editor': [52, 337], 'Peer mode Off': [1036, 476],
              'Navigate to Validate before Fetch': [1218, 740],
              'Dismiss second malformed choices item': [581, 443],
              'Dismiss repaired second': [392, 497]}
CASE_NAMES = ['serial', 'suppressed', 'broken-choices', 'vanished-archive', 'malformed']
CASES = set(os.environ.get('MODSYNC_CASES', ','.join(CASE_NAMES)).split(','))
assert CASES and CASES <= set(CASE_NAMES), 'Unknown native scenario'
EVENTS, CHECKS, REQUESTS = [], [], []
PENDING = ['Stale editor outcome after an actual guide/root replacement: source review only until a safe UI route is reachable.',
           'Completed-base choice preservation: requires an established completed-base fixture.']
IDENTITY = {}
COMPLETED = False
GUI = None
WID = None
XVFB = None
T0 = time.monotonic()
OUT.mkdir(parents=True, exist_ok=True)
(OUT / 'screenshots').mkdir(exist_ok=True)
(OUT / 'logs').mkdir(exist_ok=True)
ENV = {'PATH': '/usr/bin:/bin', 'HOME': str(ROOT / 'home'), 'USER': 'step2-fixture', 'DISPLAY': DISPLAY,
       'LANG': 'C.UTF-8', 'LIBGL_ALWAYS_SOFTWARE': '1',
       **{k: str(ROOT / v) for k, v in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                                      ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')]}}
for name in ['HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_RUNTIME_DIR']:
    Path(ENV[name]).mkdir(mode=0o700)


def run(args, **kw):
    return sp.run(args, check=True, capture_output=True, text=True, **kw).stdout


def digest(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def manifest(p):
    return {str(f.relative_to(p)): digest(f) for f in sorted(p.rglob('*')) if f.is_file()}


def event(kind, **kw):
    row = dict(t=round(time.monotonic() - T0, 3), kind=kind, **kw)
    EVENTS.append(row)
    print(json.dumps(row), flush=True)


def check(name, actual, expected=True):
    row = dict(name=name, actual=actual, expected=expected, passed=actual == expected)
    CHECKS.append(row)
    event('assertion', **row)
    if not row['passed']:
        raise AssertionError(name)


def x(*args):
    return run(['xdotool', *map(str, args)], env=ENV).strip()


def screen(label):
    path = OUT / 'screenshots' / f'{len(EVENTS):04}-{label}.png'
    run(['import', '-window', 'root', str(path)], env=ENV)
    geometry = dict(line.split('=', 1) for line in x('getwindowgeometry', '--shell', WID).splitlines() if '=' in line)
    ocr = ROOT / 'ocr.png'
    run(['convert', str(path), '-crop', f"{geometry['WIDTH']}x{geometry['HEIGHT']}+0+0",
         '-colorspace', 'Gray', '-resize', '200%', str(ocr)])
    tsv = run(['tesseract', str(ocr), 'stdout', '--psm', '11', 'tsv'],
              env={**os.environ, 'OMP_THREAD_LIMIT': '1'}, timeout=30)
    words = [r for r in csv.DictReader(io.StringIO(tsv), delimiter='\t', quoting=csv.QUOTE_NONE) if r['text'].strip()]
    for word in words:
        for key in ['left', 'top', 'width', 'height']:
            word[key] = str(int(word[key]) // 2)
    text = ' '.join(r['text'] for r in words)
    event('screen', file=str(path.relative_to(OUT)), sha256=digest(path), text=text)
    return words, text


def locate(words, label):
    wanted = label.casefold().split()
    tokens = [w['text'].casefold().strip('.,:;()[]') for w in words]
    hits = []
    for i in range(len(tokens) - len(wanted) + 1):
        if tokens[i:i + len(wanted)] == wanted:
            group = words[i:i + len(wanted)]
            left = min(int(w['left']) for w in group)
            top = min(int(w['top']) for w in group)
            right = max(int(w['left']) + int(w['width']) for w in group)
            bottom = max(int(w['top']) + int(w['height']) for w in group)
            hits.append(((left + right) // 2, (top + bottom) // 2))
    return hits


def click(label, action=None, last=False):
    action = action or label
    words, _ = screen('before-' + action.lower().replace(' ', '-'))
    hits = locate(words, label)
    if label == 'Fetch Downloads':
        hits = [p for p in hits if p[0] > 960 and p[1] > 350]
        last = True
    if label == 'Run Validation':
        # Native 1280x800 validation button, visually calibrated in final2's
        # validation-before-fetch image. OCR repeatedly corrupts its icon/text.
        hits = [(96, 395)]
    point = MAP.get(action) or CALIBRATED.get(action) or (hits[-1 if last else 0] if hits else None)
    # Calibrated from the retained 1280x800 Welcome/Workspace screenshots. Native
    # OCR can omit the focused grey Next button; navigation actions run at this
    # size, and subsequent page assertions detect a missed or disabled click.
    footer_fallback = point is None and label == 'Next'
    if footer_fallback:
        path = OUT / 'screenshots' / EVENTS[-1]['file'].split('/')[-1]
        geometry = dict(line.split('=', 1) for line in x('getwindowgeometry', '--shell', WID).splitlines() if '=' in line)
        width, height = int(geometry['WIDTH']), int(geometry['HEIGHT'])
        pixels = run(['convert', str(path), '-crop', f'1x{height}+{width - 40}+0', 'txt:-'])
        runs = []
        for line in pixels.splitlines()[1:]:
            y = int(line.split(':', 1)[0].split(',')[1])
            color = line.split()[2]
            if runs and runs[-1][2] == color and runs[-1][1] == y - 1:
                runs[-1][1] = y
            else:
                runs.append([y, y, color])
        eligible = [r for r in runs if r[2] != '#FFFFFF' and 26 <= r[1] - r[0] + 1 <= 40]
        assert eligible, 'Native footer button rectangle could not be identified'
        point = [width - 62, sum(eligible[-1][:2]) // 2]
    if not point:
        raise AssertionError(f'Cannot locate {label!r}; screenshot retained. Supply a calibrated MODSYNC_UI_MAP entry {action!r}.')
    x('mousemove', *point)
    x('click', 1)
    event('click', action=action, label=label, position=point, calibrated=action in MAP or action in CALIBRATED or footer_fallback or label == 'Run Validation')
    time.sleep(.7)


def await_text(label, timeout=30):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        _, text = screen('wait')
        if label.casefold() in text.casefold():
            return text
        time.sleep(.8)
    raise AssertionError(f'Visible text did not appear: {label}')


def await_state(predicate, name, timeout=30):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if predicate():
            event('observed', name=name)
            return
        time.sleep(.1)
    raise AssertionError(f'State did not settle: {name}')


def close():
    global GUI
    if GUI and GUI.poll() is None:
        GUI.terminate()
        try:
            GUI.wait(10)
        except sp.TimeoutExpired:
            GUI.kill()
            GUI.wait()
    if GUI:
        event('close', exit=GUI.returncode)
    GUI = None


def launch(case, label):
    global GUI, WID
    await_state(lambda: sp.run(['xdotool', 'getdisplaygeometry'], env=ENV, capture_output=True).returncode == 0,
                'private X server accepts connections')
    args = [str(BIN), 'gui', '--settings-dir', str(case / 'settings'), '--input', str(case / 'guide.json'),
            '--source-dir', str(case / 'mods'), '--game-dir', str(case / 'game'), '--extra-ca-file', str(CERT)]
    with open(OUT / 'logs' / f'{label}.log', 'w') as log:
        GUI = sp.Popen(args, env=ENV, stdout=log, stderr=sp.STDOUT)
    event('launch', argv=args)
    for _ in range(100):
        found = sp.run(['xdotool', 'search', '--name', '^ModSync.rs$'], env=ENV, capture_output=True, text=True)
        if found.returncode == 0 and found.stdout.strip():
            wid = found.stdout.split()[0]
            WID = wid
            x('windowmove', wid, 0, 0)
            x('windowsize', wid, 1280, 800)
            time.sleep(2)
            return wid
        if GUI.poll() is not None:
            raise AssertionError(f'GUI exited: {label}')
        time.sleep(.2)
    raise AssertionError('No production GUI window')


def downloads():
    # A preloaded guide begins on Welcome; the production wizard has four reading
    # pages before DownloadsExplain. Label-driven clicks are logged/screenshotted.
    for index in range(4):
        click('Next', f'Next to downloads {index}', last=True)
    await_text('Fetch Downloads')


def archive(name):
    xml = ('<config><moduleName>' + name + '</moduleName><installSteps><installStep name="Choose">'
           '<optionalFileGroups><group name="Variant" type="SelectExactlyOne"><plugins>'
           '<plugin name="Alpha"><description>First variant</description><files>'
           '<file source="payload/a.txt" destination="Override/example.txt"/></files>'
           '<typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
           '<plugin name="Beta"><description>Second variant</description><files>'
           '<file source="payload/b.txt" destination="Override/example.txt"/></files>'
           '<typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
           '</plugins></group></optionalFileGroups></installStep></installSteps></config>')
    data = io.BytesIO()
    with zipfile.ZipFile(data, 'w') as z:
        z.writestr('fomod/ModuleConfig.xml', xml)
        z.writestr('payload/a.txt', name + ' alpha')
        z.writestr('payload/b.txt', name + ' beta')
    return data.getvalue()


BODIES = {'/one.zip': archive('Example One'), '/two.zip': archive('Example Two')}


class Origin(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = BODIES.get(self.path)
        REQUESTS.append(dict(path=self.path, status=200 if body else 404, bytes=len(body or b'')))
        self.send_response(200 if body else 404)
        self.send_header('Content-Length', str(len(body or b'')))
        self.send_header('Content-Type', 'application/zip')
        self.end_headers()
        self.wfile.write(body or b'')

    def log_message(self, *args):
        pass


def fixture(name, settings, statuses=None):
    case = ROOT / name
    for directory in ['settings', 'mods', 'game/override']:
        (case / directory).mkdir(parents=True)
    (case / 'game/swkotor.exe').write_bytes(b'game detection fixture')
    (case / 'game/override/original.txt').write_bytes(b'original game content')
    (case / 'settings/settings.json').write_text(json.dumps(settings))
    components = []
    for i, filename in enumerate(['one.zip', 'two.zip']):
        metadata = {'Files': {filename: True}}
        if statuses:
            metadata['HandlerMetadata'] = {'fomodPromptStatus': {filename: statuses[i]}}
        components.append({'Guid': f'0000000{i}-0000-4000-8000-000000000000', 'Name': f'Example Component {i}',
                           'IsSelected': True, 'Instructions': [{'Action': 'Extract', 'Source': [f'<<modDirectory>>/{filename}']}],
                           'ResourceRegistry': {BASE + '/' + filename: metadata}})
    (case / 'guide.json').write_text(json.dumps({'Components': components}, indent=2))
    return case


SETTINGS = {'peerSharing': False, 'PeerSharing': True, 'peerMode': 'race', 'PeerMode': 'fallback',
            'peerProxyListUrl': None, 'peerTrackerListUrl': [], 'peerTrackers': [], 'peerExtraPeers': [],
            'peerProxies': [], 'telemetryOptIn': False, 'nexusModsApiKey': 'synthetic-preserved-secret',
            'fixtureUnknownKey': {'nested': [1, 'preserve']}, 'peerUploadKiBps': 1}

try:
    assert os.environ.get('DOWNLOADS_GUI_PARENT_NETNS') not in (None, os.readlink('/proc/self/ns/net')), 'Requires unshare -Urn'
    run(['ip', 'link', 'set', 'lo', 'up'])
    assert not run(['ip', 'route']).strip(), 'Namespace must have no routes'
    for command in ['Xvfb', 'xdotool', 'import', 'tesseract', 'openssl']:
        assert shutil.which(command), f'Missing {command}'
    source = sorted((REPO / 'src').rglob('*.rs')) + [REPO / 'Cargo.toml', REPO / 'Cargo.lock']
    assert all(p.stat().st_mtime_ns < BIN.stat().st_mtime_ns for p in source), 'Rebuild production binary after source changes'
    IDENTITY = dict(binary=str(BIN), binary_sha256=digest(BIN), probe_sha256=digest(__file__),
                    source={str(p.relative_to(REPO)): digest(p) for p in source},
                    namespaces={'parent_network': os.environ['DOWNLOADS_GUI_PARENT_NETNS'],
                                'network': os.readlink('/proc/self/ns/net'),
                                'user': os.readlink('/proc/self/ns/user')}, uname=run(['uname', '-a']).strip())
    event('identity', **IDENTITY)
    CERT, KEY = ROOT / 'ca.pem', ROOT / 'ca.key'
    run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(KEY), '-out', str(CERT),
         '-days', '1', '-subj', '/CN=Example CA', '-addext', 'basicConstraints=critical,CA:TRUE'])
    leaf, leafkey, csr, ext = [ROOT / n for n in ['leaf.pem', 'leaf.key', 'leaf.csr', 'leaf.ext']]
    ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
    run(['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(leafkey), '-out', str(csr), '-subj', '/CN=Example Server'])
    run(['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(CERT), '-CAkey', str(KEY), '-CAcreateserial', '-out', str(leaf), '-days', '1', '-extfile', str(ext)])
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Origin)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(leaf, leafkey)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    BASE = f'https://127.0.0.1:{server.server_port}'
    assert not Path('/tmp/.X11-unix/X' + DISPLAY.lstrip(':').split('.')[0]).exists(), 'Selected display already exists; choose a private MODSYNC_DISPLAY'
    XVFB = sp.Popen(['Xvfb', DISPLAY, '-screen', '0', '1280x800x24', '-nolisten', 'tcp', '-noreset'], stdout=sp.DEVNULL, stderr=sp.DEVNULL)
    time.sleep(1)
    assert XVFB.poll() is None, 'Private Xvfb failed to start'
    if 'serial' in CASES:
        case = fixture('serial', SETTINGS)
        original_game, original_guide = manifest(case / 'game'), digest(case / 'guide.json')
        wid = launch(case, 'serial')
        downloads()
        # The final labels below are intentionally explicit: if copy/layout changes,
        # calibration must locate the genuine control rather than invent an event.
        before_open = digest(case / 'settings/settings.json')
        click('Peer-sharing settings')
        time.sleep(1)
        check('opening_peer_controls_preserves_authored_low_cap_and_all_keys', digest(case / 'settings/settings.json'), before_open)
        click('Off', 'Peer mode Off', last=True)
        await_state(lambda: json.loads((case / 'settings/settings.json').read_text()).get('peerMode') == 'off', 'asynchronous peer settings save')
        stored = json.loads((case / 'settings/settings.json').read_text())
        check('peer_mode_key_preserved', any(k.lower() == 'peermode' for k in stored))
        check('all_case_variants_mode_off', [v for k, v in stored.items() if k.lower() == 'peermode'], ['off'] * sum(k.lower() == 'peermode' for k in stored))
        for key in ['fixtureUnknownKey', 'nexusModsApiKey', 'peerUploadKiBps', 'peerSharing', 'PeerSharing']:
            check('unchanged_' + key, stored.get(key), SETTINGS[key])
        # Contradictory case aliases read fail closed (persistent-sharing-plan.md), so the
        # checkbox starts unchecked: switch it on, then off, through the real control.
        click('Sharing enabled', 'Switch sharing On')
        await_state(lambda: json.loads((case / 'settings/settings.json').read_text()).get('peerSharing') is True
                    and 'PeerSharing' not in json.loads((case / 'settings/settings.json').read_text()), 'asynchronous sharing On alias normalization')
        click('Sharing enabled', 'Switch sharing Off')
        await_state(lambda: json.loads((case / 'settings/settings.json').read_text()).get('peerSharing') is False
                    and 'PeerSharing' not in json.loads((case / 'settings/settings.json').read_text()), 'asynchronous sharing Off alias normalization')
        stored = json.loads((case / 'settings/settings.json').read_text())
        check('sharing_off_removes_case_override', {k: v for k, v in stored.items() if k.lower() == 'peersharing'}, {'peerSharing': False})
        check('sharing_off_preserves_secret', stored['nexusModsApiKey'], SETTINGS['nexusModsApiKey'])
        check('sharing_off_preserves_unknown', stored['fixtureUnknownKey'], SETTINGS['fixtureUnknownKey'])
        saved = digest(case / 'settings/settings.json')
        close()
        wid = launch(case, 'restart')
        downloads()
        check('restart_settings_unchanged', digest(case / 'settings/settings.json'), saved)
        screen('settings-restart')
        click('Next', 'Navigate to Validate before Fetch', last=True)
        gate_words, _ = screen('validation-before-fetch')
        validation_hits = [(96, 395)]  # visually calibrated actual button, not prose
        event('calibration', control='Run Validation', position=validation_hits[0],
              source='Visually inspected 1280x800 validation-before-fetch screenshot; later positive validation uses identical coordinates')
        validation_point = validation_hits[0]
        click('Fetch Downloads')
        await_text('Configure one.zip')
        first = screen('first-editor')[1]
        check('first_editor_only', 'Configure two.zip' in first, False)
        x('mousemove', *validation_point)
        x('click', 1)
        event('click', action='Attempt Run Validation with unresolved FOMOD', position=validation_point,
              calibrated_from='validation-before-fetch screenshot')
        time.sleep(1)
        unresolved = screen('validation-blocked-by-unresolved-editor')[1]
        check('unresolved_editor_blocks_validation', 'configure one.zip' in unresolved.casefold() and 'validation failed' not in unresolved.casefold() and 'validation complete' not in unresolved.casefold())
        check('both_archives_downloaded', sorted(p.name for p in (case / 'mods').iterdir()), ['one.zip', 'two.zip'])
        for filename in ['one.zip', 'two.zip']:
            check('archive_bytes_' + filename, digest(case / 'mods' / filename), hashlib.sha256(BODIES['/' + filename]).hexdigest())
        x('windowsize', wid, 600, 400)
        time.sleep(1)
        screen('first-editor-small')
        click('Beta', 'Choose Beta at 600x400')
        click('Use these choices', 'Apply choices at 600x400')
        await_text('Configure two.zip')
        check('second_editor_no_first', 'Configure one.zip' in screen('second-editor')[1], False)
        click('Cancel', 'Dismiss second editor')
        time.sleep(2)
        check('no_repaint_relaunch', 'Configure two.zip' in screen('dismissed')[1], False)
        x('windowsize', wid, 1280, 800)
        time.sleep(1)
        screen('fresh-validation-required')
        click('Next', 'Try advancing without fresh validation', last=True)
        check('cannot_advance_before_fresh_validation', 'Run Validation' in screen('fresh-validation-next-blocked')[1])
        click('Run Validation', 'Validate after queue resolved')
        await_text('Validation failed')
        check('resolved_queue_allows_real_validation', 'validation failed' in screen('validation-ran-after-dismiss')[1].casefold())
        click('Next', 'Try advancing after failed validation', last=True)
        blocked_text = screen('failed-validation-next-blocked')[1].casefold()
        check('failed_validation_cannot_advance', 'validation failed' in blocked_text and 'step 6 of 10' in blocked_text)
        for width, height in [(600, 400), (1280, 800)]:
            x('windowsize', wid, width, height)
            time.sleep(1)
            screen(f'validation-{width}x{height}')
            if width == 600:
                click('Back', 'Back at 600x400')
                await_text('Step 5 of 10')
                check('small_window_back_reaches_downloads', 'step 5 of 10' in screen('small-navigation-back')[1].casefold())
                click('Next', 'Next at 600x400', last=True)
                await_text('Step 6 of 10')
                check('small_window_navigation_returns_to_validation', 'step 6 of 10' in screen('small-navigation-return')[1].casefold())
        check('game_unchanged_acquisition_and_choices', manifest(case / 'game'), original_game)
        check('guide_not_autosaved', digest(case / 'guide.json'), original_guide)
        check('no_install_receipts', [p.name for p in (case / 'game').rglob('*') if 'receipt' in p.name.lower()], [])
        # Gate semantics need positive runtime evidence, beyond OCR/screenshot claims.
        PENDING.append('Direct Install-button interaction while a FOMOD dialog is unresolved is source-reviewed only; the native flow checks blocked validation and wizard advancement, then actual validation after resolution.')
        close()
        event("scenario_complete", scenario='serial')
    if 'suppressed' in CASES:
        suppressed = fixture('suppressed', SETTINGS, ['configured', 'warned'])
        wid = launch(suppressed, 'suppressed')
        downloads()
        click('Fetch Downloads')
        deadline = time.monotonic() + 30
        while len(list((suppressed / 'mods').glob('*.zip'))) < 2 and time.monotonic() < deadline:
            time.sleep(.2)
        time.sleep(3)
        check('suppressed_archives_present', len(list((suppressed / 'mods').glob('*.zip'))), 2)
        check('configured_warned_no_editor', 'Configure one.zip' in screen('suppressed')[1] or 'Configure two.zip' in screen('suppressed-two')[1], False)
        close()
        event("scenario_complete", scenario='suppressed')
    if 'broken-choices' in CASES:
        broken = fixture('broken-choices', SETTINGS)
        broken_guide = json.loads((broken / 'guide.json').read_text())
        broken_guide['Components'][0]['FomodChoices'] = 'invalid saved choices fixture'
        (broken / 'guide.json').write_text(json.dumps(broken_guide))
        broken_game = manifest(broken / 'game')
        broken_source = digest(broken / 'guide.json')
        wid = launch(broken, 'broken-choices')
        downloads()
        click('Fetch Downloads', 'Fetch malformed saved choices')
        await_text('FOMOD choices need attention')
        failure = screen('broken-choices-blocker')[1]
        check('malformed_choices_blocker_explains_gate', 'validation and installation are blocked' in failure.casefold())
        check('malformed_choices_no_second_editor', 'Configure two.zip' in failure, False)
        before_retry = len(REQUESTS)
        click('Retry FOMOD choices')
        await_text('FOMOD choices need attention')
        check('retry_same_failure_no_second_editor', 'Configure two.zip' in screen('broken-choices-retry')[1], False)
        check('retry_does_not_redownload', len(REQUESTS), before_retry)
        check('retry_preserves_authored_choices', digest(broken / 'guide.json'), broken_source)
        check('malformed_choices_blocker_names_parse_failure', 'cannot be parsed' in failure.casefold())
        click('Dismiss FOMOD prompt')
        # Queue items bind their own consumers (bound_fomod_followups), so only the
        # malformed component blocks. two.zip's consumer has no saved choices and gets
        # its ordinary editor after the explicit dismissal; it is dismissed too.
        await_text('Configure two.zip')
        second = screen('broken-choices-second-item')[1]
        check('second_archive_gets_ordinary_editor', 'FOMOD choices need attention' in second, False)
        click('Cancel', 'Dismiss second archive editor')
        time.sleep(2)
        check('malformed_choices_no_automatic_relaunch', 'FOMOD choices need attention' in screen('broken-choices-dismissed')[1], False)
        check('malformed_choices_no_game_mutation', manifest(broken / 'game'), broken_game)
        check('malformed_choices_guide_preserved', digest(broken / 'guide.json'), broken_source)
        click('Next', 'Navigate after explicit malformed choices dismissals', last=True)
        click('Run Validation', 'Validate after malformed choices dismissals')
        await_text('Validation failed')
        check('explicit_error_dismissals_release_validation', 'validation failed' in screen('broken-choices-validation')[1].casefold())
        close()
        event("scenario_complete", scenario='broken-choices')
    if 'vanished-archive' in CASES:
        vanished = fixture('vanished-archive', SETTINGS)
        wid = launch(vanished, 'vanished-archive')
        downloads()
        click('Fetch Downloads', 'Fetch archive-disappearance fixture')
        await_text('Configure one.zip')
        (vanished / 'mods/two.zip').unlink()
        event('fixture_mutation', target='vanished-archive/mods/two.zip', reason='Archive disappears after shared inspection, before queued editor inspection')
        click('Cancel', 'Dismiss first before missing second')
        await_text('Configure two.zip')
        time.sleep(2)
        missing_text = screen('inspection-error')[1]
        check('editor_inspection_failed', any(term in missing_text.casefold() for term in ['no such file', 'does not exist', 'not found', 'cannot open', 'opening archive']))
        click('Cancel', 'Cancel inspection error without Outcome')
        time.sleep(2)
        check('inspection_error_closed', 'Configure two.zip' in screen('inspection-error-closed')[1], False)
        click('Next', 'Navigate after inspection Cancel', last=True)
        click('Run Validation', 'Validate after inspection Cancel')
        await_text('Validation failed')
        check('inspection_cancel_releases_readiness', 'validation failed' in screen('inspection-cancel-validation')[1].casefold())
        close()
        event("scenario_complete", scenario='vanished-archive')
    if 'malformed' in CASES:
        malformed = fixture('malformed', SETTINGS)
        raw = '{"fixtureUnknownKey": broken'
        (malformed / 'settings/settings.json').write_text(raw)
        wid = launch(malformed, 'malformed')
        downloads()
        await_text('Peer settings could not be read')
        before_requests = len(REQUESTS)
        before_files = manifest(malformed)
        click('Fetch Downloads', 'Fetch with malformed settings')
        await_text('Peer settings')
        time.sleep(2)
        check('malformed_fetch_no_origin_requests', len(REQUESTS), before_requests)
        check('malformed_fetch_no_file_mutation', manifest(malformed), before_files)
        check('malformed_not_overwritten', (malformed / 'settings/settings.json').read_text(), raw)
        screen('malformed-save-error')
        (malformed / 'settings/settings.json').write_text(json.dumps(SETTINGS))
        event('fixture_repair', target='malformed/settings/settings.json', reason='Exercise explicit production Refresh after operator repairs malformed JSON')
        click('Refresh peer settings')
        await_text('Peer-sharing settings')
        click('Fetch Downloads', 'Fetch after explicit settings refresh')
        await_text('Configure one.zip')
        check('repaired_settings_allow_real_acquisition', len(REQUESTS) > before_requests)
        click('Cancel', 'Dismiss repaired first')
        await_text('Configure two.zip')
        click('Cancel', 'Dismiss repaired second')
        check('settings_repair_does_not_install', manifest(malformed / 'game'),
              {k.removeprefix('game/'): v for k, v in before_files.items() if k.startswith('game/')})
        event("scenario_complete", scenario='malformed')
    COMPLETED = True
except BaseException as error:
    event('error', type=type(error).__name__, message=str(error))
    if GUI and GUI.poll() is None:
        try:
            screen('failure')
        except Exception:
            pass
    raise
finally:
    close()
    if XVFB:
        XVFB.terminate()
        XVFB.wait(10)
    (OUT / 'receipt.json').write_text(json.dumps({'identity': IDENTITY, 'completed': COMPLETED, 'scenarios': sorted(CASES), 'checks': CHECKS, 'events': EVENTS, 'requests': REQUESTS,
        'pending': PENDING, 'accepted': False, 'fixture_root': str(ROOT),
        'note': 'Acceptance requires review of pending cases and screenshots; an exit code alone does not prove gates.'}, indent=2))
    # Preserve controlled fixture state for a reviewer, without copying caches.
    retained = OUT / 'fixture-state'
    retained.mkdir(exist_ok=True)
    for case in ['serial', 'suppressed', 'broken-choices', 'vanished-archive', 'malformed']:
        if (ROOT / case).exists():
            shutil.copytree(ROOT / case, retained / case, dirs_exist_ok=True)
