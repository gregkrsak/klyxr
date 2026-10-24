#!/usr/bin/env python3
"""Bounded M0.1 occurrence reconciliation. This tool performs no renames.

The accepted Git baseline, not two editable copies of an expectation, anchors
historical truth. Mapping declarations classify occurrences; scanning never
guesses a role. Independent review is still required for those declarations.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import urllib.request

BASE = "37cb00d5cc01804e46446809fd325322c0c3f05f"
TREE = "d817671d87760f62a603d2de2f696fe49f0a24ac"
PLAN = "2d03f764b3fb516bc8d18c4e32a26c5c06a4d3fa"
GOV = "37d4446153cd761b70ad7fc599db1df994352585"
GOV_TREE = "dab0ee3d04e3c478d818a7dc15f7a502303a356d"
TARGET = "bowfin-migration-staging"
ROOT = Path(__file__).resolve().parents[1]
DATA = "docs/migration/readiness/"
ARTIFACTS = ["manifest.json", "inventory.json", "allowlist.json", "denylist.json",
             "consumers.json", "external.json", "visual.json", "supplemental.json",
             "summary.json", "pr-evidence.json", "digests.json"]
NEW_PATHS = sorted([DATA + name for name in ARTIFACTS] + [DATA + "README.md",
                   "scripts/verify-bowfin-readiness.py", "scripts/test-bowfin-readiness.py"])
LIMITS = {"files": 512, "file_bytes": 16_777_216,
          "total_bytes": 67_108_864, "occurrences": 100_000,
          "json_depth": 64}
ROLES = {"live", "compatibility", "historical"}
KINDS = {"brand", "engineering_decision", "language_decision", "control",
         "control_family", "local_identifier", "test_identity", "machine",
         "compiler_identity", "environment", "package", "crate", "binary",
         "configuration_reservation", "source_extension", "fence", "filename",
         "context_input", "context_output", "repository", "domain", "monogram",
         "visual_text", "decision_family", "script_stem", "rust_identifier", "pronunciation"}
PATTERNS = [
    ("repository", r"gregkrsak/klyxr(?:-ai-governance)?"),
    ("domain", r"klyxr\.(?:com|online)\b"),
    ("configuration_reservation", r"klyxr\.toml\b"),
    ("machine", r"klyxr\.[a-z0-9_.-]+"),
    ("environment", r"CARGO_[A-Za-z0-9_]+|RUNNER_TEMP|GITHUB_[A-Z0-9_]+|K[0-9]+_[A-Z][A-Z0-9_]*"),
    ("compiler_identity", r"klyxr-compiler/(?:<version>|[0-9.]+)?"),
    ("package", r"klyxr-compiler\b"),
    ("crate", r"klyxr_compiler\b"),
    ("context_output", r"KLYXR_AI_CONTEXT_BUNDLE\.md"),
    ("context_input", r"KLYXR_CONTEXT\.md"),
    ("script_stem", r"(?:verify-)?ked\d{3}[-a-z]+\.py"),
    ("engineering_decision", r"ked-?\d{3}(?:[–-]\d{3})?"),
    ("language_decision", r"kd-?\d{3}(?:[–-]\d{3})?"),
    ("control", r"\bK\d{2,3}-[A-Z]+\d+(?:[–-][A-Z]*\d+)?"),
    ("control_family", r"\bK\d{2,3}(?:-[A-Z]+)?\b"),
    ("local_identifier", r"\bk\d{2,3}\b"),
    ("source_extension", r"\.klx\b"),
    ("monogram", r"\bklx\b"),
    ("pronunciation", r"CLICK[-‑–]ser"),
    ("decision_family", r"\bked\b|\bkd\b"),
    ("binary", r"(?-i:(?<![A-Za-z0-9_.-])klyxr(?![A-Za-z0-9_.-]))"),
    ("brand", r"klyxr"),
]
COMPILED = [(k, re.compile(p, re.I if k not in
             {"environment", "control", "control_family"} else 0))
            for k, p in PATTERNS]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def blob_digest(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def file_tree_digest(files):
    """Independently bind a complete external file list to its Git tree."""
    tree = {}
    for item in files:
        parts = item["path"].split("/")
        require(all(p not in {"", ".", ".."} for p in parts), "unsafe external path")
        node = tree
        for part in parts[:-1]:
            node = node.setdefault(part, {})
            require(isinstance(node, dict), "external tree path collision")
        require(parts[-1] not in node, "duplicate external file")
        require(item["mode"] == "100644", "unsupported external mode")
        node[parts[-1]] = item
    def finish(node):
        data = bytearray()
        for name, child in sorted(node.items(), key=lambda pair: (pair[0] + ("/" if "blob" not in pair[1] else "")).encode()):
            if "blob" in child:
                mode, sha = child["mode"], child["blob"]
            else:
                mode, sha = "40000", finish(child)
            data.extend(mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(sha))
        return hashlib.sha1(b"tree " + str(len(data)).encode() + b"\0" + data).hexdigest()
    return finish(tree)


def canonical_json(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True,
                       separators=(",", ":")) + "\n").encode()


def strict_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key: " + key)
        result[key] = value
    return result


def load(path):
    data = path.read_bytes()
    require(len(data) <= LIMITS["file_bytes"], "file byte bound")
    return json.loads(data, object_pairs_hook=strict_pairs)


def tree_files(revision):
    entries = git("ls-tree", "-r", "-z", revision).split(b"\0")
    result = []
    for entry in filter(None, entries):
        info, path = entry.split(b"\t", 1)
        mode, kind, sha = info.decode().split()
        require(kind == "blob", "submodule or unsupported tracked kind")
        result.append({"path": path.decode(), "mode": mode, "blob": sha})
    require(len(result) <= LIMITS["files"], "tracked file bound")
    return result


def tokens(text):
    candidates = []
    for priority, (kind, pattern) in enumerate(COMPILED):
        for m in pattern.finditer(text):
            candidates.append((m.start(), priority, -len(m[0]), m.end(), kind, m[0]))
    end = -1
    for start, _, _, stop, kind, token in sorted(candidates):
        if start < end:
            continue
        end = stop
        yield start, stop, kind, token


def pointer(path):
    return "/" + "/".join(str(p).replace("~", "~0").replace("/", "~1") for p in path)


def scalar_fields(value, path=(), depth=0):
    require(depth <= LIMITS["json_depth"], "JSON depth bound")
    if isinstance(value, dict):
        for key in sorted(value):
            yield pointer(path + (key,)) + "@key", key
            yield from scalar_fields(value[key], path + (key,), depth + 1)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from scalar_fields(item, path + (index,), depth + 1)
    elif isinstance(value, str):
        yield pointer(path), value


def occurrences(path, data, source="compiler"):
    """JSON selectors index decoded scalars; text selectors use Unicode offsets.

    The complete bytes are independently hashed. No numeric suffix is treated
    as a decision identity. Filename and test-identity rows are additional typed
    units, so counts are not a naive grep match count.
    """
    require(len(data) <= LIMITS["file_bytes"], "source file bound")
    rows = []

    def add(selector, text, start, end, kind, token, line=0, column=0):
        location = {"source": source, "path": path, "selector": selector,
                    "start": start, "end": end, "line": line, "column": column}
        key = digest(canonical_json([location, kind, token]))[:24]
        rows.append({"id": key, **location, "kind": kind, "token": token})

    if list(tokens(path)):
        kind = ("context_output" if path.endswith("KLYXR_AI_CONTEXT_BUNDLE.md") else
                "context_input" if path.endswith("KLYXR_CONTEXT.md") else "filename")
        add("filename", path, 0, len(path), kind, path)
        for start, end, component_kind, token in tokens(path):
            add("filename-component", path, start, end, component_kind, token)
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return rows, "binary"
    if path.endswith(".json"):
        fields = scalar_fields(json.loads(text, object_pairs_hook=strict_pairs))
    else:
        fields = [("text", text)]
    if path.endswith(".rs"):
        for match in re.finditer(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\s*\(", text):
            name = match[1]
            if re.search(r"(?i)ked|klyxr|(?:^|_)k\d", name):
                start, end = match.span(1)
                kind = "test_identity" if "#[test]" in text[max(0, match.start()-160):match.start()] else "rust_identifier"
                add("text", text, start, end, kind, name,
                    text.count("\n", 0, start)+1, start-text.rfind("\n", 0, start))
    for selector, scalar in fields:
        # Entire recorded test identities are accounted even if unbranded.
        if selector != "text" and (scalar.startswith(("lib.rs::", "tests/", "doctest:"))
                                   and "::" in scalar):
            add(selector, scalar, 0, len(scalar), "test_identity", scalar)
        for start, end, kind, token in tokens(scalar):
            if source == "metadata" and path == "repository.json" and selector in {"/name", "/full_name"}:
                kind = "repository"
            line = scalar.count("\n", 0, start) + 1 if selector == "text" else 0
            column = start - scalar.rfind("\n", 0, start) if line else 0
            if kind in {"brand", "binary"} and selector == "text":
                prefix = scalar[max(0, start - 3):start]
                if prefix == "```":
                    kind = "fence"
                elif scalar[max(0, start - 14):start] == "CARGO_BIN_EXE_":
                    kind = "environment"
            add(selector, scalar, start, end, kind, token, line, column)
    return rows, "text"


def mapping_key(kind, token):
    return kind + ":" + token


def validate_mapping(entries):
    keys, targets = set(), set()
    for entry in entries:
        require(set(entry) == {"id", "kind", "legacy", "canonical", "phase", "policy", "validation"},
                "unknown or ambiguous mapping field")
        kind, old, new = entry["kind"], entry["legacy"], entry["canonical"]
        require(kind in KINDS, "unknown namespace kind")
        key = mapping_key(kind, old)
        require(entry["id"] == key and key not in keys, "duplicate or malformed mapping key")
        require((kind, new) not in targets, "non-injective canonical mapping")
        require(isinstance(old, str) and old and isinstance(new, str) and new,
                "empty or numeric-only mapping")
        if kind == "engineering_decision":
            require(re.fullmatch(r"(?i)ked-?\d{3}(?:[–-]\d{3})?", old)
                    and new == re.sub(r"(?i)^ked", lambda m: "BED" if m[0].isupper() else "bed", old),
                    "KED/BED namespace conflation")
        if kind == "language_decision":
            require(re.fullmatch(r"(?i)kd-?\d{3}(?:[–-]\d{3})?", old)
                    and new == re.sub(r"(?i)^kd", lambda m: "BD" if m[0].isupper() else "bd", old),
                    "KD/BD namespace conflation")
        if kind in {"control", "control_family"}:
            require(re.fullmatch(r"K\d{2,3}(?:-[A-Z]+\d*(?:[–-][A-Z]*\d+)?)?", old)
                    and new == "B" + old[1:], "generic K replacement")
        if old == "K" or new == "B" and kind in {"control", "control_family"}:
            raise ValueError("generic K replacement")
        require(entry["phase"] in {"M0.1", "M1", "M2", "M3", "M4", "M5", "M6", "M7"},
                "unknown phase")
        require(entry["policy"] and entry["validation"], "missing support or validation policy")
        keys.add(key)
        targets.add((kind, new))
    return keys


def validate_locations(rows, observed, mappings, consumers):
    require(len(rows) <= LIMITS["occurrences"], "occurrence bound")
    assigned = {}
    core = {k: v for k, v in observed.items()}
    for row in rows:
        require(set(row) == {"id", "source", "path", "selector", "start", "end", "line", "column",
                             "kind", "token", "mapping", "role", "phase", "consumer", "producer",
                             "reason", "historical_relationship"}, "ambiguous occurrence schema")
        require(row["role"] in ROLES, "unknown or ambiguous occurrence role")
        require(row["id"] not in assigned, "duplicate occurrence assignment")
        require(row["id"] in core, "stale or nonexistent occurrence")
        actual = {k: row[k] for k in core[row["id"]]}
        require(actual == core[row["id"]], "manifest/inventory location drift")
        require(row["mapping"] == mapping_key(row["kind"], row["token"])
                and row["mapping"] in mappings, "unmapped occurrence")
        require(row["consumer"] in consumers and row["producer"] and row["reason"],
                "missing producer/consumer or role reason")
        require(row["historical_relationship"], "missing historical relationship")
        require(row["phase"] in {"M0.1", "M1", "M2", "M3", "M4", "M5", "M6", "M7"},
                "unknown occurrence phase")
        assigned[row["id"]] = row
    require(set(assigned) == set(core), "unclassified discovered occurrence")
    require({r["mapping"] for r in rows} == mappings, "unused or unlocated mapping")
    return assigned


def validate_descriptions(descriptions, mappings):
    """Closed-schema descriptions may reference known names, never hide new ones."""
    for location, text in descriptions:
        for _, _, kind, token in tokens(text):
            require(mapping_key(kind, token) in mappings,
                    "unknown descriptive identity at " + location)


def validate_allowlist(entries, rows):
    expected = {k for k, v in rows.items() if v["role"] in {"historical", "compatibility"}}
    seen = set()
    for entry in entries:
        require(set(entry) == {"id", "source", "path", "selector", "start", "end",
                               "token", "role", "reason"}, "ambiguous historical exemption")
        require(entry["id"] in expected and entry["id"] not in seen, "unlocated exemption")
        row = rows[entry["id"]]
        require(entry == {k: row[k] for k in entry}, "allowlist/occurrence drift")
        require(not any(c in entry["path"] for c in "*?[]")
                and entry["selector"] not in {"*", "**", "directory"}, "wildcard exemption")
        seen.add(entry["id"])
    require(seen == expected, "missing exact historical/compatibility exception")


def validate_pr(record, actual=None):
    require(record["base_branch"] == TARGET and record["base_sha"] == BASE,
            "incorrect recorded PR base")
    require(record["head_branch"] == "bowfin-m0-pre-m1-readiness",
            "incorrect recorded phase branch")
    require(record["sole_merger"] == "gregkrsak" and record["automatic_merging"] is False,
            "owner merge assumption changed")
    if actual is not None:
        require(actual["base"]["ref"] == TARGET and actual["base"]["sha"] == BASE,
                "incorrect actual PR base")
        require(actual["head"]["ref"] == record["head_branch"] and actual["draft"]
                and not actual.get("auto_merge"), "PR state or authority mismatch")


def summaries(rows):
    return {"total": len(rows), "by_kind": dict(sorted(Counter(r["kind"] for r in rows).items())),
            "by_role": dict(sorted(Counter(r["role"] for r in rows).items())),
            "by_source": dict(sorted(Counter(r["source"] for r in rows).items()))}


def validate(root=ROOT, pr=None, governance_root=None):
    global ROOT
    ROOT = root
    folder = root / DATA
    require(git("rev-parse", BASE + "^{tree}").decode().strip() == TREE, "accepted base tree mismatch")
    require(git("rev-parse", BASE + ":docs/migration/BOWFIN-NAME-MIGRATION-PLAN.md").decode().strip() == PLAN,
            "frozen contract blob mismatch")
    manifest = load(folder / "manifest.json")
    require(set(manifest) == {"schema", "base", "mappings", "new_paths", "structured_artifacts"},
            "unknown manifest field")
    require(manifest["schema"] == 1 and manifest["base"] == {"commit": BASE, "tree": TREE},
            "manifest baseline drift")
    require(manifest["new_paths"] == NEW_PATHS and manifest["structured_artifacts"] == ARTIFACTS,
            "unregistered readiness parser or path")
    mappings = validate_mapping(manifest["mappings"])
    consumers = load(folder / "consumers.json")
    consumer_ids = set()
    for item in consumers:
        require(set(item) == {"id", "producer", "consumer", "phase", "authority_boundary"},
                "unknown consumer field")
        require(item["id"] not in consumer_ids and item["producer"] and item["consumer"]
                and item["authority_boundary"], "duplicate or incomplete consumer")
        consumer_ids.add(item["id"])
    inventory = load(folder / "inventory.json")
    require(set(inventory) == {"files", "occurrences"}, "unknown inventory field")
    baseline = tree_files(BASE)
    require(inventory["files"] == baseline, "baseline inventory drift")
    observed, total_bytes, binary = {}, 0, []
    for item in baseline:
        path = item["path"]
        data = git("show", BASE + ":" + path)
        require(blob_digest(data) == item["blob"], "baseline blob mismatch")
        current = (root / path).read_bytes()
        if current != data:
            expected_workflow = data.replace(b"      - uses: actions/checkout@v4\n",
                                            b"      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n", 1)
            require(path == ".github/workflows/docs.yml"
                    and current == expected_workflow + WORKFLOW_APPEND.encode(),
                    "M0.1 changed an existing payload: " + path)
        total_bytes += len(data)
        require(total_bytes <= LIMITS["total_bytes"], "snapshot byte bound")
        found, encoding = occurrences(path, data)
        if encoding == "binary":
            binary.append(path)
        for row in found:
            require(row["id"] not in observed, "duplicate scanner identity")
            observed[row["id"]] = row
    external = load(folder / "external.json")
    require(external["governance"]["commit"] == GOV
            and external["governance"]["tree"] == GOV_TREE, "external anchor drift")
    require(file_tree_digest(external["governance"]["files"]) == GOV_TREE,
            "incomplete or altered governance file inventory")
    external_observed = []
    for item in external["governance"]["files"]:
        require(not any(p in item["path"].split("/") for p in ("..", ""))
                and not item["path"].startswith("/"), "unsafe external path")
        if governance_root is not None:
            content = (governance_root / item["path"]).read_bytes()
        else:
            url = ("https://raw.githubusercontent.com/gregkrsak/klyxr-ai-governance/"
                   + GOV + "/" + item["path"])
            with urllib.request.urlopen(url, timeout=15) as response:
                content = response.read(LIMITS["file_bytes"] + 1)
        require(len(content) <= LIMITS["file_bytes"] and blob_digest(content) == item["blob"],
                "governance snapshot content mismatch")
        found, encoding = occurrences(item["path"], content, "governance")
        require(encoding == "text", "uninspected governance binary")
        external_observed.extend(found)
    for name, snapshot in sorted(external["metadata"].items()):
        found, encoding = occurrences(name + ".json", canonical_json(snapshot), "metadata")
        external_observed.extend(found)
    require(external["occurrences"] == external_observed, "external occurrence drift")
    for row in external_observed:
        require(row["id"] not in observed, "duplicate external occurrence")
        observed[row["id"]] = row
    visual = load(folder / "visual.json")
    require(set(v["path"] for v in visual["assets"]) == set(binary), "binary inspection coverage gap")
    for item in visual["assets"]:
        require(item["blob"] == next(x["blob"] for x in baseline if x["path"] == item["path"])
                and item["method"] == "manual inspection" and item["limitations"], "unbound visual inspection")
    for row in visual["occurrences"]:
        require(row["id"] not in observed, "duplicate visual occurrence")
        observed[row["id"]] = row
    supplemental = load(folder / "supplemental.json")
    expected_new = set(manifest["new_paths"])
    tracked = {x.decode() for x in git("ls-files", "-z").split(b"\0") if x}
    require(tracked == {x["path"] for x in baseline} | expected_new, "unregistered tracked file")
    structural = {DATA + n for n in manifest["structured_artifacts"]}
    extra = []
    for path in sorted(expected_new - structural):
        found, encoding = occurrences(path, (root / path).read_bytes())
        require(encoding == "text", "new unsupported binary")
        extra.extend(found)
    require(supplemental["occurrences"] == extra, "supplemental occurrence drift")
    for row in extra:
        require(row["id"] not in observed, "duplicate supplemental occurrence")
        observed[row["id"]] = row
    # Generated data references are checked structurally, not recursively scanned
    # as fresh source occurrences of their own copied tokens (an infinite regress).
    assigned = validate_locations(inventory["occurrences"], observed, mappings, consumer_ids)
    allowlist = load(folder / "allowlist.json")
    validate_allowlist(allowlist, assigned)
    deny = load(folder / "denylist.json")
    require(deny["phase"] == "M0.1" and deny["rename_performed"] is False
            and deny["patterns"] == PATTERNS_AS_DATA, "denylist vocabulary drift")
    definitions = {entry["id"]: entry for entry in manifest["mappings"]}
    require(deny["live_obligations"] == sorted(k for k, v in assigned.items()
            if v["role"] == "live" and definitions[v["mapping"]]["canonical"] != v["token"]),
            "live denylist obligation drift")
    descriptions = []
    for index, entry in enumerate(manifest["mappings"]):
        for key in ("policy", "validation"):
            descriptions.append(("manifest:" + str(index) + ":" + key, entry[key]))
    for index, item in enumerate(consumers):
        for key in ("producer", "consumer", "authority_boundary"):
            descriptions.append(("consumer:" + str(index) + ":" + key, item[key]))
    for row in assigned.values():
        for key in ("reason", "historical_relationship"):
            descriptions.append((row["id"] + ":" + key, row[key]))
    # Repeated policy text need be recognized only once. This does not remove
    # occurrence records: their exact selectors remain separately checked above.
    unique = {}
    for location, text in descriptions:
        unique.setdefault(text, location)
    validate_descriptions([(location, text) for text, location in sorted(unique.items())], mappings)
    validate_pr(load(folder / "pr-evidence.json"), pr)
    summary = {"base_commit": BASE, "base_tree": TREE, "tracked_baseline_files": len(baseline),
               "snapshot_bytes": total_bytes, "binary_assets": len(binary),
               **summaries(list(assigned.values())), "mapping_count": len(mappings),
               "exceptions": len(load(folder / "allowlist.json")),
               "status": "accounted; independent role review and owner acceptance pending"}
    require(load(folder / "summary.json") == summary, "generated summary drift")
    names = manifest["structured_artifacts"]
    digests = load(folder / "digests.json")
    require(set(digests) == set(names) - {"digests.json"}, "digest coverage drift")
    for name, value in digests.items():
        require(value == digest((folder / name).read_bytes()), "readiness digest mismatch: " + name)
    return summary


WORKFLOW_APPEND = '''
      - name: Validate bounded Bowfin M0.1 readiness
        run: |
          python scripts/test-bowfin-readiness.py
          if [ "$GITHUB_EVENT_NAME" = "pull_request" ]; then
            python scripts/verify-bowfin-readiness.py --pr-json "$GITHUB_EVENT_PATH"
          else
            python scripts/verify-bowfin-readiness.py
          fi
'''
PATTERNS_AS_DATA = [{"kind": k, "pattern": p} for k, p in PATTERNS]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pr-json", type=Path, help="fresh read-only GitHub PR metadata, if supplied")
    parser.add_argument("--governance-root", type=Path,
                        help="local read-only mirror; each blob is checked against pinned identity")
    args = parser.parse_args()
    try:
        actual = load(args.pr_json) if args.pr_json else None
        if actual is not None:
            actual = actual.get("pull_request", actual)
        print(canonical_json(validate(pr=actual,
                                      governance_root=args.governance_root)).decode(), end="")
    except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError,
            RecursionError, json.JSONDecodeError) as error:
        print("M0.1 readiness rejected: " + str(error), file=sys.stderr)
        sys.exit(1)
