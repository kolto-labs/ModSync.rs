#!/usr/bin/env python3
"""Nexus H1a production-CLI proof: shared API redirect policy and nxm key encoding.

Run inside a disposable network namespace with no routes:
  NEXUS_H1A_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/nexus-h1a-proof/probe.py
Builds nothing. Drives the existing production binary (MODSYNC_BINARY, else
$CARGO_TARGET_DIR/debug/modsync, else target/debug/modsync) through the existing
explicit-key `--nexus-api-base` transport against recording loopback servers.
No Rust test or harness, no fake acquisition adapter, no real Nexus host.
"""
import hashlib, http.server, io, json, os, pathlib, select, shutil, socket, ssl, subprocess
import sys, tempfile, threading, time, traceback, urllib.parse, urllib.request, zipfile

sys.dont_write_bytecode = True
REPO = pathlib.Path(__file__).resolve().parents[3]
TARGET = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', str(REPO/'target'))).resolve()
OUT = pathlib.Path(os.environ.get('MODSYNC_PROOF_OUT', str(TARGET/'cloud-proof/nexus-h1a'))).resolve()
OUT.mkdir(parents=True, exist_ok=True)
BIN = pathlib.Path(os.environ.get('MODSYNC_BINARY', str(TARGET/'debug/modsync'))).resolve()
PARENT = os.environ.get('NEXUS_H1A_PARENT_NETNS')
assert PARENT and PARENT != os.readlink('/proc/self/ns/net'), 'run under unshare -Urn with NEXUS_H1A_PARENT_NETNS'
subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
assert not subprocess.check_output(['ip', 'route'], text=True).strip(), 'namespace must have no routes'

def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def source_files(): return sorted((REPO/'src').rglob('*.rs')) + [REPO/'Cargo.toml', REPO/'Cargo.lock']
SOURCE = {str(p.relative_to(REPO)): digest(p) for p in source_files()}
BINARY = digest(BIN)
# Bind the binary to this source: it must have been built after every hashed file changed.
STALE = [str(p.relative_to(REPO)) for p in source_files() if p.stat().st_mtime_ns >= BIN.stat().st_mtime_ns]
assert not STALE, f'binary {BIN} is older than hashed source {STALE}; rebuild before running the proof'
def git(*args): return subprocess.check_output(['git', '-c', f'safe.directory={REPO}', '-C', str(REPO)] + list(args), text=True)
GIT_HEAD = git('rev-parse', 'HEAD').strip()
GIT_STATUS = git('status', '--porcelain', '--', 'src', 'Cargo.toml', 'Cargo.lock')
ROOT = pathlib.Path(tempfile.mkdtemp(prefix='nexus-h1a-'))
ENV = {k: v for k, v in os.environ.items() if not k.lower().endswith('_proxy') and 'NEXUS' not in k}
for k in ['HOME', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME']:
    ENV[k] = str(ROOT/k); pathlib.Path(ENV[k]).mkdir()

API_KEY = 'fixture-api-key-7f3c'
NXM_KEY = 'a+b/c=d&e f%'
# Every spelling of the key that could leak into output: raw, strict and form encoded.
NXM_KEY_FORMS = [NXM_KEY, urllib.parse.quote(NXM_KEY, safe=''), urllib.parse.quote_plus(NXM_KEY)]
EXPIRES = 4102444800
CHECKS = []; RUNS = []; LOG = []; LOCK = threading.Lock()
ACTIVE = [0]  # accepted (or accepting) connections whose handler has not finished

def check(name, actual, expected=True):
    row = dict(name=name, actual=actual, expected=expected, passed=actual == expected); CHECKS.append(row)
    if not row['passed']: print('FAIL', name, repr(actual), flush=True)

def record(kind, **kw):
    with LOCK: LOG.append(dict(kind=kind, time=time.monotonic(), **kw))

def archive_bytes():
    b = io.BytesIO()
    with zipfile.ZipFile(b, 'w') as z:
        z.writestr(zipfile.ZipInfo('payload/example.txt', date_time=(2020, 1, 1, 0, 0, 0)), 'example payload\n')
    return b.getvalue()
ARCHIVE = archive_bytes(); ARCHIVE_SHA = hashlib.sha256(ARCHIVE).hexdigest()

class Recording(http.server.BaseHTTPRequestHandler):
    """Records every accepted connection's first byte (0x16 = TLS ClientHello) and every
    parsed request line with all headers, before answering."""
    def setup(self):
        super().setup()
        if isinstance(self.connection, ssl.SSLSocket):
            # TLS servers complete the handshake in accept(); the connection is TLS.
            record('connection', server=self.server.label, firstByte='16', tls=True); return
        try:
            self.connection.settimeout(5)
            first = self.connection.recv(1, socket.MSG_PEEK)
        except (OSError, ssl.SSLError):
            first = b''
        record('connection', server=self.server.label, firstByte=first.hex(), tls=first == b'\x16')

    def do_GET(self):
        record('request', server=self.server.label, requestLine=self.requestline, path=self.path,
               headers=[[k, v] for k, v in self.headers.items()])
        code, headers, body = self.server.respond(self)
        self.send_response(code)
        for k, v in headers.items(): self.send_header(k, v)
        self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)

    def log_message(self, *args): pass

