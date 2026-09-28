#!/usr/bin/env python3
"""Read existing conversion outputs; emit anonymous summary. Never runs an engine.
Usage: python3 generate_summary.py BASELINE_DIRECTORY OUTPUT_JSON
Do not supply original guide text. Baseline JSON and receipts remain external.
"""
import collections
import hashlib
import json
import pathlib
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def nodes(items, prefix=''):
    for ordinal, item in enumerate(items, 1):
        path = f'{prefix}component[{ordinal}]'
        yield path, item
        yield from nodes(item.get('Options', []), path + '/option/')


def normalize(value, guids):
    if isinstance(value, str):
        return guids.get(value.lower(), value)
    if isinstance(value, list):
        return [normalize(v, guids) for v in value]
    if isinstance(value, dict):
        return {k: normalize(v, guids) for k, v in value.items()}
    return value


def reconstructed_instruction(item, generation_intent):
    # Both models default missing Overwrite to true. C# drafter instead sets
    # Delete false, then its serializer omits that field. These are DIFFERENT
    # interpretations, deliberately emitted separately; neither rewrites inputs.
    defaults = {'Destination': '', 'Arguments': '', 'Overwrite': True,
                'Dependencies': [], 'Restrictions': []}
    if generation_intent and item.get('Action') == 'Delete':
        defaults['Overwrite'] = False
    return {**defaults, **item}


def main():
    baseline, destination = map(pathlib.Path, sys.argv[1:])
    receipt = json.loads((baseline / 'receipt.json').read_text())
    output = {
        'schema': 1,
        'scope': 'Existing conversion output comparison; no engine execution, reload or installation.',
        'identityRule': 'Map generated GUIDs to ordinal component/option tree paths only after exact Name and Author alignment.',
        'sourceBindings': {'receiptSha256': sha(baseline / 'receipt.json'),
                           'rustBinarySha256': receipt['rustSha256'],
                           'csharpDllSha256': receipt['csharpSha256']},
        'pairs': {},
    }
    for game in ('k1', 'k2'):
        for mode in ('plain', 'draft'):
            label = f'{game}-{mode}'
            data, proofs = {}, {}
            for engine in ('rust', 'csharp'):
                name = f'{game}-{engine}-{mode}'
                path = baseline / (name + '.json')
                rp = baseline / (name + '.receipt.json')
                r = json.loads(rp.read_text())
                assert r['exit'] == 0 and sha(path) == r['outputSha256'], 'Receipt mismatch'
                data[engine] = json.loads(path.read_text())
                proofs[engine] = {'commandEvidence': rp.name, 'exit': r['exit'],
                                  'receiptSha256': sha(rp), 'inputSha256': r['inputSha256'],
                                  'outputSha256': sha(path)}
            assert proofs['rust']['inputSha256'] == proofs['csharp']['inputSha256'], 'Different inputs'
            left, right = (dict(nodes(data[e])) for e in ('rust', 'csharp'))
            assert left.keys() == right.keys(), 'Tree mismatch'
            assert all(left[p].get(k) == right[p].get(k) for p in left for k in ('Name', 'Author')), 'Ordinal identity mismatch'
            lm = {v['Guid'].lower(): p for p, v in left.items()}
            rm = {v['Guid'].lower(): p for p, v in right.items()}
            assert len(lm) == len(left) and len(rm) == len(right), 'Duplicate GUID'
            counts = collections.Counter()
            semantic = {'modelDefault': [], 'draftGenerationReconstruction': []}
            actions = {e: collections.Counter() for e in data}
            for path in left:
                a, b = normalize(left[path], lm), normalize(right[path], rm)
                for field in sorted(a.keys() | b.keys()):
                    if field in ('Instructions', 'Options'):
                        continue
                    if field not in a or field not in b:
                        side = 'rustOnly' if field in a else 'csharpOnly'
                        v = a[field] if field in a else b[field]
                        empty = v is None or v == '' or v == [] or v == {}
                        counts[f'{field}:{side}:{"empty" if empty else "nonempty"}'] += 1
                    elif a[field] != b[field]:
                        if field == 'ResourceRegistry':
                            assert a[field].keys() == b[field].keys(), 'Resource identity mismatch'
                            for key in a[field]:
                                x, y = a[field][key], b[field][key]
                                for sub in x.keys() | y.keys():
                                    if sub not in x or sub not in y:
                                        side = 'rustOnly' if sub in x else 'csharpOnly'
                                        counts[f'ResourceRegistry.{sub}:{side}'] += 1
                                    elif x[sub] != y[sub]:
                                        counts[f'ResourceRegistry.{sub}:sharedValueDifference'] += 1
                        else:
                            counts[f'{field}:sharedValueDifference'] += 1
                assert len(a['Instructions']) == len(b['Instructions']), 'Instruction count mismatch'
                for ordinal, (x, y) in enumerate(zip(a['Instructions'], b['Instructions']), 1):
                    for engine, item in (('rust', x), ('csharp', y)):
                        actions[engine][item['Action']] += 1
                    for field in x.keys() | y.keys():
                        if field not in x or field not in y:
                            counts[f'Instructions.{field}:rawPresenceDifference'] += 1
                        elif x[field] != y[field]:
                            counts[f'Instructions.{field}:sharedValueDifference'] += 1
                    for generation, key in ((False, 'modelDefault'), (True, 'draftGenerationReconstruction')):
                        xx, yy = (reconstructed_instruction(v, generation) for v in (x, y))
                        fields = sorted(k for k in xx.keys() | yy.keys() if xx.get(k) != yy.get(k))
                        if fields:
                            semantic[key].append({'path': f'{path}/instruction[{ordinal}]', 'fields': fields})
            roots = data['rust']
            output['pairs'][label] = {
                'proofs': proofs, 'components': len(roots), 'optionNodes': len(left) - len(roots),
                'ordinalNameAuthorAlignment': True,
                'rawGuidDifferences': sum(left[p]['Guid'] != right[p]['Guid'] for p in left),
                'resources': sum(len(v['ResourceRegistry']) for v in roots),
                'instructionActionCounts': {k: dict(v) for k, v in actions.items()},
                'rawFieldDifferencesAfterGuidMapping': dict(sorted(counts.items())),
                'instructionComparisons': semantic,
                'rustOnlyPreservedMetadata': {field: [i for i, c in enumerate(roots, 1) if field in c]
                                             for field in ('NameFieldContent', 'UnresolvedMasters', 'DraftReview')},
            }
    destination.write_text(json.dumps(output, indent=2, sort_keys=True) + '\n')
    print('Wrote anonymous summary for four pairs; all eight receipt hashes verified.')


if __name__ == '__main__':
    main()
