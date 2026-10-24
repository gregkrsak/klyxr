# M0.1 readiness evidence

This package classifies names before any migration implementation. It grants no
approval, execution, replay, custody, resource, configuration or merge authority.
Independent review must assess the role declarations as well as the validator.

The accepted base is commit `37cb00d5cc01804e46446809fd325322c0c3f05f`, tree
`d817671d87760f62a603d2de2f696fe49f0a24ac`. The governing contract is unchanged,
blob `2d03f764b3fb516bc8d18c4e32a26c5c06a4d3fa`. This package neither revises it
nor implements the future identities described in it.

## Data model

- `manifest.json` declares namespace-qualified, injective mappings and the exact
  package paths. A mapping may intentionally retain an identity. Historical
  occurrences retain their original spelling even when a live mapping exists.
- `inventory.json` lists the complete accepted Git tree and individual occurrence
  assignments: exact source, path, scalar selector or text offset, token, role,
  mapping, producer, consumer and historical relationship. Offsets in text are
  Unicode code points; JSON offsets index the decoded string, not escaped bytes.
  JSON Pointer escaping distinguishes keys from values; `@key` marks key text.
  Filename-component selectors additionally locate typed components inside the
  exact tracked path; they are distinct from its complete filename identity.
- `allowlist.json` contains one exact entry for every historical or compatibility
  occurrence. No directory, path glob or blanket historical-file exception exists.
- `denylist.json` records the scanner vocabulary and exact live obligations. This
  is an M0.1 accounting gate: planned later changes are not reported as performed.
- `consumers.json` names current producers/consumers and absent facilities. Future
  translation is descriptive and cannot substitute for existing admission checks.
- `external.json` pins the read-only governance dependency and snapshots repository
  metadata. Governance text is fetched at its pinned commit and blob-checked when
  validating. Metadata is a point-in-time observation, not an immutable service.
- `visual.json` records manual inspection of the exact two raster blobs and their
  located text. SVG text is scanned directly. Small raster glyph uncertainty is
  bounded to the recorded regions and requires renewed inspection during M5.
- `supplemental.json` accounts for vocabulary in the new validator, tests and guide.
- `summary.json` is deterministic output recomputed from the occurrence assignments.
- `pr-evidence.json` records the owner gate's base and branch requirements.
- `digests.json` gives SHA-256 over each other structured artifact's exact UTF-8
  bytes, including its final newline. It deliberately does not digest itself.
  The final PR report also records the digest of this digest document.

Structured artifacts are descriptive records of the same occurrences, not new
product identities. Their references are checked by schema, exact correspondence
and digest rather than recursively rescanning every copied token into an infinite
inventory. Every other new file is scanned; the path set is exact and closed.
These parsers do not exempt product source, evidence, generated context or history.

## Validation

Run the standard-library tools from a checkout containing the accepted baseline:

```bash
python scripts/test-bowfin-readiness.py
python scripts/verify-bowfin-readiness.py
```

An optional `--governance-root PATH` supplies a read-only local mirror instead of
network fetches; every content blob must still match its pinned identity. Without
that argument, only pinned public raw text is requested, with a 15-second timeout
and a 16 MiB per-file read bound. Network unavailability rejects verification.
No governance code is executed. `--pr-json PATH` checks fresh connector/API metadata
against the recorded base; the report records the actual head and base SHAs.

Validation independently reads the accepted Git objects. It rejects any changed
existing payload except the exact documentation-workflow history-fetch setting
and appended validation step. Thus
editing both an old observation and a new expectation cannot replace the original
historical baseline. The baseline objects must be available; CI fetches full Git
history. There is no migration, normalization, compatibility reader or alias.

## Bounds and limitations

The scanner admits at most 512 tracked files, 16 MiB per file, 64 MiB of baseline
content, JSON depth 64 and 100,000 assigned occurrences. It scans UTF-8 text and
filenames, JSON keys/scalars, selected Rust function names, and exact external
snapshot fields. It does not execute code, infer dynamically assembled names,
decode arbitrary embedded archives or certify all Unicode confusables. The
compile-time compiler prefix and version producer are explicitly mapped.

Counts are typed units: an entire recorded test identity and a contained legacy
token can both have distinct rows. Filenames, raster regions and external fields
are separate units. These totals must not be presented as naive grep counts.

Repository metadata, tags/releases, domain references and service visibility have
the exact scope recorded in `external.json`. Enabled wiki content, private settings,
registrar/DNS state, social accounts and untracked external consumers are not
certified by a Git-tree scan. Their bounded reconciliation requirements remain
before the relevant external phase. Unknown or newly discovered covered occurrences
must be added and independently classified; they cannot silently inherit a role.

This does not begin extension compatibility tests, native work or M1. The sole-owner
merge gate remains procedural: implementer and reviewer verify the actual staging
target; Greg checks it immediately before merging. A wrong target or changed
sole-merger/no-automation assumption blocks acceptance. No staging workflow is
claimed to enforce production policy. Successful CI does not authorize a merge.