class Server(http.server.ThreadingHTTPServer):
    def get_request(self):
        with LOCK: ACTIVE[0] += 1
        try:
            return super().get_request()
        except ssl.SSLError as e:
            # A TLS client that rejected this server's certificate still reached it.
            record('tls-handshake-failed', server=self.label, reason=e.reason)
            with LOCK: ACTIVE[0] -= 1
            raise
        except BaseException:
            with LOCK: ACTIVE[0] -= 1
            raise

    def shutdown_request(self, request):
        # Every accepted request ends here, after its handler (or on refusal/error).
        try:
            super().shutdown_request(request)
        finally:
            with LOCK: ACTIVE[0] -= 1

SERVERS = []
def serve(address, port, label, respond, ctx=None):
    srv = Server((address, port), Recording)
    srv.daemon_threads = True; srv.label = label; srv.respond = respond
    if ctx: srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
    threading.Thread(target=srv.serve_forever, daemon=True).start(); SERVERS.append(srv); return srv

def quiesce(polls=4, interval=.05, limit=15):
    """Wait until no listening socket has a pending connection, no handler is in flight and
    the log is unchanged over `polls` consecutive polls, so zero counts are final."""
    stable, last, deadline = 0, None, time.monotonic() + limit
    while time.monotonic() < deadline:
        pending = select.select([s.socket for s in SERVERS], [], [], 0)[0]
        with LOCK: state = (ACTIVE[0], len(LOG))
        stable = stable + 1 if not pending and state[0] == 0 and state == last else 0
        if stable >= polls: return
        last = state; time.sleep(interval)
    raise AssertionError(f'servers did not quiesce: active={ACTIVE[0]} log={len(LOG)}')

def json_reply(value, code=200): return code, {'Content-Type': 'application/json'}, json.dumps(value).encode()
def redirect(location): return 302, {'Location': location, 'Content-Type': 'text/plain'}, b'redirect fixture'

def api_respond(handler):
    """Synthetic Nexus API. The first path segment names the scenario."""
    url = urllib.parse.urlsplit(handler.path)
    parts = url.path.strip('/').split('/')
    scenario, rest = parts[0], '/'.join(parts[2:])  # /<scenario>/v1/<rest>
    if rest == 'users/validate.json': return json_reply({'name': 'fixture', 'is_premium': True})
    if rest.endswith('download_link.json'):
        hop = int(urllib.parse.parse_qs(url.query).get('hop', ['0'])[0])
        # Same-origin hops keep the request's other query parameters (an nxm key included).
        query = [(k, v) for k, v in urllib.parse.parse_qsl(url.query, keep_blank_values=True) if k != 'hop']
        same = f'{API}{url.path}?' + urllib.parse.urlencode(query + [('hop', hop + 1)], quote_via=urllib.parse.quote)
        if scenario == 'same' and hop < 2: return redirect(same)
        if scenario == 'loop': return redirect(same)
        if scenario == 'cross-host': return redirect(f'{CROSS_HOST}{url.path}')
        if scenario == 'cross-port': return redirect(f'{CROSS_PORT}{url.path}')
        if scenario == 'scheme': return redirect(f'https://127.0.0.1:{API_PORT}{url.path}')
        if scenario == 'alias': return redirect(f'http://localhost:{API_PORT}{url.path}')
        return json_reply([{'URI': ARCHIVE_URL}])
    if rest.startswith('games/'): return json_reply({'file_name': 'example-archive.zip'})
    return 404, {}, b'not expected'

