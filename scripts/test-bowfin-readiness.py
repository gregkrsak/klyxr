#!/usr/bin/env python3
"""Private standard-library tests of the readiness gate, never source compatibility tests."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("readiness", Path(__file__).with_name("verify-bowfin-readiness.py"))
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def mapping(kind="engineering_decision", old="KED-035", new="BED-035"):
    return {"id": kind + ":" + old, "kind": kind, "legacy": old,
            "canonical": new, "phase": "M4", "policy": "explicit mapping only",
            "validation": "test_namespace_separation"}


class GateTests(unittest.TestCase):
    def setUp(self):
        raw, _ = gate.occurrences("example.txt", b"KED-035")
        self.raw = raw[0]
        self.row = dict(self.raw, role="historical", mapping="engineering_decision:KED-035",
                        phase="M4",
                        consumer="review", producer="exact original file",
                        reason="original recorded decision reference",
                        historical_relationship="accepted baseline blob, not reissued evidence")
        self.observed = {self.raw["id"]: self.raw}

    def locations(self, row=None, observed=None):
        return gate.validate_locations([row or self.row], observed or self.observed,
                                       {"engineering_decision:KED-035"}, {"review"})

    def test_namespace_separation(self):
        gate.validate_mapping([mapping(), mapping("language_decision", "KD-035", "BD-035")])

    def test_duplicate_mapping(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            gate.validate_mapping([mapping(), mapping()])

    def test_noninjective_mapping(self):
        a = mapping("brand", "Klyxr", "Bowfin")
        b = mapping("brand", "KLYXR", "Bowfin")
        with self.assertRaisesRegex(ValueError, "non-injective"):
            gate.validate_mapping([a, b])

    def test_engineering_cannot_be_language(self):
        with self.assertRaisesRegex(ValueError, "KED/BED"):
            gate.validate_mapping([mapping(new="BD-035")])

    def test_language_cannot_be_engineering(self):
        with self.assertRaisesRegex(ValueError, "KD/BD"):
            gate.validate_mapping([mapping("language_decision", "KD-035", "BED-035")])

    def test_numeric_suffix_is_not_identity(self):
        with self.assertRaisesRegex(ValueError, "KED/BED"):
            gate.validate_mapping([mapping(old="035", new="035")])

    def test_generic_k_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "generic K"):
            gate.validate_mapping([mapping("control", "K", "B")])

    def test_unknown_kind(self):
        with self.assertRaisesRegex(ValueError, "unknown namespace"):
            gate.validate_mapping([mapping("made_up")])

    def test_incidental_local_identifier_is_retained(self):
        gate.validate_mapping([mapping("local_identifier", "k33", "k33")])

    def test_unknown_role(self):
        row = dict(self.row, role="both")
        with self.assertRaisesRegex(ValueError, "role"):
            self.locations(row)

    def test_second_role_field_is_not_ignored(self):
        with self.assertRaisesRegex(ValueError, "ambiguous occurrence schema"):
            self.locations(dict(self.row, roles=["live", "historical"]))

    def test_unknown_phase(self):
        with self.assertRaisesRegex(ValueError, "phase"):
            self.locations(dict(self.row, phase="M8"))

    def test_unknown_mapping_field(self):
        with self.assertRaisesRegex(ValueError, "mapping field"):
            gate.validate_mapping([dict(mapping(), role="historical")])

    def test_external_tree_list_is_cryptographically_complete(self):
        folder = gate.ROOT / gate.DATA
        files = gate.load(folder / "external.json")["governance"]["files"]
        self.assertEqual(gate.file_tree_digest(files), gate.GOV_TREE)
        self.assertNotEqual(gate.file_tree_digest(files[:-1]), gate.GOV_TREE)

    def test_exact_historical_location(self):
        rows = self.locations()
        entry = {k: self.row[k] for k in ("id", "source", "path", "selector", "start", "end", "token", "role", "reason")}
        gate.validate_allowlist([entry], rows)

    def test_wildcard_exemption(self):
        row = dict(self.row, path="docs/*")
        entry = {k: row[k] for k in ("id", "source", "path", "selector", "start", "end", "token", "role", "reason")}
        with self.assertRaisesRegex(ValueError, "wildcard"):
            gate.validate_allowlist([entry], {row["id"]: row})

    def test_directory_exemption(self):
        row = dict(self.row, selector="directory")
        entry = {k: row[k] for k in ("id", "source", "path", "selector", "start", "end", "token", "role", "reason")}
        with self.assertRaisesRegex(ValueError, "wildcard"):
            gate.validate_allowlist([entry], {row["id"]: row})

    def test_unclassified_occurrence(self):
        with self.assertRaisesRegex(ValueError, "unclassified"):
            gate.validate_locations([], self.observed, set(), set())

    def test_unused_mapping_has_no_invented_location(self):
        with self.assertRaisesRegex(ValueError, "unused or unlocated"):
            gate.validate_locations([self.row], self.observed,
                                    {self.row["mapping"], "engineering_decision:KED-034"}, {"review"})

    def test_unknown_identity_cannot_hide_in_consumer_description(self):
        with self.assertRaisesRegex(ValueError, "unknown descriptive identity"):
            gate.validate_descriptions([("consumer:0:producer", "KED-035")], set())

    def test_stale_location(self):
        with self.assertRaisesRegex(ValueError, "drift"):
            self.locations(dict(self.row, start=1))

    def test_nonexistent_location(self):
        with self.assertRaisesRegex(ValueError, "nonexistent"):
            self.locations(dict(self.row, id="absent"))

    def test_inventory_token_drift(self):
        with self.assertRaisesRegex(ValueError, "drift"):
            self.locations(dict(self.row, token="KED-034"))

    def test_duplicate_assignment(self):
        with self.assertRaisesRegex(ValueError, "duplicate occurrence"):
            gate.validate_locations([self.row, self.row], self.observed,
                                    {self.row["mapping"]}, {"review"})

    def test_missing_historical_relationship(self):
        with self.assertRaisesRegex(ValueError, "historical relationship"):
            self.locations(dict(self.row, historical_relationship=""))

    def test_missing_consumer(self):
        with self.assertRaisesRegex(ValueError, "producer/consumer"):
            self.locations(dict(self.row, consumer="absent"))

    def test_duplicate_json_keys(self):
        with self.assertRaisesRegex(ValueError, "duplicate JSON"):
            json.loads('{"role":"live","role":"historical"}', object_pairs_hook=gate.strict_pairs)

    def test_decoded_field_selectors_are_distinct(self):
        rows, _ = gate.occurrences("data.json", b'{"live":"KED-035","old":"KED-035"}')
        self.assertEqual({r["selector"] for r in rows}, {"/live", "/old"})
        self.assertEqual(len({r["id"] for r in rows}), 2)

    def test_incorrect_recorded_base(self):
        record = {"base_branch": "prod", "base_sha": gate.BASE,
                  "head_branch": "bowfin-m0-pre-m1-readiness", "sole_merger": "gregkrsak",
                  "automatic_merging": False}
        with self.assertRaisesRegex(ValueError, "recorded PR base"):
            gate.validate_pr(record)

    def test_actual_retargeting_cannot_inherit_record(self):
        record = {"base_branch": gate.TARGET, "base_sha": gate.BASE,
                  "head_branch": "bowfin-m0-pre-m1-readiness", "sole_merger": "gregkrsak",
                  "automatic_merging": False}
        actual = {"base": {"ref": "prod", "sha": gate.BASE}, "head": {"ref": record["head_branch"]}, "draft": True}
        with self.assertRaisesRegex(ValueError, "actual PR base"):
            gate.validate_pr(record, actual)

    def test_merge_automation_invalidates_assumption(self):
        record = {"base_branch": gate.TARGET, "base_sha": gate.BASE,
                  "head_branch": "bowfin-m0-pre-m1-readiness", "sole_merger": "gregkrsak",
                  "automatic_merging": True}
        with self.assertRaisesRegex(ValueError, "assumption"):
            gate.validate_pr(record)

    def test_rewritten_observation_cannot_replace_git_history(self):
        path = gate.ROOT / "docs/engineering/ked-034-predecessors.json"
        original = Path.read_bytes
        def changed(p):
            data = original(p)
            return data.replace(b'"ok"', b'"new"', 1) if p == path else data
        with patch.object(Path, "read_bytes", changed):
            with self.assertRaisesRegex(ValueError, "changed an existing payload"):
                gate.validate()


if __name__ == "__main__":
    unittest.main(verbosity=2)
