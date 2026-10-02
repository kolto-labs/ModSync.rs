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
CASE_NAMES = ['peer-win']
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


import importlib.util
import socket
import sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('accepted_fixture', REPO / 'docs/parity/fetch-downloads/run_acceptance.py')
f = importlib.util.module_from_spec(spec); spec.loader.exec_module(f)
f.BIN = BIN; f.OUT = OUT; f.ROOT = ROOT; f.ENV = ENV
ORIGIN_BODY_COMPLETE = []
ORIGIN_DISCONNECTED = []

def peer_origin_get(self):
    body = PAYLOAD if self.path == '/peer-win' else b''
    REQUESTS.append(dict(path=self.path, status=200 if body else 404, bytes=len(body)))
    self.send_response(200 if body else 404)
    self.send_header('Content-Type', 'application/zip')
    self.send_header('Content-Length', str(len(body)))
    self.end_headers()
    try:
        for offset in range(0, len(body), 8192):
            self.wfile.write(body[offset:offset + 8192]); self.wfile.flush()
            time.sleep(.15)
        ORIGIN_BODY_COMPLETE.append(self.path); event('origin_body_complete', path=self.path)
    except (BrokenPipeError, ConnectionResetError, ssl.SSLError):
        ORIGIN_DISCONNECTED.append(self.path); event('origin_body_disconnected', path=self.path)
Origin.do_GET = peer_origin_get

try:
    assert os.environ.get('DOWNLOADS_GUI_PARENT_NETNS') not in (None, os.readlink('/proc/self/ns/net')), 'Requires unshare -Urn'
    run(['ip', 'link', 'set', 'lo', 'up']); assert not run(['ip', 'route']).strip()
    source = sorted((REPO / 'src').rglob('*.rs')) + [REPO / 'Cargo.toml', REPO / 'Cargo.lock']
    assert all(p.stat().st_mtime_ns < BIN.stat().st_mtime_ns for p in source), 'Rebuild production binary'
    IDENTITY = dict(binary=str(BIN), binary_sha256=digest(BIN), probe_sha256=digest(__file__),
                    fixture_sha256=digest(REPO / 'docs/parity/fetch-downloads/run_acceptance.py'),
                    source={str(p.relative_to(REPO)): digest(p) for p in source},
                    namespaces={'parent_network': os.environ['DOWNLOADS_GUI_PARENT_NETNS'],
                                'network': os.readlink('/proc/self/ns/net'), 'user': os.readlink('/proc/self/ns/user')},
                    uname=run(['uname', '-a']).strip())
    event('identity', **IDENTITY)
    relay = socket.socket(); relay.bind(('127.0.0.1', 0)); relay.listen(64)
    threading.Thread(target=f.relay_loop, args=(relay,), daemon=True).start()
    seed_archive, magnet, info, peer = f.make_seed()
    PAYLOAD = seed_archive.read_bytes()
    event('seeder_ready', archive_sha256=digest(seed_archive), magnet=magnet, peer=peer,
          socks_port=relay.getsockname()[1])
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
    settings = {'peerSharing': True, 'peerMode': 'race',
                'peerProxies': [f'socks5://127.0.0.1:{relay.getsockname()[1]}'],
                'peerProxyListUrl': '', 'peerTrackerListUrl': None, 'peerTrackers': [],
                'peerExtraPeers': [peer], 'telemetryOptIn': False}
    case = fixture('peer-win', settings)
    metadata = {'Files': {'example.zip': True}, 'ContentHashSHA256': digest(seed_archive),
                'Magnet': magnet, 'TorrentInfo': info}
    guide = {'Components': [{'Guid': 'aaaaaaaa-1111-4111-8111-111111111111',
        'Name': 'Example Component', 'IsSelected': True,
        'Instructions': [{'Action': 'Extract', 'Source': ['<<modDirectory>>/example.zip']}],
        'ResourceRegistry': {BASE + '/peer-win': metadata}}]}
    (case / 'guide.json').write_text(json.dumps(guide, indent=2))
    guide_before, game_before = digest(case / 'guide.json'), manifest(case / 'game')
    wid = launch(case, 'peer-win'); downloads()
    click('Fetch Downloads')
    await_text('every planned archive is ready', timeout=60)
    status = screen('peer-win-terminal')[1]
    x('mousemove', 1150, 550)
    for _ in range(8): x('click', 5)
    screen('scrolled-to-finished-group')
    CALIBRATED['Expand finished peer result'] = [1010, 439]
    click('Finished', 'Expand finished peer result')
    x('mousemove', 1150, 550)
    for _ in range(3): x('click', 5)
    result_text = screen('expanded-peer-result')[1]
    check('native_result_source_peer', 'via Peer' in result_text)
    check('peer_proxy_connected', len(f.connects) > 0)
    check('origin_not_complete_before_peer_publish', ORIGIN_BODY_COMPLETE, [])
    check('peer_archive_published_exact', digest(case / 'mods/example.zip'), digest(seed_archive))
    check('no_second_archive', sorted(p.name for p in (case / 'mods').glob('*.zip')), ['example.zip'])
    check('no_partial_files', [p.name for p in (case / 'mods').rglob('*') if p.name.startswith(('.part', '.peer', '.tmp'))], [])
    check('guide_not_automatically_saved', digest(case / 'guide.json'), guide_before)
    check('game_unchanged', manifest(case / 'game'), game_before)
    event('terminal_text', text=status, peer_connections=f.connects, origin_requests=REQUESTS,
          origin_complete=ORIGIN_BODY_COMPLETE, origin_disconnected=ORIGIN_DISCONNECTED)
    COMPLETED = True; event('scenario_complete', scenario='peer-win')
except BaseException as error:
    event('error', type=type(error).__name__, message=str(error))
    if GUI and GUI.poll() is None:
        try: screen('failure')
        except Exception: pass
    raise
finally:
    close()
    if XVFB: XVFB.terminate(); XVFB.wait(10)
    for child in f.children:
        if child.poll() is None: child.terminate(); child.wait(10)
    (OUT / 'receipt.json').write_text(json.dumps({'identity': IDENTITY, 'completed': COMPLETED,
        'scenarios': sorted(CASES), 'checks': CHECKS, 'events': EVENTS, 'requests': REQUESTS,
        'peer_connections': f.connects, 'origin_complete': ORIGIN_BODY_COMPLETE,
        'origin_disconnected': ORIGIN_DISCONNECTED, 'pending': PENDING, 'accepted': False,
        'fixture_root': str(ROOT)}, indent=2))
    retained = OUT / 'fixture-state'; retained.mkdir(exist_ok=True)
    for case_name in CASE_NAMES:
        if (ROOT / case_name).exists(): shutil.copytree(ROOT / case_name, retained / case_name, dirs_exist_ok=True)
