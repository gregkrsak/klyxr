# KED-028 implementation ledger

Issue: https://github.com/gregkrsak/klyxr/issues/64
Frozen specification: Draft 1; freeze comment 6195981810.
Accepted pressure review: comment 6195811443.
Baseline: `01f80fa636e8a06bd5b4a145307a7f0cd3626d04`.
Baseline tree: `55d2a06fde7cdf990bbb4f364baf82b49ecf8c06`.
Branch: `ked-028-deterministic-move-replacement`; PR target: `prod`.
Arnold charter retrieved at governance revision
`2887c9239fb6f82ded4fd7b2d8f5f592b725cccc`, blob
`b04c4ba5251fdcacb5278ba26c0aaf8a06b4db7e`.

Greg authorized GitHub Actions as the authoritative compilation and executable
validation environment. Local Rust validation is unavailable because of confirmed
Work-runtime extraction corruption. No local Rust execution, installation,
extraction, integrity probing or remediation is performed during this assignment.
Source edits, Python text processing and Git operations are local. Shell push
lacks credentials; connected GitHub Git-data operations publish reviewed commits.

## Unchanged baseline gate

Fetched origin/prod and verified exact commit/tree. Index and worktree were clean.
No authorized remote branch existed; created it unchanged at the exact baseline.
All four push workflows passed independently:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38787690440 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38787690500 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38787690573 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38787690531 |

The complete tests job (116603833362) supplied both debug and release identity
inventories: exactly 1,667 each, identical identities and outcomes, all `ok`.
`ked-028-predecessors.json` preserves this exact inventory, including the authority
compile-fail doctest. Both profiles execute the 1,800-case affine cross-check.
The original KED-027 inventory and verifier are preserved as historical evidence.

## Milestone 1 — prepared checker replacement

Canonical assignment validation checks mutable owned target identity and exact
RHS type. The existing single ownership checker snapshots prepared entry, validates
target availability and relative loop eligibility, prevents target consumption
through its canonical consume path, evaluates the complete RHS, expires ordinary
loans and checks final whole-owner exclusivity. Its scoped restriction creates
no synthetic loan and participates in complete rollback. Copy assignment retains
its existing path. Runtime replacement authority and generations are later layers;
this milestone does not claim complete KED-028 execution.

### Predecessor corrections

The four designated rejection-to-acceptance migrations are listed exactly in
`ked-028-evidence.json`; assertions now establish acceptance and RHS transfer.
The pressure review's `boundary_tests` namespace is corrected to the observed
`bounds_and_artifact_tests` identity without changing its witness.

One additional obsolete rejection was found in the exact inventory:
`tests/iteration_local_moves.rs::iteration_local_replacement`. It initializes a
mutable owner inside the current loop iteration and replaces it with `fresh()`.
Frozen §12 expressly accepts this form. The test keeps its identity and source,
but now asserts acceptance. This is recorded separately as an outcome correction,
not hidden among the four designated renames. No frozen semantic change is made.

Header-target, header-source, break and return witnesses remain Ownership
rejections, with diagnostics updated to the narrower controlling loop boundary.
The CLI non-Copy dereference fixture remains unchanged.

New focused checker tests cover final and future shared/exclusive uses,
self-consumption at nested RHS depth, moved/conditional targets, exact nominal
and reference boundaries, branches, iteration-local exits and nested-loop targets.
New identities require explicit manifest membership; counts alone cannot pass CI.

Milestone validation is pending. No PR, acceptance, issue closure or merge.
