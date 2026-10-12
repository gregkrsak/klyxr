# KED-027 / KD-042 implementation ledger

Issue: https://github.com/gregkrsak/klyxr/issues/60
Branch: `ked-027-deterministic-scope-cleanup`
Starting commit: `8d5b60ac9f46ba44d6df0392b505ebf8343eba5e`
Starting tree: `0d73ae4ed7ba8d11f4c62896193cdbacee57db63`

Greg authorized GitHub Actions as the authoritative executable environment during
K27-I01. Local formatting, checking and Clippy are advisory only. No local test
or conformance claim is made. Command-line push lacked credentials; branch
creation and subsequent publication use the connected GitHub Git-data API.

## Exact predecessor baseline

The unchanged branch triggered all four push workflows at the exact starting SHA.
All passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38296578935 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38296578865 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38296578897 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38296578863 |

Tests: 1,580 unit/integration identities plus the `CheckedExecutable`
compile-fail doctest, all passing in debug and release (1,581 in each).
No failed, ignored or filtered predecessor tests. Rust/Cargo 1.80.0;
workspace/all-target checking, formatting, Clippy with warnings denied,
README assets, accepted/rejected battery CLI checks and the CLI suite passed.
The affine exhaustive-domain cross-check ran in both profiles.

## Milestone 1 — canonical resource foundations

Private canonical resource and trusted acquisition/release identities; structural
owned field types for internal fixtures; bounded acyclic, shared reverse-declaration
cleanup plans. Source record declarations still admit named-range fields only.
Reference fields and public resource registration are not admitted. Every record
and trusted leaf remains Move under the existing single ownership checker.

Nine new catalog controls cover reverse field order, name independence, inert Move
records, foreign identities, invalid trusted pairing, duplicate leaves, borrowed
release parameters, cycles, and foreign fields. These are classification/schema
controls, not resource execution evidence. Existing predecessor assertions retain
their range-identity and malformed-metadata witnesses through the explicit
`Field::range_type()` accessor after structural field typing.

Runtime cleanup, lexical activation, resource payloads and synthetic execution are
not implemented at this milestone. No PR or merge is authorized by this ledger.
