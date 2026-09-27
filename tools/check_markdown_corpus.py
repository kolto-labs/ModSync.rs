"""Full-corpus Markdown field differential against a separately built ModSync C# Core DLL.

Runs the reference `convert --format json` (sandboxed, no network) and the Rust
`convert` on every `*.md` guide under the corpus `content/k1` and `content/k2`
folders, then compares every key of each reference component object except its
random GUID. Dependency GUIDs are compared as component indices.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, help='Rust CLI; omit with --reuse-rust')
parser.add_argument('--corpus', type=Path, required=True, help='mod-builds checkout')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--reuse-csharp', action='store_true', help='keep existing C# outputs')
parser.add_argument('--reuse-rust', action='store_true', help='compare existing Rust outputs only')
args = parser.parse_args()
assert shutil.which('bwrap'), 'Install bubblewrap to isolate C# cache, settings, and network access'
output = args.output.resolve()
(output / 'csharp').mkdir(parents=True, exist_ok=True)
(output / 'rust').mkdir(parents=True, exist_ok=True)
reference = args.reference_dll.resolve(strict=True)
assert args.reuse_rust or args.rust, '--rust is required unless --reuse-rust is set'
rust = None if args.reuse_rust else args.rust.resolve(strict=True)
sandbox = ['bwrap', '--ro-bind', '/', '/', '--dev', '/dev', '--tmpfs', '/tmp',
           '--tmpfs', str(Path.home() / '.local/share'), '--tmpfs', str(Path.home() / '.config'),
           '--bind', str(output), str(output), '--unshare-net', '--die-with-parent']


def run(command, log):
    result = subprocess.run(command, cwd=output, capture_output=True, timeout=1800)
    log.write_bytes(result.stdout + result.stderr)
    return result.returncode


def load(path):
    return json.loads(path.read_text()) if path.exists() else None


def project(components):
    ids = {c.get('Guid'): i for i, c in enumerate(components)}
    result = []
    for component in components:
        item = {}
        for key, value in component.items():
            if key == 'Guid':
                continue
            if key in ('Dependencies', 'Restrictions', 'InstallBefore', 'InstallAfter'):
                value = [ids.get(v, v) for v in value]
            elif key == 'ResourceRegistry':
                value = list(value)
            item[key] = value
        result.append(item)
    return result


def default(value):
    return {list: [], str: '', bool: False, dict: {}}.get(type(value))


def empty(value):
    return value in ('', [], {}, None, False)


def compare(cs, rs):
    cs, rs = project(cs), project(rs)
    fields = sorted({k for c in cs for k in c})
    mismatches, per_field, matching = [], {}, 0
    non_empty, non_empty_per_field, rust_missing = 0, {}, {}
    for index in range(max(len(cs), len(rs))):
        if index >= len(cs) or index >= len(rs):
            mismatches.append({'component_index': index, 'field': '<component count>'})
            continue
        ok = True
        for field in fields:
            if not empty(cs[index].get(field)):
                non_empty += 1
                non_empty_per_field[field] = non_empty_per_field.get(field, 0) + 1
            if field not in rs[index]:
                # Rust omits empty keys; the C# empty value is substituted and counted here.
                rust_missing[field] = rust_missing.get(field, 0) + 1
            if field not in cs[index] and field not in rs[index]:
                # Absent on both sides proves nothing, so it is a difference.
                left, right = '<missing>', '<missing>'
            else:
                left = cs[index].get(field)
                right = rs[index].get(field, default(left))
            if left == '<missing>' or left != right:
                ok = False
                per_field[field] = per_field.get(field, 0) + 1
                mismatches.append({'component_index': index, 'name': cs[index].get('Name'),
                                   'field': field, 'csharp': left, 'rust': right})
        matching += ok
    compared = len(fields) * min(len(cs), len(rs))
    return {'csharp_components': len(cs), 'rust_components': len(rs),
            'components_fully_matching': matching, 'field_values_compared': compared,
            'field_values_matching': compared - sum(per_field.values()),
            'csharp_non_empty_values': non_empty, 'csharp_non_empty_per_field': non_empty_per_field,
            'rust_missing_keys_filled_empty': rust_missing,
            'mismatches_per_field': per_field, 'mismatches': mismatches}


results = {}
guides = sorted((args.corpus / 'content').glob('k[12]/*.md'))
for guide in guides:
    name = f'{guide.parent.name}-{guide.stem}'
    cs_out, rs_out = output / 'csharp' / f'{name}.json', output / 'rust' / f'{name}.json'
    if not (args.reuse_csharp and cs_out.exists()):
        run(sandbox + ['dotnet', str(reference), 'convert', '--input', str(guide), '--output', str(cs_out),
                       '--format', 'json', '--plaintext', '--non-interactive'], cs_out.with_suffix('.log'))
    if rust is None:
        rust_code = 0 if rs_out.exists() else 1
    else:
        rust_code = run([str(rust), 'convert', str(guide), str(rs_out)], rs_out.with_suffix('.log'))
    cs, rs = load(cs_out), load(rs_out) if rust_code == 0 else None
    entry = {'sha256': hashlib.sha256(guide.read_bytes()).hexdigest(), 'rust_exit': rust_code}
    if cs is not None and rs is not None:
        entry.update(compare(cs, rs))
    else:
        entry.update({'csharp_components': None if cs is None else len(cs), 'rust_components': None})
    results[name] = entry
    print(name, {k: entry.get(k) for k in ('csharp_components', 'rust_components', 'rust_exit',
                                           'components_fully_matching', 'field_values_matching',
                                           'field_values_compared', 'csharp_non_empty_values')}, flush=True)
receipt = {'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(), 'guides': results,
           'scope': 'Component-list JSON from reference convert: every emitted component key except random GUIDs. '
                    'Keys absent from Rust output are compared as the empty value of the C# type; Rust-only keys '
                    '(NameFieldContent, UnresolvedMasters) are not compared. Parity covers this corpus only. '
                    'Preamble, epilogue, widescreen-warning metadata, drafted instructions and installs are not covered.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=1, ensure_ascii=False) + '\n')
