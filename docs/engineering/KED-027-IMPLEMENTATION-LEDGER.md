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

Milestone 1 commit: `60cee544039ecd22b7ed01576490231e2a17cfd1`.
All four push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38297326807 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38297326828 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38297326804 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38297326820 |

Debug/release: 1,590 passed each. All 1,581 predecessor identities and outcomes
remain present; nine new identities passed in each profile. No failures or ignored
tests. Local fmt/check/Clippy passed as advisory only.

## Milestone 2 — lexical MIR cleanup metadata

The existing structured lowerer now emits private MIR scope descriptors, static
scope-entry sites, explicit successful-initialization sites, exact inner-to-outer
exit sets and synthesized origins. Function metadata retains reverse-ordered owned
record parameters and a protected-return boundary. Resource-free source lowering
retains its previous representation with no cleanup metadata.

MIR validation rejects duplicate exits, missing activations and missing cleanup
origins. Opaque execution correspondence compares the new metadata as part of the
complete deterministic MIR artifact. Admission bounds the new vectors before
recursive comparison. Trusted acquisition/release stubs reject arbitrary user
bodies, and canonical fixture construction repeats ordinary HIR validation and
ownership checking before lowering. Four new lowering/malformed-metadata controls
are authored; runtime cleanup is still pending.

Milestone 2 commit: `0bf7542f029040ce270c57b0ca398045e9df5a8f`.
All four push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38297981588 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38297981569 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38297981517 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38297981600 |

Debug/release: 1,594 passed each (1,581 predecessors plus thirteen new tests).
Local fmt/check/Clippy passed as advisory only.

## Milestone 3 — private synthetic leaf vertical slice

A non-cloneable runtime leaf retains canonical resource/type identity and a fresh
acquisition identity. Fixed trusted operations dispatch by FunctionId and catalog
identity; they do not execute the stub body or invoke callbacks. Scope activation
histories come from explicit MIR, including successful initialization and exit
sets. A transferred resource slot is distinguished from mere vacancy; cleanup
rejects an invalid owner state. Only resource-bearing local initialization and
parameter identities participate in cleanup.

The protected result, current cleanup payload, event trace, activation histories
and frame metadata are included in runtime storage accounting. Acquisition,
release attempts/completions/failures and frame retirement have a deterministic
private event trace. No source-visible production API or live-resource public
result is admitted. Private fixtures re-enter canonical admission, ownership,
lowering, opaque approval and correspondence; they are not public constructors.

Seven new vertical controls cover implicit completion, explicit NoValue return,
borrowed handles, name-independent trusted dispatch, public-result rejection,
missing cleanup, and forged cleanup plans. Aggregate payloads, pending construction
and the full boundary/failure batteries remain later milestones.

Milestone 3 commit: `2127503186654e618260fad063df9509e4ab4f34`.
All four push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38298735533 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38298735645 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38298735624 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38298735531 |

Debug/release: 1,601 passed each (1,581 predecessors plus twenty new tests).
No local executable evidence is used.

## Milestones 4–5 — transfer and structured-exit conformance

The vertical runtime already includes the inseparable transfer, parameter and
protected-result machinery. This milestone independently exercises those contracts
and structured control flow before aggregate payloads are added. An explicit guard
rejects reuse of an owner slot before its activation has completed cleanup.
Terminal trusted-release failure cannot resume scope cleanup.

Eighteen new tests cover same-scope A/B/C transfer ordering, explicit release,
reverse parameters, locals-before-parameters, parameter moves, protected returns,
parameter return, both branch-dependent consumption paths, skipped and executed
short-circuit consumption, nested fallthrough, repeated loop iterations, continue,
break, nested loops, nested return, conditional initialization and borrow-rule
preservation. No language semantics or predecessor assertion is weakened.

Milestones 4–5 initial commit: `2fbc0720f955a9da9d90abc6b7eb69bc85326ce0`.
Build/docs/verify passed. Tests run
https://github.com/gregkrsak/klyxr/actions/runs/38299407839 failed two new
short-circuit fixtures during resolution: source spelled the accepted built-in
`bool` as `Bool`. Classified as a test-construction defect, not an implementation
or semantic defect. The correction changes only that spelling and preserves both
skipped-consumption and executed-consumption assertions. No later architectural
layer is being added until the corrected complete workflow gate passes.

Milestones 4–5 correction: `85a122266e0cde4897b15192d7fd91982e9f0366`.
All four corrected push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38299632722 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38299632462 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38299633208 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38299632633 |

## Milestone 6 — aggregates and pending construction

Private aggregate payloads cannot clone resource leaves. Pending constructions
retain canonical record/field IDs, exact initialized membership, owned field
payloads and completion state. Each field is evaluated and committed in written
order; complete publication requires the full canonical field set. Cleanup uses
shared reverse-declaration plans and explicit bounded continuations.

The private synthetic failure control aborts pending construction and then stops
the tool. It is separate from range traps, configured stops, integrity failures
and host failures, and adds no recoverable source syntax, general propagation or
unwinding. Initialized pending resource fields clean in reverse canonical order,
not evaluation order. Nested pending containers abort inside-out. Moved sources
remain transferred and are never restored.

Retained detached payloads, pending fields, aggregate buffers, cleanup work,
protected return results and synthetic events participate in prospective storage
accounting. Field initialization, cleanup traversal and fixed release charge fuel.
Ten new aggregate controls cover complete reverse-order cleanup, a non-last
resource field, the exact third/first/second-fails witness, aggregate move/return,
nested complete/pending structures, source non-restoration, projection rejection
and incomplete-publication rejection.

Advisory local checking/Clippy completed but also reported a corrupt incremental
cache artifact. No repair, target experiment or local executable test was attempted;
GitHub remains the only authoritative executable environment.
