#!/usr/bin/env python3
"""Downloads GUI step-1 native proof: the real production desktop binary on a private
Xvfb display, driven by xdotool, against recording loopback HTTPS/HTTP fixtures in one
isolated network namespace with private HOME/XDG/settings roots.

  DOWNLOADS_GUI_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/downloads-gui/probe.py

Builds nothing. No Rust test or harness, no synthetic GUI message, no fake acquisition
adapter and no real host: every action is a pointer click on the native window, and
every assertion reads server logs, the private filesystem or the process table.
Generic fixture names only. `explore` as the first argument sets up the first scenario,
launches the GUI and waits (for calibrating click positions); it records nothing.
"""
import hashlib, http.server, io, json, os, pathlib, random, shutil, socket, ssl, subprocess
import re, sys, tempfile, threading, time, zipfile

sys.dont_write_bytecode = True
REPO = pathlib.Path(__file__).resolve().parents[3]
TARGET = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
BIN = pathlib.Path(os.environ.get('MODSYNC_BINARY', str(TARGET/'debug/modsync'))).resolve()
OUT = pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(REPO/'docs/parity/downloads-gui'))).resolve()
EXPLORE = sys.argv[1:2] == ['explore']
PARENT = os.environ.get('DOWNLOADS_GUI_PARENT_NETNS')
assert PARENT and PARENT != os.readlink('/proc/self/ns/net'), 'run under unshare -Urn with DOWNLOADS_GUI_PARENT_NETNS'
subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
assert not subprocess.check_output(['ip', 'route'], text=True).strip(), 'namespace must have no routes'

def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def source_files(): return sorted((REPO/'src').rglob('*.rs')) + [REPO/'Cargo.toml', REPO/'Cargo.lock']
SOURCE = {str(p.relative_to(REPO)): digest(p) for p in source_files()}
BINARY = digest(BIN)
STALE = [str(p.relative_to(REPO)) for p in source_files() if p.stat().st_mtime_ns >= BIN.stat().st_mtime_ns]
assert not STALE, f'binary {BIN} is older than hashed source {STALE}; rebuild first'
def git(*args):
    r = subprocess.run(['git', '-c', f'safe.directory={REPO}', '-C', str(REPO), *args], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None

ROOT = pathlib.Path(tempfile.mkdtemp(prefix='downloads-gui-'))
SHOTS = OUT/'screenshots'
if not EXPLORE:
    shutil.rmtree(SHOTS, ignore_errors=True); SHOTS.mkdir(parents=True)
CHECKS = []; REQUESTS = []; API = []; EVENTS = []; LOCK = threading.Lock(); T0 = time.monotonic()

def now(): return round(time.monotonic() - T0, 3)
def event(kind, **kw):
    row = dict(t=now(), kind=kind, **kw)
    with LOCK: EVENTS.append(row)
    print(json.dumps(row), flush=True)
def check(name, actual, expected=True):
    row = dict(name=name, actual=actual, expected=expected, passed=actual == expected); CHECKS.append(row)
    print(('ok  ' if row['passed'] else 'FAIL'), name, '' if row['passed'] else repr(actual)[:300], flush=True)

# ---------------------------------------------------------------- archives
def zipped(name, payload):
    b = io.BytesIO()
    with zipfile.ZipFile(b, 'w', compression=zipfile.ZIP_STORED) as z:
        z.writestr(zipfile.ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0)), payload)
    return b.getvalue()
