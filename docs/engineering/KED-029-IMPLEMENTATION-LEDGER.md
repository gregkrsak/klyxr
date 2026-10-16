# KED-029 implementation ledger

Issue: https://github.com/gregkrsak/klyxr/issues/66
Frozen specification: Draft 2; freeze 6207528580; authorization 6207551736.
Accepted implementation-pressure review: 6207528400.
Baseline: `72c34593e79e7763ce16f0e18ad99595959a2454`.
Tree: `2b9339fbe21f705cb8c5722ff681c34680870490`.
Branch: `ked-029-deterministic-expression-temporaries`; target: `prod`.
Current charter blob: `b04c4ba5251fdcacb5278ba26c0aaf8a06b4db7e`.

GitHub Actions is authoritative for compilation and executable evidence. Local
Rust validation is unavailable because of confirmed Work-runtime corruption.
No local Rust, Rustfmt, Clippy, compiler or test execution is performed. Source,
Git and non-Rust text operations are local. No runtime remediation is attempted.

## Unchanged predecessor gate

Fetched origin/prod, verified exact commit/tree and clean index/worktree. Neither
local nor remote implementation branch existed. Created both at the exact baseline.
The unchanged branch passed all four push workflows:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38846904919 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38846904912 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38846904978 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38846904965 |

Tests job 116799384664 reports 1,706 passing identities in each complete debug
and release profile: 1,667 reconciled predecessors and 39 KED-028 additions.
Both authority doctests and both 1,800-case affine checks passed; failed, ignored
and filtered counts are zero. `ked-029-predecessors.json` records the exact debug
inventory. Historical inventories and verifiers remain unchanged.

## Milestone 1 — canonical source-region metadata

Sealed MIR carries separate source-region and expression-site identities, exact
types, canonical plan references, destination identities, source origins, written
child order and explicit field-child region nesting. Conditional initializer
conditions and leaves share one source region; calls do not create caller-side
regions. Metadata allocation is fallible and bounded by existing admitted syntax;
shape checks precede exact deterministic correspondence comparison.

This milestone does not admit new runtime resource arguments or claim temporary
execution support. Existing ownership traversal, call holds, borrowing, recurring
conditions and resource-free runtime remain unchanged. Four focused metadata
regressions are classified explicitly in the evidence manifest. The new inventory
verifier requires all 1,706 predecessor identities and all declared additions.

Validation pending. No implementation PR, acceptance, issue closure or merge.