def archive_respond(handler):
    return 200, {'Content-Type': 'application/octet-stream'}, ARCHIVE

def since(start, **match):
    return [r for r in LOG[start:] if all(r.get(k) == v for k, v in match.items())]

PAGE = 'https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1'
GUID = 'aaaaaaaa-1111-4111-8111-111111111111'

def execute(name, argv_tail, command='fetch-downloads', guide_url=PAGE, workspace=True):
    case = ROOT/name; case.mkdir(); mods = case/'mods'; mods.mkdir(); settings = case/'settings'; settings.mkdir()
    scratch = case/'scratch'; scratch.mkdir()
    (settings/'settings.json').write_text(json.dumps({'peerSharing': False, 'nexusModsApiKey': 'fixture-stored-secret'}))
    guide = case/'guide.json'
    meta = {'Files': {'example-archive.zip': True}, 'ContentHashSHA256': ARCHIVE_SHA} if command == 'fetch-downloads' else {}
    guide.write_text(json.dumps({'Components': [{'Guid': GUID, 'Name': 'Example Component', 'IsSelected': True,
                                                  'Instructions': [], 'ResourceRegistry': {guide_url: meta}}]}))
    if command == 'fetch-downloads':
        argv = [str(BIN), 'fetch-downloads', '--input', str(guide), '--source-dir', str(mods), '--scratch-dir', str(scratch),
                '--settings-dir', str(settings), '--peer-mode', 'off', '--retries', '0', '--extra-ca-file', str(CA)] + argv_tail
    else:
        argv = [str(BIN), 'nxm-download'] + [a.replace('{MODS}', str(mods)).replace('{GUIDE}', str(guide)).replace('{SETTINGS}', str(settings)) for a in argv_tail]
    start = len(LOG); began = time.monotonic()
    proc = subprocess.run(argv, capture_output=True, text=True, env=ENV, timeout=90)
    quiesce()  # every connection this run made is recorded and finished
    row = dict(name=name, argv=argv, exit=proc.returncode, stdout=proc.stdout, stderr=proc.stderr,
               elapsed=round(time.monotonic() - began, 3), guide=json.loads(guide.read_text()),
               observed=LOG[start:], published=sorted(p.name for p in mods.iterdir()))
    row['mods'] = mods; RUNS.append(row)
    (OUT/f'{name}.json').write_text(json.dumps({k: v for k, v in row.items() if k != 'mods'}, indent=2, default=str) + '\n')
    check(f'{name}_no_stored_secret', any('fixture-stored-secret' in json.dumps(r) for r in row['observed']), False)
    return row, start

def api_requests(start, label='api'): return since(start, kind='request', server=label)
def has_key(r): return any(k.lower() == 'apikey' for k, _ in r['headers'])
def key_values(rows): return sorted({v for r in rows for k, v in r['headers'] if k.lower() == 'apikey'})

def completed(row):
    check(row['name'] + '_exit', row['exit'], 0)
    check(row['name'] + '_published', row['published'], ['example-archive.zip'])
    path = row['mods']/'example-archive.zip'
    check(row['name'] + '_sha256', digest(path) if path.exists() else None, ARCHIVE_SHA)

def archive_clean(row, start, expected_requests):
    reqs = api_requests(start, 'archive')
    check(row['name'] + '_archive_requests', len(reqs), expected_requests)
    check(row['name'] + '_archive_no_apikey_header', [r['requestLine'] for r in reqs if has_key(r)], [])
    check(row['name'] + '_archive_no_key_value', any(API_KEY in json.dumps(r) for r in reqs), False)

def refused(row, start, destination, message='Refusing Nexus API redirect to another origin'):
    """`destination=None` when the refused target is not a separate recording server: the loop
    stays on `api`, and the host-alias and scheme-change targets are the `api` socket itself,
    so those cases assert on `api`'s own requests and TLS hellos instead."""
    check(row['name'] + '_failed', row['exit'] != 0)
    check(row['name'] + '_unpublished', row['published'], [])
    check(row['name'] + '_policy_message', message in row['stdout'] + row['stderr'])
    check(row['name'] + '_api_positive_contact', len(api_requests(start)) > 0)
    check(row['name'] + '_zero_archive_requests', api_requests(start, 'archive'), [])
    if destination:
        check(row['name'] + '_zero_destination_connections', since(start, kind='connection', server=destination), [])
        check(row['name'] + '_zero_destination_requests', api_requests(start, destination), [])
        check(row['name'] + '_zero_leaked_key', any(API_KEY in json.dumps(r) for r in LOG[start:] if r.get('server') == destination), False)

