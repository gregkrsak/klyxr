#!/usr/bin/env python3
"""Check completeness and referential integrity of the Stage I evidence ledger.

This is a manifest check, not execution evidence or a semantic proof. Cargo
identity reconciliation and exact numeric neighbors have separate CI checks.
"""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
p = json.loads((root / 'docs/engineering/ked-035-evidence.json').read_text())
k34 = json.loads((root / 'docs/engineering/ked-034-evidence.json').read_text())
pre = json.loads((root / 'docs/engineering/ked-034-predecessors.json').read_text())
known = {x['id'] for x in pre['identities']} | set(k34['additions']) | set(p['additions'])
assert p['baseline_commit'] == '5f28977a42a7df1817e5cac46b3f6660a38435d3'
assert p['baseline_tree'] == '0b330328f3f213a26b6fc4af9f45ca06a98876b8'
assert len(known) == 2045 + len(p['additions'])
assert set(p['requirements']) == {str(i) for i in range(4, 19)}
assert set(p['semantic_obligations']) == {f'15.{i:02}' for i in range(1, 31)}
assert set(p['constraints']) == {f'K35-P{i:02}' for i in range(1, 11)}
assert set(p['milestones']) == {f'M{i}' for i in range(10)}
for group in ['requirements', 'semantic_obligations', 'constraints', 'malformed_families']:
    for key, row in p[group].items():
        assert row['description'] and row['status'], (group, key)
        deferred = 'deferred Stage N' in row['status']
        script_only = key == '15.29'
        assert row['tests'] or deferred or script_only, (group, key)
        assert len(row['tests']) == len(set(row['tests'])), (group, key)
        assert set(row['tests']) <= known, (group, key, 'unknown test')
        assert row['code'] or deferred, (group, key)
        for path in row['code']:
            assert (root / path).is_file(), (group, key, path)
assert p['constraints']['K35-P10']['status'] == 'deferred Stage N'
for key in ['15.23', '15.24']:
    assert p['semantic_obligations'][key]['status'] == 'deferred Stage N'
assert set(p['classifications']) == set(p['additions'])
# Every new identity must name an actual test function, independently of Cargo.
# Cargo CI additionally checks its exact module/target identity and pass status.
text = '\n'.join(path.read_text() for path in (root / 'compiler').rglob('*.rs'))
for identity in p['additions']:
    assert 'fn ' + identity.rsplit('::', 1)[-1] + '(' in text, identity
assert p['m5_checkpoint']['commit'] == 'e31f6104d8222ee201bc392a3daf87bcb03734ab'
assert p['m5_checkpoint']['tree'] == '34466f3032475e6b03931f59e61e712fab3b54e2'
assert p['stage_n_deferred'] and p['limitations'] and p['public_api'] and p['boundedness']
ledger = (root / 'docs/engineering/KED-035-STAGE-I-LEDGER.md').read_text()
assert all(f'K35-P{i:02}' in ledger for i in range(1, 11))
assert all(f'M{i}' in ledger for i in range(10))
print('KED-035 map: frozen sections 4–18; 30 obligations; P01–P10; M0–M9; named malformed families')
print(f'2045 retained identities + {len(p["additions"])} classified additions; native evidence explicitly deferred')
