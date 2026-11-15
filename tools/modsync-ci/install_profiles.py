"""Closed install selections for the complete pinned 197/151 source definitions.

Profiles change IsSelected only. They never truncate guides, drop definitions,
mutate ordering or turn a reduced profile into a claim of exhaustive coverage.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import tomllib

POLICY = 'pinned-guide-install-profiles-v1'
POLICY_SHA256 = '4e1c86071620044d63a0f80dc917f30825031b5d3d5de82a2d47eb8b8f509b73'
PROFILES = ('minimal', 'full', 'exhaustive')
_ASPYR_4GB = '7c3bc4e3-7afd-8e2c-9aec-17527238184e'
_SHA = re.compile(r'[0-9a-f]{64}')


def _require(condition, code):
    if not condition:
        raise ValueError('install_profile_' + code)


def _policy():
    source = Path(__file__).resolve().parent
    body = (source / 'install_profiles.json').read_bytes()
    _require(hashlib.sha256(body).hexdigest() == POLICY_SHA256, 'policy_identity_changed')
    data = json.loads(body)
    _require(data['schema'] == 1 and data['policy'] == POLICY, 'policy_schema')
    for game, count in (('k1', 197), ('k2', 151)):
        cfg = data['games'][game]
        owners = cfg['owners']
        _require(cfg['source_count'] == len(owners) == count
                 and len({o['guid'] for o in owners}) == count, 'source_inventory')
        guide = source.parents[1] / 'src/archive_generation' / ('source_plan_' + game + '.md')
        plan = source / 'fixtures/ci' / (game + '-manual-plan.json')
        _require(hashlib.sha256(guide.read_bytes()).hexdigest() == cfg['guide_sha256']
                 and hashlib.sha256(plan.read_bytes()).hexdigest() == cfg['plan_sha256'], 'source_pins')
        ids = {o['guid'] for o in owners}
        _require(set(cfg['protected_guids']) <= ids
                 and not set(cfg['protected_guids']) & {e['guid'] for e in cfg['exclusions']}, 'protected_prerequisites')
        _require(all(set(o['dependencies']) <= ids for o in owners), 'dependency_identity')
    return data


def profile_scope(game, profile='minimal'):
    """Pre-acquisition scope; source items shared by selected owners stay required.

    Stage may exclude an item only when ALL its referencing steps are omitted.
    Required compatibility hashes/source IDs override any heading-only omission.
    """
    _require(game in ('k1', 'k2') and profile in PROFILES, 'name')
    data = _policy()
    cfg = data['games'][game]
    excluded = {e['guid']: dict(e) for e in cfg['exclusions'] if profile in e['profiles']}
    platform = set(cfg['platform_default_inactive_guids'])
    omitted = set(excluded) | platform
    # Removing a prerequisite also removes every dependent, never a partial recipe.
    changed = True
    while changed:
        changed = False
        for owner in cfg['owners']:
            if owner['guid'] not in omitted and set(owner['dependencies']) & omitted:
                _require(owner['guid'] not in cfg['protected_guids'], 'protected_dependency_excluded')
                excluded[owner['guid']] = {'guid': owner['guid'], 'heading': owner['heading'],
                    'kind': 'dependent-of-profile-exclusion', 'profiles': [profile],
                    'caused_by': sorted(set(owner['dependencies']) & omitted)}
                omitted.add(owner['guid'])
                changed = True
    selected = [o['guid'] for o in cfg['owners'] if o['guid'] not in omitted]
    selected_set = set(selected)
    compatibility = [dict(row) for row in data['compatibility_sources'] if row['owner_guid'] in selected_set]
    source_ids = {source_id for owner in cfg['owners'] if owner['guid'] in selected_set
                  for source_id in owner['source_ids']}
    source_ids.update(row['source_id'] for row in compatibility)
    acquisition_ids = {source_id for owner in cfg['owners'] if owner['guid'] in selected_set
                       for source_id in owner['acquisition_source_ids']}
    acquisition_ids.update(row['source_id'] for row in compatibility)
    platform_sources = {source_id for owner in cfg['owners'] if owner['guid'] in platform
                        for source_id in owner['acquisition_source_ids']} - acquisition_ids
    by_id = {o['guid']: o for o in cfg['owners']}
    return {'schema_version': 1, 'policy': POLICY, 'policy_sha256': POLICY_SHA256,
        'game': game, 'profile': profile, 'source_guide_sha256': cfg['guide_sha256'],
        'source_plan_sha256': cfg['plan_sha256'], 'source_definition_count': cfg['source_count'],
        'original_guids': [o['guid'] for o in cfg['owners']], 'selected_guids': selected,
        'selected_count': len(selected), 'excluded_guids': sorted(omitted),
        'profile_excluded_guids': sorted(excluded), 'platform_inactive_guids': sorted(platform),
        'platform_inactive_source_ids': sorted(platform_sources),
        'excluded_headings': [o['heading'] for o in cfg['owners'] if o['guid'] in omitted],
        'exclusions': [excluded[o['guid']] for o in cfg['owners'] if o['guid'] in excluded],
        'source_ids': sorted(source_ids),
        'acquisition_source_ids': sorted(acquisition_ids),
        'non_archive_provenance_source_ids': sorted(source_ids - acquisition_ids),
        'required_compatibility_input_shas': sorted({row['sha256'] for row in compatibility}),
        'required_compatibility_source_ids': sorted({row['source_id'] for row in compatibility}),
        'required_compatibility_inputs': compatibility,
        'requires_k1_widescreen_inputs': game == 'k1' and '6c3c7165-297e-80eb-b9f8-d47506dab2d5' in selected_set,
        'requires_k2_main_menu_inputs': game == 'k2' and '14d721c3-67f9-885b-9736-04282d41bed0' in selected_set,
        'protected_guids': cfg['protected_guids'],
        'size_evidence': data['size_evidence']}


def plan(profile, game):
    return profile_scope(game, profile)


def _model(text, game):
    cfg = _policy()['games'][game]
    document = tomllib.loads(text)
    key = 'thisMod' if 'thisMod' in document else 'Components'
    components = document.get(key)
    _require(isinstance(components, list)
             and [c.get('Guid') for c in components] == [o['guid'] for o in cfg['owners']]
             and all(c.get('Heading', c.get('Name')) == o['heading']
                     for c, o in zip(components, cfg['owners'])), 'complete_definition_inventory_changed')
    return document, key, components


def apply(converted, game, profile='minimal', *, baseline_applicability=None):
    """Retain every source definition and order, setting only root IsSelected."""
    scope = profile_scope(game, profile)
    original = Path(converted).read_bytes()
    text = original.decode('utf-8')
    document, key, components = _model(text, game)
    platform = set(scope['platform_inactive_guids'])
    if baseline_applicability is not None:
        import fullrun_guide_choices
        _require(game == 'k2', 'unexpected_baseline_applicability')
        fullrun_guide_choices.validate_baseline_applicability(
            baseline_applicability, hashlib.sha256(original).hexdigest())
        platform.add(_ASPYR_4GB)
    excluded = set(scope['profile_excluded_guids']) - platform
    selected = set(scope['original_guids']) - excluded - platform
    _require(set(scope['protected_guids']) <= selected, 'protected_owner_deselected')
    owners = _policy()['games'][game]['owners']
    _require(all(not (set(o['dependencies']) - selected) for o in owners if o['guid'] in selected),
             'selected_dependency_missing')
    header = '[[' + key + ']]'
    index, inside, seen, lines = -1, False, 0, []
    for line in text.splitlines(keepends=True):
        stripped = line.strip()
        if stripped.startswith('['):
            inside = stripped == header
            if inside:
                index += 1
        elif inside and re.fullmatch(r'IsSelected\s*=\s*(true|false)', stripped):
            line = 'IsSelected = ' + ('true' if components[index]['Guid'] in selected else 'false') + '\n'
            seen += 1
        lines.append(line)
    _require(index + 1 == len(components) == seen, 'selection_rewrite_incomplete')
    rewritten = ''.join(lines)
    updated, _, updated_components = _model(rewritten, game)
    expected = [c['Guid'] in selected for c in components]
    _require([c.get('IsSelected') for c in updated_components] == expected, 'selection_rewrite_mismatch')
    for model in (document, updated):
        for component in model[key]:
            component.pop('IsSelected', None)
    _require(document == updated, 'rewrite_changed_nonselection_fields')
    Path(converted).write_text(rewritten, encoding='utf-8')
    receipt = {**scope, 'claim': profile + ' selected install profile; complete source definitions retained',
        'original_input_sha256': hashlib.sha256(original).hexdigest(),
        'admitted_input_sha256': hashlib.sha256(rewritten.encode()).hexdigest(),
        'selected_guids': [g for g in scope['original_guids'] if g in selected],
        'selected_count': len(selected), 'excluded_guids': sorted(excluded),
        'platform_inactive_guids': sorted(platform), 'retained_definition_count': len(components)}
    if baseline_applicability is not None:
        receipt['baseline_applicability'] = baseline_applicability
    validate_receipt(receipt, expected_profile=profile)
    return receipt


def validate_receipt(receipt, expected_profile=None):
    """Return exactly admitted GUIDs after authenticating policy and partition."""
    _require(isinstance(receipt, dict), 'receipt_schema')
    scope = profile_scope(receipt.get('game'), receipt.get('profile'))
    _require(expected_profile is None or receipt['profile'] == expected_profile, 'unexpected_profile')
    for key in ('schema_version', 'policy', 'policy_sha256', 'source_guide_sha256',
                'source_plan_sha256', 'source_definition_count', 'original_guids'):
        _require(receipt.get(key) == scope[key], 'receipt_identity_' + key)
    platform = set(scope['platform_inactive_guids'])
    baseline = receipt.get('baseline_applicability')
    if baseline is not None:
        import fullrun_guide_choices
        _require(scope['game'] == 'k2', 'receipt_baseline_game')
        fullrun_guide_choices.validate_baseline_applicability(baseline, receipt.get('original_input_sha256'))
        platform.add(_ASPYR_4GB)
    excluded = set(scope['profile_excluded_guids']) - platform
    selected = [g for g in scope['original_guids'] if g not in excluded | platform]
    _require(receipt.get('selected_guids') == selected and receipt.get('selected_count') == len(selected)
             and receipt.get('excluded_guids') == sorted(excluded)
             and receipt.get('platform_inactive_guids') == sorted(platform)
             and receipt.get('retained_definition_count') == scope['source_definition_count'], 'receipt_partition')
    _require(all(isinstance(receipt.get(key), str) and _SHA.fullmatch(receipt[key])
                 for key in ('original_input_sha256', 'admitted_input_sha256')), 'receipt_input_hash')
    _require(receipt.get('exclusions') == scope['exclusions'], 'receipt_exclusion_provenance')
    return set(selected)