def decoded(name, raw):
    """Check the API saw exactly key/expires and both decoders recover the original key."""
    query = urllib.parse.urlsplit(raw).query
    check(name + '_raw_query', query, 'key=a%2Bb%2Fc%3Dd%26e%20f%25&expires=' + str(EXPIRES))
    check(name + '_form_decoded', urllib.parse.parse_qs(query, keep_blank_values=True, strict_parsing=True),
          {'key': [NXM_KEY], 'expires': [str(EXPIRES)]})
    strict = dict(pair.split('=', 1) for pair in query.split('&'))
    check(name + '_strict_decoded', {k: urllib.parse.unquote(v) for k, v in strict.items()},
          {'key': NXM_KEY, 'expires': str(EXPIRES)})

def main():
    global API, API_PORT, CROSS_HOST, CROSS_PORT, ARCHIVE_URL, CA
    error = None
    try:
        CA = ROOT/'ca.pem'; ca_key = ROOT/'ca.key'; leaf = ROOT/'leaf.pem'; leaf_key = ROOT/'leaf.key'; csr = ROOT/'leaf.csr'; ext = ROOT/'leaf.ext'
        ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
        commands = [['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(ca_key), '-out', str(CA), '-days', '1', '-subj', '/CN=Loopback Fixture CA',
                     '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
                    ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(leaf_key), '-out', str(csr), '-subj', '/CN=Loopback Server'],
                    ['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(CA), '-CAkey', str(ca_key), '-CAcreateserial', '-out', str(leaf), '-days', '1', '-extfile', str(ext)]]
        for cmd in commands: subprocess.run(cmd, check=True, capture_output=True)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); ctx.load_cert_chain(leaf, leaf_key)

        api = serve('127.0.0.1', 0, 'api', api_respond); API_PORT = api.server_port; API = f'http://127.0.0.1:{API_PORT}'
        cross_host = serve('127.0.0.2', API_PORT, 'cross-host', api_respond); CROSS_HOST = f'http://127.0.0.2:{API_PORT}'
        cross_port = serve('127.0.0.1', 0, 'cross-port', api_respond); CROSS_PORT = f'http://127.0.0.1:{cross_port.server_port}'
        archive = serve('127.0.0.1', 0, 'archive', archive_respond, ctx); ARCHIVE_URL = f'https://127.0.0.1:{archive.server_port}/example-archive.zip'
        https_api = serve('127.0.0.1', 0, 'https-api', api_respond, ctx)
        base = lambda scenario, origin=None: ['--nexus-api-base', f'{origin or API}/{scenario}/v1/', '--nexus-mods-api-key', API_KEY]

        # Positive controls: every destination server is reachable and records, and the API
        # port records a TLS ClientHello when one is actually sent to it.
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        for label, origin in [('cross-host', CROSS_HOST), ('cross-port', CROSS_PORT), ('api', API)]:
            start = len(LOG)
            body = opener.open(f'{origin}/plain/v1/users/validate.json', timeout=5).read()
            check(f'control_reachable_{label}', json.loads(body)['name'], 'fixture')
            check(f'control_recorded_{label}', len(api_requests(start, label)), 1)
        start = len(LOG)
        try:
            with socket.create_connection(('127.0.0.1', API_PORT), timeout=5) as raw:
                ssl.create_default_context(cafile=str(CA)).wrap_socket(raw, server_hostname='127.0.0.1')
        except (OSError, ssl.SSLError):
            pass
        quiesce()
        check('control_api_port_records_tls_hello', [r['tls'] for r in since(start, kind='connection', server='api')], [True])

        # Plain success: API + archive, archive carries no apikey.
        row, start = execute('plain_success', base('plain')); completed(row); archive_clean(row, start, 1)
        reqs = api_requests(start)
        check('plain_success_api_paths', [r['path'] for r in reqs],
              ['/plain/v1/users/validate.json', '/plain/v1/games/kotor/mods/1/files/1.json', '/plain/v1/games/kotor/mods/1/files/1/download_link.json'])
        check('plain_success_api_key_on_every_api_request', key_values(reqs), [API_KEY])
        check('plain_success_every_api_request_keyed', all(has_key(r) for r in reqs))

        # Production would contact, and send the key to, each destination when it is the base.
        for label, origin in [('cross-host', CROSS_HOST), ('cross-port', CROSS_PORT)]:
            row, start = execute(f'control_{label}_as_base', base('plain', origin)); completed(row); archive_clean(row, start, 1)
            check(f'control_{label}_as_base_contacted', len(api_requests(start, label)), 3)
            check(f'control_{label}_as_base_key_sent', key_values(api_requests(start, label)), [API_KEY])

        # Permitted same-origin redirects (two hops) succeed.
        row, start = execute('same_origin_redirect', base('same')); completed(row); archive_clean(row, start, 1)
        links = [r['path'] for r in api_requests(start) if 'download_link' in r['path']]
        check('same_origin_redirect_hops', links, ['/same/v1/games/kotor/mods/1/files/1/download_link.json',
                                                   '/same/v1/games/kotor/mods/1/files/1/download_link.json?hop=1',
                                                   '/same/v1/games/kotor/mods/1/files/1/download_link.json?hop=2'])
        check('same_origin_redirect_key_kept_same_origin', all(has_key(r) for r in api_requests(start)))

        # A same-origin loop stops at the hop limit.
        row, start = execute('same_origin_loop', base('loop')); refused(row, start, None, 'Too many Nexus API redirects')
        links = [r for r in api_requests(start) if 'download_link' in r['path']]
        # The original request plus NEXUS_API_REDIRECT_LIMIT (10) followed hops.
        check('same_origin_loop_request_count', len(links), 11)
        check('same_origin_loop_bounded_time', row['elapsed'] < 30)

        # A base without a trailing slash keeps its last path segment (`/noslash/v1`).
        row, start = execute('base_without_trailing_slash', ['--nexus-api-base', f'{API}/noslash/v1', '--nexus-mods-api-key', API_KEY])
        completed(row); archive_clean(row, start, 1)
        check('base_without_trailing_slash_api_paths', [r['path'] for r in api_requests(start)],
              ['/noslash/v1/users/validate.json', '/noslash/v1/games/kotor/mods/1/files/1.json', '/noslash/v1/games/kotor/mods/1/files/1/download_link.json'])

        # Cross-host, cross-port, host alias and scheme change are refused before contact.
        row, start = execute('cross_host_redirect', base('cross-host')); refused(row, start, 'cross-host')
        row, start = execute('cross_port_redirect', base('cross-port')); refused(row, start, 'cross-port')
        row, start = execute('alias_host_redirect', base('alias')); refused(row, start, None)
        # localhost resolves to this same API socket: a followed hop would be recorded here.
        check('alias_host_redirect_no_followed_request', [r['requestLine'] for r in api_requests(start) if 'download_link' in r['path']],
              ['GET /alias/v1/games/kotor/mods/1/files/1/download_link.json HTTP/1.1'])
        check('alias_host_redirect_no_localhost_host_header', any(v.startswith('localhost') for r in api_requests(start) for k, v in r['headers'] if k.lower() == 'host'), False)
        row, start = execute('scheme_change_redirect', base('scheme')); refused(row, start, None)
        check('scheme_change_redirect_zero_tls_hello', [r for r in since(start, kind='connection', server='api') if r['tls']], [])
        check('scheme_change_redirect_single_link_request', len([r for r in api_requests(start) if 'download_link' in r['path']]), 1)

        # Boundary: an HTTPS API base signed by the per-invocation CA is not trusted by the
        # API client (the extra CA is archive-only), so https->http downgrade cannot be
        # driven through the existing CLI. Recorded to make that boundary explicit.
        row, start = execute('https_api_base_boundary', base('plain', f'https://127.0.0.1:{https_api.server_port}'))
        check('https_api_base_boundary_failed', row['exit'] != 0)
        check('https_api_base_boundary_no_request', api_requests(start, 'https-api'), [])
        check('https_api_base_boundary_tls_rejected_by_client', len(since(start, kind='tls-handshake-failed', server='https-api')) > 0)
        check('https_api_base_boundary_certificate', 'certificate' in (row['stdout'] + row['stderr']).lower())

        # Reserved characters in the nxm key reach the API exactly (direct client path).
        nxm = f'nxm://kotor/mods/1/files/1?key={urllib.parse.quote(NXM_KEY, safe="")}&expires={EXPIRES}'
        check('nxm_input_url', nxm, f'nxm://kotor/mods/1/files/1?key=a%2Bb%2Fc%3Dd%26e%20f%25&expires={EXPIRES}')
        row, start = execute('nxm_key_direct', [nxm, '{MODS}/example-archive.zip'] + base('keyed'), command='nxm')
        links = [r for r in api_requests(start) if 'download_link' in r['path']]
        check('nxm_key_direct_one_link_request', len(links), 1)
        if links: decoded('nxm_key_direct', links[0]['path'])
        # The archive transfer then fails: this command has no per-invocation CA option.
        check('nxm_key_direct_archive_tls_only', api_requests(start, 'archive'), [])
        check('nxm_key_direct_failed_at_archive_certificate', row['exit'] != 0 and 'certificate' in (row['stdout'] + row['stderr']).lower())

        # A keyed download_link request that loops or is refused never prints the key.
        for name, scenario, message, destination in [
                ('nxm_key_loop', 'loop', 'Too many Nexus API redirects', None),
                ('nxm_key_cross_host', 'cross-host', 'Refusing Nexus API redirect to another origin', 'cross-host')]:
            row, start = execute(name, [nxm, '{MODS}/example-archive.zip'] + base(scenario), command='nxm')
            refused(row, start, destination, message)
            links = [r for r in api_requests(start) if 'download_link' in r['path']]
            check(name + '_key_reached_api', len(links) > 0 and all('key=a%2Bb%2Fc%3Dd%26e%20f%25' in r['path'] for r in links))
            check(name + '_link_request_count', len(links), 11 if scenario == 'loop' else 1)
            check(name + '_no_key_in_output', [f for f in NXM_KEY_FORMS if f in row['stdout'] + row['stderr']], [])

        # Same key through the shared acquisition path (nxm_target -> nxm_url -> run).
        row, start = execute('nxm_key_orchestrated', [nxm, '{MODS}', '--input', '{GUIDE}'] + base('keyed'), command='nxm',
                             guide_url='https://www.nexusmods.com/kotor/mods/1')
        links = [r for r in api_requests(start) if 'download_link' in r['path']]
        check('nxm_key_orchestrated_one_link_request', len(links), 1)
        if links: decoded('nxm_key_orchestrated', links[0]['path'])
        check('nxm_key_orchestrated_no_validate', [r['path'] for r in api_requests(start) if 'validate' in r['path']], [])
        check('nxm_key_orchestrated_failed_at_archive_certificate', row['exit'] != 0 and 'certificate' in (row['stdout'] + row['stderr']).lower())
    except Exception:
        error = traceback.format_exc(); print(error, flush=True)
    finally:
        for s in SERVERS: s.shutdown(); s.server_close()
        check('source_frozen', SOURCE, {str(p.relative_to(REPO)): digest(p) for p in source_files()})
        check('binary_frozen', BINARY, digest(BIN))
        check('git_head_frozen', GIT_HEAD, git('rev-parse', 'HEAD').strip())
        receipt = dict(binary=str(BIN), binarySha256=BINARY, binaryMtimeNs=BIN.stat().st_mtime_ns, sourceSha256=SOURCE,
                       gitHead=GIT_HEAD, gitStatusPorcelain=GIT_STATUS, proofSha256=digest(__file__),
                       namespace=os.readlink('/proc/self/ns/net'), parentNamespace=PARENT,
                       routes=subprocess.check_output(['ip', 'route'], text=True),
                       archiveSha256=ARCHIVE_SHA, apiKeyFixture=API_KEY, nxmKey=NXM_KEY,
                       cases=[r['name'] for r in RUNS], checks=CHECKS, error=error,
                       passed=sum(c['passed'] for c in CHECKS), total=len(CHECKS), log=LOG)
        (OUT/'receipt.json').write_text(json.dumps(receipt, indent=2, default=str) + '\n')
        print(json.dumps(dict(passed=receipt['passed'], total=receipt['total'], error=error,
                              failed=[c['name'] for c in CHECKS if not c['passed']], out=str(OUT))), flush=True)
        shutil.rmtree(ROOT)
    return 0 if error is None and all(c['passed'] for c in CHECKS) else 1

if __name__ == '__main__':
    sys.exit(main())
