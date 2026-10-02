#!/usr/bin/env python3
"""Bounded step-3 native desktop acceptance; no Rust tests or synthetic messages.

DOWNLOADS_GUI_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn \
  python3 docs/parity/downloads-gui-step3/probe.py

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
ROOT = Path(tempfile.mkdtemp(prefix='modsync-step3-'))
DISPLAY = os.environ.get('MODSYNC_DISPLAY', ':94')
MAP = json.loads(Path(os.environ['MODSYNC_UI_MAP']).read_text()) if os.environ.get('MODSYNC_UI_MAP') else {}
CALIBRATED = {'Choose Beta at 600x400': [60, 239], 'Apply choices at 600x400': [126, 277],
              'Dismiss second editor': [53, 298],
              'Dismiss second malformed choices item': [581, 443],
              'Dismiss repaired second': [392, 497]}
CASE_NAMES = ['record']
CASES = set(os.environ.get('MODSYNC_CASES', ','.join(CASE_NAMES)).split(','))
assert CASES and CASES <= set(CASE_NAMES), 'Unknown native scenario'
EVENTS, CHECKS, REQUESTS = [], [], []
PENDING = []
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
        hits = [(96, 280)]
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


# A simple valid archive keeps FOMOD dialogs out of the Save/record comparison.
for name in ['/one.zip', '/two.zip']:
    data = io.BytesIO()
    with zipfile.ZipFile(data, 'w') as z: z.writestr('payload/file.txt', name)
    BODIES[name] = data.getvalue()

try:
    assert os.environ.get('DOWNLOADS_GUI_PARENT_NETNS') not in (None, os.readlink('/proc/self/ns/net')), 'Requires unshare -Urn'
    run(['ip', 'link', 'set', 'lo', 'up']); assert not run(['ip', 'route']).strip()
    source = sorted((REPO / 'src').rglob('*.rs')) + [REPO / 'Cargo.toml', REPO / 'Cargo.lock']
    assert all(p.stat().st_mtime_ns < BIN.stat().st_mtime_ns for p in source), 'Rebuild production binary'
    IDENTITY = dict(binary=str(BIN), binary_sha256=digest(BIN), probe_sha256=digest(__file__),
                    source={str(p.relative_to(REPO)): digest(p) for p in source},
                    namespaces={'parent_network': os.environ['DOWNLOADS_GUI_PARENT_NETNS'],
                                'network': os.readlink('/proc/self/ns/net'), 'user': os.readlink('/proc/self/ns/user')},
                    uname=run(['uname', '-a']).strip())
    event('identity', **IDENTITY)
    CERT, KEY = ROOT / 'ca.pem', ROOT / 'ca.key'
    run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(KEY), '-out', str(CERT),
         '-days', '1', '-subj', '/CN=Example CA', '-addext', 'basicConstraints=critical,CA:TRUE'])
    leaf, leafkey, csr, ext = [ROOT / n for n in ['leaf.pem', 'leaf.key', 'leaf.csr', 'leaf.ext']]
    ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
    run(['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(leafkey), '-out', str(csr), '-subj', '/CN=Example Server'])
    run(['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(CERT), '-CAkey', str(KEY), '-CAcreateserial', '-out', str(leaf), '-days', '1', '-extfile', str(ext)])
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Origin)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); tls.load_cert_chain(leaf, leafkey)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    BASE = f'https://127.0.0.1:{server.server_port}'
    assert not Path('/tmp/.X11-unix/X' + DISPLAY.lstrip(':').split('.')[0]).exists()
    XVFB = sp.Popen(['Xvfb', DISPLAY, '-screen', '0', '1280x800x24', '-nolisten', 'tcp', '-noreset'], stdout=sp.DEVNULL, stderr=sp.DEVNULL)
    time.sleep(1); assert XVFB.poll() is None
    case = fixture('record', SETTINGS)
    # The second component is unselected and marked Files=false; the first is the
    # only acquisition candidate. It is also a valid base-install instruction.
    guide = json.loads((case / 'guide.json').read_text())
    guide['Components'][0]['ResourceRegistry'][BASE + '/one.zip']['Files'] = {}
    guide['Components'][1]['IsSelected'] = False
    guide['Components'][1]['ResourceRegistry'][BASE + '/two.zip']['Files']['two.zip'] = False
    (case / 'guide.json').write_text(json.dumps(guide, indent=2))
    original_guide, original_game = digest(case / 'guide.json'), manifest(case / 'game')
    wid = launch(case, 'record'); downloads()
    click('Fetch Downloads')
    await_text('every planned archive is ready')
    screen('fetch-terminal-unsaved')
    check('selected_archive_only', sorted(p.name for p in (case / 'mods').glob('*.zip')), ['one.zip'])
    check('selected_archive_exact', digest(case / 'mods/one.zip'), hashlib.sha256(BODIES['/one.zip']).hexdigest())
    check('authored_guide_not_saved', digest(case / 'guide.json'), original_guide)
    check('game_unchanged_after_fetch', manifest(case / 'game'), original_game)
    click('Guide editor')
    await_text('Save guide as')
    screen('guide-editor-save-control')
    click('Save guide as')
    time.sleep(2)
    screen('native-save-dialog')
    event('save_dialog_windows', windows=x('search', '--onlyvisible', '--name', '.*'))
    # GTK/rfd Save dialog supports Ctrl+L for an absolute destination.
    save_path = case / 'saved-guide.json'
    x('key', 'ctrl+l'); x('type', '--clearmodifiers', str(save_path)); x('key', 'Return')
    time.sleep(2)
    screen('after-save-dialog')
    if save_path.exists():
        saved = json.loads(save_path.read_text())
        first, second = saved if isinstance(saved, list) else saved['Components']
        check('saved_first_downloaded', first['ResourceRegistry'][BASE + '/one.zip']['Files']['one.zip'], True)
        check('saved_second_false_preserved', second['ResourceRegistry'][BASE + '/two.zip']['Files']['two.zip'], False)
        check('saved_selection_preserved', [c['IsSelected'] for c in (saved if isinstance(saved, list) else saved['Components'])], [True, False])
    else:
        PENDING.append('Native rfd Save guide dialog did not surface on private Xvfb; no saved-guide or GUI record-diff claim.')
        event('save_dialog_unreachable', windows=x('search', '--onlyvisible', '--name', '.*'))
    check('game_unchanged_after_save', manifest(case / 'game'), original_game)
    close()
    # Same guide and settings through the production CLI, with a fresh archive root.
    cli_mods = case / 'cli-mods'; cli_mods.mkdir()
    args = [str(BIN), 'fetch-downloads', '--input', str(case / 'guide.json'),
            '--source-dir', str(cli_mods), '--settings-dir', str(case / 'settings'),
            '--extra-ca-file', str(CERT), '--peer-mode', 'off', '--progress-json', '--record']
    result = sp.run(args, env=ENV, capture_output=True, text=True, timeout=30)
    (OUT / 'logs/cli.stdout').write_text(result.stdout)
    (OUT / 'logs/cli.stderr').write_text(result.stderr)
    event('cli', argv=args, exit=result.returncode)
    check('cli_exit', result.returncode, 0)
    check('cli_archive_exact', digest(cli_mods / 'one.zip'), digest(case / 'mods/one.zip'))
    check('cli_unselected_absent', (cli_mods / 'two.zip').exists(), False)
    cli_guide = json.loads((case / 'guide.json').read_text())
    cli_components = cli_guide if isinstance(cli_guide, list) else cli_guide['Components']
    check('cli_record_first_downloaded',
          cli_components[0]['ResourceRegistry'][BASE + '/one.zip']['Files']['one.zip'], True)
    check('cli_record_false_preserved',
          cli_components[1]['ResourceRegistry'][BASE + '/two.zip']['Files']['two.zip'], False)
    check('cli_record_selection_preserved',
          [c['IsSelected'] for c in cli_components], [True, False])
    check('cli_record_changed_only_after_explicit_record', digest(case / 'guide.json') != original_guide)
    # The final CLI summary exposes the shared record operation.
    objs = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    event('cli_summary', summary=objs[-1] if objs else None)
    check('cli_summary_present', bool(objs))
    check('cli_record_summary_present', objs[-1]['summary']['recordSummary'] is not None)
    COMPLETED = True; event('scenario_complete', scenario='record')
except BaseException as error:
    event('error', type=type(error).__name__, message=str(error))
    if GUI and GUI.poll() is None:
        try: screen('failure')
        except Exception: pass
    raise
finally:
    close()
    if XVFB: XVFB.terminate(); XVFB.wait(10)
    (OUT / 'receipt.json').write_text(json.dumps({'identity': IDENTITY, 'completed': COMPLETED,
        'scenarios': sorted(CASES), 'checks': CHECKS, 'events': EVENTS, 'requests': REQUESTS,
        'pending': PENDING, 'accepted': False, 'fixture_root': str(ROOT)}, indent=2))
    retained = OUT / 'fixture-state'; retained.mkdir(exist_ok=True)
    for case_name in CASE_NAMES:
        if (ROOT / case_name).exists(): shutil.copytree(ROOT / case_name, retained / case_name, dirs_exist_ok=True)
