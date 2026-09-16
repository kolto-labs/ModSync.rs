"""Linux differential format check against a separately built ModSync C# executable."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert shutil.which('bwrap'), 'Install bubblewrap to isolate C# cache, settings, and network access'
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
reference = args.reference_dll.resolve(strict=True)
rust = args.rust.resolve(strict=True)
fixture = Path(__file__).resolve().parents[1] / 'tests/fixtures/guide.json'
sandbox = ['bwrap', '--ro-bind', '/', '/', '--dev', '/dev', '--tmpfs', '/tmp',
           '--tmpfs', str(Path.home() / '.local/share'), '--tmpfs', str(Path.home() / '.config'),
           '--bind', str(output), str(output), '--unshare-net', '--die-with-parent']


def run(command, label):
    result = subprocess.run(command, cwd=output, capture_output=True, timeout=120)
    log = output / (label + '.log')
    log.write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, f'{label} failed: {log}'


def csharp(source, target, format):
    run(sandbox + ['dotnet', str(reference), 'convert', '--input', str(source), '--output', str(target),
                   '--format', format, '--plaintext', '--non-interactive'], target.stem)


def project(components):
    """Compare install-relevant fields; this deliberately does not certify resource migration."""
    def instruction(item):
        action = item['Action']
        return {key: item.get(key, default) for key, default in {
            'Action': '', 'Source': [], 'Destination': '', 'Arguments': '',
            'Overwrite': action != 'Delete', 'Dependencies': [], 'Restrictions': []}.items()}
    return [{**{key: item.get(key, default) for key, default in {
        'Guid': '', 'Name': '', 'Category': [], 'IsSelected': False,
        'Dependencies': [], 'Restrictions': [], 'InstallBefore': [], 'InstallAfter': []}.items()},
        'Instructions': [instruction(i) for i in item.get('Instructions', [])],
        'Options': project(item.get('Options', []))} for item in components]


control = output / 'control.json'
csharp(fixture, control, 'json')
expected = project(json.loads(control.read_text()))
results = []
for format in ['toml', 'json', 'yaml', 'xml']:
    cs = output / ('csharp.' + format)
    rs = output / ('rust.' + format)
    read_rs = output / ('csharp-read-rust-' + format + '.json')
    read_cs = output / ('rust-read-csharp-' + format + '.json')
    csharp(fixture, cs, format)
    run([str(rust), 'convert', str(fixture), str(rs)], 'rust-write-' + format)
    csharp(rs, read_rs, 'json')
    run([str(rust), 'convert', str(cs), str(read_cs)], 'rust-read-' + format)
    assert project(json.loads(read_rs.read_text())) == expected, f'C# lost or changed {format} data'
    assert project(json.loads(read_cs.read_text())) == expected, f'Rust lost or changed {format} data'
    results.append({'format': format, 'csharp_reads_rust': True, 'rust_reads_csharp': True})
    print(f'{format}: both directions preserve compared fields', flush=True)
receipt = {'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(),
           'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(), 'checks': results,
           'scope': 'Component identity, category, selection, ordering constraints, instructions, options. Resource metadata and build metadata are not certified.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