rng = random.Random(7)
SLOW_ONE = zipped('payload/one.bin', bytes(rng.getrandbits(8) for _ in range(3 << 20)))
SLOW_TWO = zipped('payload/two.bin', bytes(rng.getrandbits(8) for _ in range(3 << 20)))
WIDE = zipped('payload/wide.txt', b'widescreen fixture\n')
# Already in the workspace; slow to verify (one deflated entry of zeros), so the
# preparation phase (plan + existing-archive verification) is visible for seconds.
def slow_verify(size=1920 << 20):
    b = io.BytesIO(); info = zipfile.ZipInfo('payload/present.bin', date_time=(2020, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
    with zipfile.ZipFile(b, 'w') as z:
        with z.open(info, 'w') as f:
            for _ in range(size >> 20): f.write(bytes(1 << 20))
    return b.getvalue()
PRESENT = slow_verify()
NEXUS = zipped('payload/nexus.txt', b'nexus handoff fixture\n')
HUGE = zipped('payload/huge.bin', bytes(rng.getrandbits(8) for _ in range(8 << 20)))
LANDING = b'<!doctype html><html><head><title>Example download page</title></head><body><p>Choose a mirror to continue.</p></body></html>'
CHUNK = 64 << 10
# path -> (body, content type, send length, seconds between chunks)
SERVE = {
    '/slow/one.zip': (SLOW_ONE, 'application/zip', True, 0.12),
    '/slow/two.zip': (SLOW_TWO, 'application/zip', False, 0.12),
    '/wide/wide.zip': (WIDE, 'application/zip', True, 0),
    '/page/landing': (LANDING, 'text/html; charset=utf-8', True, 0),
    '/unselected/never.zip': (WIDE, 'application/zip', True, 0),
    '/cdn/nexus-example.zip': (NEXUS, 'application/zip', True, 0),
    '/stop/huge.zip': (HUGE, 'application/zip', True, 1.5),
}

# ---------------------------------------------------------------- TLS
key, cert = ROOT/'loopback.key', ROOT/'loopback.pem'
CERT_ARGS = ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(key), '-out', str(cert), '-days', '1', '-subj', '/CN=Loopback Fixture', '-addext', 'subjectAltName=IP:127.0.0.1', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign']
subprocess.run(CERT_ARGS, capture_output=True, check=True)
wrong_key, wrong_cert = ROOT/'wrong.key', ROOT/'wrong.pem'
subprocess.run([a if a not in (str(key), str(cert)) else (str(wrong_key) if a == str(key) else str(wrong_cert)) for a in CERT_ARGS], capture_output=True, check=True)
leafkey, leaf, csr, ext = ROOT/'leaf.key', ROOT/'leaf.pem', ROOT/'leaf.csr', ROOT/'leaf.ext'
ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
for args in [['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(leafkey), '-out', str(csr), '-subj', '/CN=Loopback Server'],
             ['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(cert), '-CAkey', str(key), '-CAcreateserial', '-out', str(leaf), '-days', '1', '-extfile', str(ext)]]:
    subprocess.run(args, capture_output=True, check=True)

class Origin(http.server.BaseHTTPRequestHandler):
    # HTTP/1.1 so a body without Content-Length is chunked and ends cleanly under TLS.
    protocol_version = 'HTTP/1.1'
    def do_GET(self):
        path = self.path.split('?', 1)[0]
        row = dict(path=path, query=self.path.partition('?')[2], start=now(), sent=0, complete=False, headers=dict(self.headers))
        with LOCK: REQUESTS.append(row)
        body, ctype, length, pause = SERVE.get(path, (b'missing', 'application/zip', True, 0))
        try:
            self.send_response(200 if path in SERVE else 404)
            self.send_header('Content-Type', ctype)
            self.send_header('Connection', 'close')
            if length: self.send_header('Content-Length', str(len(body)))
            else: self.send_header('Transfer-Encoding', 'chunked')
            self.end_headers()
            for i in range(0, len(body), CHUNK):
                piece = body[i:i + CHUNK]
                self.wfile.write(piece if length else b'%x\r\n%s\r\n' % (len(piece), piece)); self.wfile.flush(); row['sent'] += len(piece)
                if pause: time.sleep(pause)
            if not length: self.wfile.write(b'0\r\n\r\n'); self.wfile.flush()
            row['complete'] = True
            self.close_connection = True
        except (BrokenPipeError, ConnectionResetError, ssl.SSLError, OSError) as error:
            row['aborted'] = type(error).__name__
        row['end'] = now()
    def log_message(self, *args): pass
class Quiet(http.server.ThreadingHTTPServer):
    daemon_threads = True
    def handle_error(self, request, address):
        with LOCK: REQUESTS.append(dict(path=None, start=now(), tlsError=str(sys.exc_info()[1])[:200]))
origin = Quiet(('127.0.0.1', 0), Origin)
tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); tls.load_cert_chain(leaf, leafkey)
origin.socket = tls.wrap_socket(origin.socket, server_side=True)
threading.Thread(target=origin.serve_forever, daemon=True).start()
BASE = f'https://127.0.0.1:{origin.server_port}'

SYNTHETIC_KEY = 'synthetic-probe-key-5d2e'
STORED_SENTINEL = 'stored-sentinel-key-91ab'
ENV_SENTINEL = 'env-sentinel-key-44cd'
NXM_KEY = 'nxm-link-key-0f7a'
SIGNED = 'signed-cdn-token-3b8c'
class NexusApi(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split('?', 1)[0]
        with LOCK: API.append(dict(path=path, query=self.path.partition('?')[2], apikey=self.headers.get('apikey'), t=now()))
        if path.endswith('/files/2.json'):
            body = {'file_name': 'nexus-example.zip', 'file_id': 2}
        elif path.endswith('/files/2/download_link.json'):
            body = [{'URI': f'{BASE}/cdn/nexus-example.zip?md5={SIGNED}&expires=4102444800', 'short_name': 'Loopback'}]
        else:
            self.send_response(404); self.end_headers(); return
        data = json.dumps(body).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json'); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)
    def log_message(self, *args): pass
api = http.server.ThreadingHTTPServer(('127.0.0.1', 0), NexusApi)
threading.Thread(target=api.serve_forever, daemon=True).start()
API_BASE = f'http://127.0.0.1:{api.server_port}/v1'

# ---------------------------------------------------------------- private roots
HOME = ROOT/'home'
XDG = {k: HOME/v for k, v in [('XDG_CONFIG_HOME', '.config'), ('XDG_CACHE_HOME', '.cache'), ('XDG_DATA_HOME', '.local/share')]}
RUNTIME = ROOT/'runtime'
for p in [HOME, RUNTIME, *XDG.values()]: p.mkdir(parents=True, exist_ok=True)
RUNTIME.chmod(0o700)
USER = 'fixture'
# A listener where the ordinary per-user instance socket lives: a probe must never touch it.
USER_SOCKET = RUNTIME/f'modsync-rs-{USER}.sock'
FORWARDED = []
listener = socket.socket(socket.AF_UNIX); listener.bind(str(USER_SOCKET)); listener.listen(4)
def watch_socket():
    while True:
        conn, _ = listener.accept()
        FORWARDED.append(conn.recv(8192).decode(errors='replace')); conn.close()
threading.Thread(target=watch_socket, daemon=True).start()
# The default settings directory holds a sentinel key a probe must never send.
DEFAULT_SETTINGS = XDG['XDG_CONFIG_HOME']/'ModSync'; DEFAULT_SETTINGS.mkdir(parents=True)
(DEFAULT_SETTINGS/'settings.json').write_text(json.dumps({'nexusModsApiKey': STORED_SENTINEL}))
DISPLAY = ':93'
ENV = {'PATH': '/usr/bin:/bin', 'HOME': str(HOME), 'USER': USER, 'XDG_RUNTIME_DIR': str(RUNTIME), 'DISPLAY': DISPLAY,
       'LIBGL_ALWAYS_SOFTWARE': '1', 'NEXUS_MODS_API_KEY': ENV_SENTINEL, 'KOTOR_MODSYNC_NEXUS_API_KEY': ENV_SENTINEL, 'LANG': 'C.UTF-8',
       **{k: str(v) for k, v in XDG.items()}}

def snapshot(p): return {str(f.relative_to(p)): digest(f) for f in sorted(p.rglob('*')) if f.is_file()}
EMPTY = hashlib.sha256(b'').hexdigest()
def documented_state(name, value):
    # Dated NLog-style log (runtime-revalidation-2026-10-03.md) and empty cooperating-writer
    # lock files: sibling .modsync-lock-<hash> (acquisition-root-coordination.md) and the
    # peer-sharing owner/transaction locks (persistent-sharing-plan.md).
    return (re.fullmatch(r'settings/Logs/kotormodsync_\d{4}-\d{2}-\d{2}\.log', name) is not None
            or (value == EMPTY and (re.fullmatch(r'\.modsync-lock-[0-9a-f]{16}', name) is not None
                                    or name in ('settings/peer-sharing-v1/owner.lock', 'settings/peer-sharing-v1/transaction.lock'))))
def content(p): return {k: v for k, v in snapshot(p).items() if not documented_state(k, v)}
GUIDS = [f'{c * 8}-{c * 4}-4{c * 3}-8{c * 3}-{c * 12}' for c in 'abcdefghij']
def comp(i, name, url, filename, selected=True, flag=True, wide=False):
    meta = {'Files': {filename: flag}} if filename else {'Files': {}}
    row = {'Guid': GUIDS[i], 'Name': name, 'IsSelected': selected, 'Category': ['Example'], 'Tier': '1 - Essential',
           'Instructions': [{'Action': 'Extract', 'Source': [f'<<modDirectory>>/{filename or "nexus-example.zip"}']}],
           'ResourceRegistry': {url: meta}}
    if wide: row['WidescreenOnly'] = True
    return row
def scenario(name, components, present=()):
    case = ROOT/name; mods = case/'mods'; game = case/'game'; settings = case/'settings'
    for p in [mods, game/'override', settings]: p.mkdir(parents=True)
    (game/'swkotor.exe').write_bytes(b'detection fixture')
    for filename, data in present: (mods/filename).write_bytes(data)
    (settings/'settings.json').write_text(json.dumps({'nexusModsApiKey': STORED_SENTINEL, 'peerSharing': False, 'usePublicPeerList': False, 'telemetryOptIn': False, 'fixtureUnknownKey': 'keep'}))
    guide = case/'guide.json'; guide.write_text(json.dumps({'Components': components}, indent=1))
    return dict(case=case, mods=mods, game=game, settings=settings, guide=guide)
MAIN = [
    comp(0, 'Example Base One', f'{BASE}/slow/one.zip', 'one.zip'),
    comp(1, 'Example Base Two', f'{BASE}/slow/two.zip', 'two.zip'),
    comp(2, 'Example Present', f'{BASE}/present/present.zip', 'present.zip'),
    comp(3, 'Example Manual', f'{BASE}/page/landing', 'manual.zip'),
    comp(4, 'Example Missing', f'{BASE}/missing/gone.zip', 'gone.zip'),
    comp(5, 'Example Unselected', f'{BASE}/unselected/never.zip', 'never.zip', selected=False),
    comp(6, 'Example Widescreen', f'{BASE}/wide/wide.zip', 'wide.zip', wide=True),
    comp(7, 'Example Flagged False', f'{BASE}/unselected/never.zip', 'flagged.zip', flag=False),
    comp(8, 'Example Nexus', 'https://www.nexusmods.com/kotor/mods/1', None, selected=False),
]
# The present archive is marked configured, so no post-run FOMOD inspection of its
# large (slow) payload delays the terminal; it still slows preparation's verification.
MAIN[2]['ResourceRegistry'][f'{BASE}/present/present.zip']['HandlerMetadata'] = {'fomodPromptStatus': {'present.zip': 'configured'}}

# ---------------------------------------------------------------- display and actions
xvfb = subprocess.Popen(['Xvfb', DISPLAY, '-screen', '0', '1280x800x24', '-nolisten', 'tcp'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1.5)
def x(*args, check_=True):
    return subprocess.run(['xdotool', *args], env={**ENV}, capture_output=True, text=True, check=check_).stdout.strip()
GUI = [None]
def launch(case, extra=(), protocol=None, label='launch', ca=None):
    argv = [str(BIN), 'gui', '--settings-dir', str(case['settings']), '--input', str(case['guide']), '--game-dir', str(case['game']),
            '--source-dir', str(case['mods']), '--nexus-mods-api-key', SYNTHETIC_KEY, '--nexus-api-base', API_BASE, '--extra-ca-file', str(ca or cert), *extra]
    if protocol: argv += ['--protocol', protocol]
    log = open(ROOT/f'{label}.log', 'w')
    GUI[0] = subprocess.Popen(argv, env=ENV, stdout=log, stderr=subprocess.STDOUT)
    event('launch', label=label, argv=argv)
    for _ in range(100):
        wid = x('search', '--name', '^ModSync.rs$', check_=False)
        if wid: break
        time.sleep(0.2)
    assert wid, 'window did not appear'
    wid = wid.split()[0]
    x('windowmove', wid, '0', '0'); x('windowsize', wid, '1280', '800'); time.sleep(1.5)
    return wid
def size(wid, w, h):
    x('windowsize', wid, str(w), str(h)); time.sleep(1.2); event('resize', width=w, height=h)
def click(px, py, what):
    x('mousemove', str(px), str(py)); time.sleep(0.15); x('click', '1'); event('click', x=px, y=py, target=what); time.sleep(0.6)
def shot(name, wid=None):
    path = SHOTS/f'{name}.png'
    subprocess.run(['import', '-window', 'root', str(path)], env=ENV, check=True, capture_output=True)
    event('screenshot', name=name, sha256=digest(path))
def wait_for(predicate, what, timeout=60):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if predicate(): event('observed', what=what); return True
        time.sleep(0.1)
    event('timeout', what=what); return False
def close_gui():
    p = GUI[0]
    if p and p.poll() is None:
        p.terminate()
        try: p.wait(10)
        except subprocess.TimeoutExpired: p.kill(); p.wait()
    event('closed', code=p.returncode if p else None)

if EXPLORE:
    main = scenario('explore', MAIN, present=[('present.zip', PRESENT)])
    wid = launch(main, label='explore')
    print('EXPLORE', json.dumps(dict(display=DISPLAY, root=str(ROOT), base=BASE, api=API_BASE, wid=wid)), flush=True)
    while not (ROOT/'explore-done').exists():
        time.sleep(0.5)
    close_gui(); xvfb.terminate(); shutil.rmtree(ROOT, ignore_errors=True); sys.exit(0)

# ---------------------------------------------------------------- helpers for the run
def column_runs(path, col, top=0, height=800):
    out = subprocess.run(['convert', str(path), '-crop', f'1x{height}+{col}+{top}', 'txt:-'], capture_output=True, text=True, check=True).stdout
    runs = []
    for line in out.splitlines()[1:]:
        y = int(line.split(':', 1)[0].split(',')[1]) + top; color = line.split()[2]
        if runs and runs[-1][2] == color and runs[-1][1] == y - 1: runs[-1][1] = y
        else: runs.append([y, y, color])
    return runs
def footer_button(col=1240):
    """The wizard footer's rightmost button (Next/Continue): the lowest solid run of
    button height in that column, which lies above only the activity panel."""
    probe = ROOT/'locate.png'
    subprocess.run(['import', '-window', 'root', str(probe)], env=ENV, check=True, capture_output=True)
    for start, end, color in reversed(column_runs(probe, col)):
        if color != '#FFFFFF' and 26 <= end - start + 1 <= 40:
            return (start + end) // 2
    raise AssertionError('footer button not found')
def next_page(what):
    y = footer_button(); click(1205, y, f'Next ({what})')
def requests(path): return [r for r in REQUESTS if r.get('path') == path]
def mods_files(case): return {f.name: digest(f) for f in case['mods'].iterdir()}

FETCH = (1027, 530); STOP = (1150, 530)  # recalibrated: peer-sharing text moved the button row
RESULTS = {}
try:
    # ============ Scenario 1: nxm handoff at launch, then Fetch at 1280x800
    main = scenario('main', MAIN, present=[('present.zip', PRESENT)])
    before_main = snapshot(main['case'])
    nxm = f'nxm://kotor/mods/1/files/2?key={NXM_KEY}&expires=4102444800'
    wid = launch(main, protocol=nxm, label='main')
    check('nxm_archive_written', wait_for(lambda: (main['mods']/'nexus-example.zip').exists() and requests('/cdn/nexus-example.zip') and requests('/cdn/nexus-example.zip')[-1].get('complete'), 'nxm archive'), True)
    time.sleep(1.5); shot('01-nxm-handoff-terminal')
    check('nxm_archive_bytes', digest(main['mods']/'nexus-example.zip'), hashlib.sha256(NEXUS).hexdigest())
    check('nxm_api_keys_sent', sorted({a['apikey'] for a in API}), [SYNTHETIC_KEY])
    check('nxm_link_query_carries_link_key', any(f'key={NXM_KEY}' in a['query'] for a in API if a['path'].endswith('download_link.json')), True)
    check('no_sentinel_on_any_request', any(STORED_SENTINEL in json.dumps(r) or ENV_SENTINEL in json.dumps(r) for r in API + REQUESTS), False)
    for _ in range(4): next_page('to DownloadsExplain')
    shot('02-downloads-explain-idle')
    check('no_fetch_request_before_click', [r['path'] for r in REQUESTS if r.get('path') and not r['path'].startswith('/cdn/')], [])
    click(*FETCH, 'Fetch Downloads'); time.sleep(0.4); shot('03-fetch-preparing')
    check('preparation_sent_no_request_yet', [r['path'] for r in REQUESTS if r.get('path') and not r['path'].startswith('/cdn/')], [])
    live = lambda: all(requests(p) and 0 < requests(p)[-1]['sent'] < len(SERVE[p][0]) for p in ['/slow/one.zip', '/slow/two.zip'])
    check('two_transfers_live_together', wait_for(live, 'two live transfers', 90), True)
    time.sleep(0.6); shot('04-two-live-rows')
    next_page('to Validate while downloading'); time.sleep(0.8); shot('05-status-after-navigation')
    done = lambda: all(requests(p) and requests(p)[-1].get('end') is not None for p in ['/slow/one.zip', '/slow/two.zip'])
    check('transfers_finished', wait_for(done, 'transfers finished', 120), True)
    time.sleep(5); shot('06-terminal-validate-page')
    # Back to the DownloadsExplain page is disabled only while busy; the sidebar keeps status.
    x('mousemove', '1110', '560'); [x('click', '5') for _ in range(12)]; time.sleep(0.8); shot('07-manual-and-failed-rows')
    size(wid, 600, 400); shot('08-terminal-600x400')
    x('mousemove', '450', '200'); [x('click', '5') for _ in range(6)]; time.sleep(0.6); shot('09-terminal-600x400-scrolled')
    size(wid, 1280, 800)
    close_gui()
    gui_fetch = sorted(r['path'] for r in REQUESTS if r.get('path') and not r['path'].startswith('/cdn/'))
    after_main = mods_files(main)
    RESULTS['main'] = dict(requests=gui_fetch, mods=after_main)
    check('unselected_never_requested', requests('/unselected/never.zip'), [])
    check('present_not_requested', requests('/present/present.zip'), [])
    check('gui_archives', sorted(after_main), ['nexus-example.zip', 'one.zip', 'present.zip', 'two.zip', 'wide.zip'])
    check('gui_archive_bytes', {k: after_main[k] for k in ['one.zip', 'two.zip', 'wide.zip']},
          {'one.zip': hashlib.sha256(SLOW_ONE).hexdigest(), 'two.zip': hashlib.sha256(SLOW_TWO).hexdigest(), 'wide.zip': hashlib.sha256(WIDE).hexdigest()})
    check('guide_file_not_saved_by_gui', digest(main['guide']), before_main['guide.json'])
    check('game_untouched', {k: v for k, v in snapshot(main['case']).items() if k.startswith('game/')}, {k: v for k, v in before_main.items() if k.startswith('game/')})
    check('settings_unchanged', {k: v for k, v in content(main['case']).items() if k.startswith('settings/')}, {k: v for k, v in before_main.items() if k.startswith('settings/')})
    check('no_partial_files_in_workspace', [n for n in after_main if n.startswith('.') or n.endswith(('.part', '.tmp'))], [])

    # ============ Scenario 2: the same fixture through the production CLI
    cli = scenario('cli', MAIN, present=[('present.zip', PRESENT)])
    start = len(REQUESTS)
    argv = [str(BIN), 'fetch-downloads', '--input', str(cli['guide']), '--source-dir', str(cli['mods']), '--settings-dir', str(cli['settings']),
            '--use-file-selection', '--record', '--extra-ca-file', str(cert)]
    r = subprocess.run(argv, env=ENV, capture_output=True, text=True, timeout=300)
    event('cli', argv=argv, exit=r.returncode)
    (ROOT/'cli.stdout').write_text(r.stdout); (ROOT/'cli.stderr').write_text(r.stderr)
    report = json.loads(r.stdout)
    RESULTS['cli'] = dict(exit=r.returncode, report=report)
    cli_fetch = sorted(x_['path'] for x_ in REQUESTS[start:] if x_.get('path'))
    check('cli_same_requests_as_gui_fetch', cli_fetch, gui_fetch)
    check('cli_same_archive_bytes', {k: v for k, v in mods_files(cli).items()}, {k: v for k, v in after_main.items() if k != 'nexus-example.zip'})
    check('cli_statuses', [(o['targetIndex'], o['status']) for o in report['report']['outcomes']],
          [(0, 'completed'), (1, 'completed'), (2, 'manualRequired'), (3, 'failed'), (4, 'completed')])
    check('cli_recorded_count', len(report['recordSummary']['recorded']), 4)
    check('cli_skips', sorted(s_['reason'] for s_ in report['report']['skipped']), ['allFilesFlaggedFalse', 'alreadyPresent'])
    check('cli_exit_failed', r.returncode, 1)

    # ============ Scenario 3: Stop mid-body
    stop = scenario('stop', [comp(0, 'Example Large', f'{BASE}/stop/huge.zip', 'huge.zip')])
    before_stop = content(stop['case'])
    wid = launch(stop, label='stop')
    for _ in range(4): next_page('to DownloadsExplain')
    click(*FETCH, 'Fetch Downloads')
    check('stop_transfer_started', wait_for(lambda: requests('/stop/huge.zip') and requests('/stop/huge.zip')[-1]['sent'] >= 3 * CHUNK, 'large transfer mid-body', 60), True)
    shot('10-large-transfer-live')
    click(*STOP, 'Stop Downloads'); time.sleep(0.5); shot('11-stop-confirmation')
    STOP_MODAL = (397, 434)
    click(*STOP_MODAL, 'Stop downloads (confirmation)'); time.sleep(0.25); shot('12-stopping')
    first = requests('/stop/huge.zip')[-1]
    check('stop_request_ends_incomplete', wait_for(lambda: first.get('end') is not None, 'first large request ended', 30) and not first['complete'], True)
    time.sleep(2.5); shot('13-stopped-summary')
    check('stop_gui_still_running', GUI[0].poll(), None)
    check('stop_no_residue', content(stop['case']), before_stop)
    # Retry: a fresh run plans again and makes a new request; then stop it too.
    click(*FETCH, 'Fetch Downloads (retry after stop)')
    check('retry_new_request', wait_for(lambda: len(requests('/stop/huge.zip')) == 2 and requests('/stop/huge.zip')[-1]['sent'] >= 2 * CHUNK, 'retry transfer', 60), True)
    check('retry_started_after_first_ended', requests('/stop/huge.zip')[1]['start'] > first['end'], True)
    shot('14-retry-live')
    click(*STOP, 'Stop Downloads (retry)'); time.sleep(0.5); click(*STOP_MODAL, 'Stop downloads (confirmation)')
    second = requests('/stop/huge.zip')[-1]
    check('retry_stopped', wait_for(lambda: second.get('end') is not None, 'retry request ended', 30) and not second['complete'], True)
    time.sleep(2.5); shot('15-retry-stopped')
    check('retry_no_residue', content(stop['case']), before_stop)
    close_gui()

    # ============ Scenario 4: probe launches that must be refused before any window
    alias = ROOT/'alias-settings'; alias.symlink_to(DEFAULT_SETTINGS)
    nested = DEFAULT_SETTINGS/'nested'; nested.mkdir()
    private = ROOT/'private-settings'; private.mkdir()
    refusals = {
        'no_settings_dir': ['--nexus-mods-api-key', SYNTHETIC_KEY],
        'default_settings_dir': ['--settings-dir', str(DEFAULT_SETTINGS), '--nexus-mods-api-key', SYNTHETIC_KEY],
        'alias_of_default': ['--settings-dir', str(alias), '--extra-ca-file', str(cert)],
        'nested_in_default': ['--settings-dir', str(nested), '--extra-ca-file', str(cert)],
        'api_base_without_key': ['--settings-dir', str(private), '--nexus-api-base', API_BASE],
        'unreadable_ca': ['--settings-dir', str(private), '--extra-ca-file', str(ROOT/'absent.pem')],
    }
    for name, extra in refusals.items():
        argv = [str(BIN), 'gui', *extra]
        r = subprocess.run(argv, env=ENV, capture_output=True, text=True, timeout=30)
        event('refusal', name=name, argv=argv, exit=r.returncode, stderr=r.stderr.strip())
        check(f'refused_{name}', (r.returncode, bool(x('search', '--name', '^ModSync.rs$', check_=False))), (1, False))
    check('refusal_messages', {n: e['stderr'] for n, e in [(ev['name'], ev) for ev in EVENTS if ev['kind'] == 'refusal']}, {
        'no_settings_dir': 'Error: Probe options require --settings-dir with a private settings directory',
        'default_settings_dir': 'Error: Probe options require a settings directory separate from the default settings directory',
        'alias_of_default': 'Error: Probe options require a settings directory separate from the default settings directory',
        'nested_in_default': 'Error: Probe options require a settings directory separate from the default settings directory',
        'api_base_without_key': 'Error: --nexus-api-base requires an explicit --nexus-mods-api-key',
        'unreadable_ca': 'Error: Cannot read loopback CA certificate',
    })

    # ============ Scenario 5: a wrong loopback CA refuses the archive transfer
    wrong = scenario('wrong-ca', MAIN[8:])
    cdn_before = len(requests('/cdn/nexus-example.zip')); api_before = len(API)
    wid = launch(wrong, protocol=nxm, label='wrong-ca', ca=wrong_cert)
    check('wrong_ca_link_resolved', wait_for(lambda: any(a['path'].endswith('download_link.json') for a in API[api_before:]), 'download link requested', 30), True)
    time.sleep(4); shot('16-wrong-ca-refused')
    check('wrong_ca_no_archive', sorted(f.name for f in wrong['mods'].iterdir()), [])
    check('wrong_ca_no_cdn_request', len(requests('/cdn/nexus-example.zip')) - cdn_before, 0)
    close_gui()

    # ============ Scenario 6: the shared transfer reports totals the same way to the CLI
    totals = scenario('totals', [comp(0, 'Example Base One', f'{BASE}/slow/one.zip', 'one.zip')])
    argv = [str(BIN), 'fetch-downloads', '--input', str(totals['guide']), '--source-dir', str(totals['mods']), '--settings-dir', str(totals['settings']),
            '--use-file-selection', '--progress-json', '--extra-ca-file', str(cert)]
    r = subprocess.run(argv, env=ENV, capture_output=True, text=True, timeout=120)
    rows = [json.loads(line) for line in r.stdout.splitlines() if line.startswith('{')]
    seen = sorted({json.dumps(e['event']['phase'].get('total')) for e in rows if e.get('type') == 'acquisition' and e['event']['phase']['phase'] == 'transferring'})
    event('cli-totals', argv=argv, exit=r.returncode, totals=seen)
    # Observed shared-transport fact, not a GUI choice: every Origin Transferring event
    # carries total 0 although the server sent Content-Length (download.rs reads the length
    # after sniffing the body prefix). The GUI shows such a total as unknown, never a percent.
    check('cli_transfer_totals_are_zero_not_content_length', seen, ['0'])

    check('no_forwarding_to_user_socket', FORWARDED, [])
    check('user_socket_left_alone', sorted(p.name for p in RUNTIME.iterdir()), [USER_SOCKET.name])
    check('no_sentinel_anywhere', any(t in json.dumps(API + REQUESTS) for t in [STORED_SENTINEL, ENV_SENTINEL]), False)
    check('api_only_explicit_key', sorted({a['apikey'] for a in API}), [SYNTHETIC_KEY])
finally:
    close_gui(); xvfb.terminate()
    logs = OUT/'logs'; shutil.rmtree(logs, ignore_errors=True); logs.mkdir(parents=True)
    for f in ROOT.glob('*.log'): shutil.copy(f, logs/f.name)
    for f in ['cli.stdout', 'cli.stderr']:
        if (ROOT/f).exists(): shutil.copy(ROOT/f, logs/f)
    receipt = dict(
        binding=dict(binary=str(BIN), binarySha256=BINARY, gitHead=git('rev-parse', 'HEAD'),
                     gitPorcelain=git('status', '--porcelain', '--', 'src', 'Cargo.toml', 'Cargo.lock'), sourceSha256=SOURCE),
        networkNamespace=os.readlink('/proc/self/ns/net'), parentNetworkNamespace=PARENT,
        routes=subprocess.check_output(['ip', 'route'], text=True), display=DISPLAY, root=str(ROOT), originBase=BASE, apiBase=API_BASE,
        certificateCommand=CERT_ARGS, events=EVENTS, requests=[{k: v for k, v in r.items() if k != 'headers'} | {'headers': sorted(r.get('headers', {}))} for r in REQUESTS],
        api=API, forwarded=FORWARDED, results=RESULTS, checks=CHECKS,
        passed=sum(c['passed'] for c in CHECKS), total=len(CHECKS))
    (OUT/'receipt.json').write_text(json.dumps(receipt, indent=1) + '\n')
    print(json.dumps(dict(passed=receipt['passed'], total=receipt['total'], out=str(OUT))), flush=True)
    if os.environ.get('MODSYNC_PROOF_KEEP') != '1': shutil.rmtree(ROOT, ignore_errors=True)
    if receipt['passed'] != receipt['total']: sys.exit(1)
