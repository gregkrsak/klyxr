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

Milestone 6 initial commit: `bf03f406220397508c84135f348c346d2bef8cf3`.
Build/docs/verify passed. Tests run
https://github.com/gregkrsak/klyxr/actions/runs/38301192993 stopped ten new aggregate
fixtures: the private fixture's placeholder-field removal renumbered the canonical
field table but had not remapped every expression and additional record. Classified
as fixture-translation failure against the existing canonical FieldId contract.
The correction remaps constructor/read/write field identities and record member
vectors; no cleanup-order, partial-construction or source-restoration assertion is
changed. Later milestones remain gated on a complete corrected workflow result.

Milestone 6 correction: `c25b4dad318bc60aa32141d04729b085197f9de7`.
All four corrected push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38301659243 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38301659298 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38301659138 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38301659270 |

## Milestone 7 — bounds, malformed artifacts and reporting

Only functions with owned resource locals/parameters or protected resource results
receive cleanup flow. The resource runtime starts lazily, so an unused private
catalog retains resource-free fuel and storage behavior. Resource-free callers can
invoke resource-owning helpers; reference-only callees carry no responsibility.

Canonical admission checks initializer/destination and statement-call parameter
identity, rejects undefined resource argument temporaries, bounds newly registered
metadata/nodes/type depth, and reserves the expanded frontend envelope. Synthesized
scope entries retain the source scope's span; empty function cleanup no longer
uses a generic start-of-file placeholder. Resource/Copy mismatches cannot obtain
approval. The payload enum remains non-cloneable and its Copy accessor rejects
leaves and aggregates.

Twenty-four new tests cover exact sufficient/insufficient fuel and storage,
controlled release failure and no retry, protected-return retention, resource-free
accounting, canonical Copy conflicts, wrong/foreign owners, wrong scopes and exit
sets, invalid initialization/origins, inert/reference cleanup targets, loop-edge
omission/reordering, foreign/reordered plans, trusted-operation pairing, undefined
temporaries, graph/name bounds, replacement rejection, renderer allocation failure,
writer failure, and conservation across every fuel stop in the A/B/C transfer
witness. Matrix tests preserve multiple independently malformed controls.

Milestone 7 commit: `c3b38909ab3a1662d0da93f05f26a5b84278d7cc`.
All four push workflows passed:

| Workflow | Run |
| --- | --- |
| build | https://github.com/gregkrsak/klyxr/actions/runs/38305366758 |
| tests | https://github.com/gregkrsak/klyxr/actions/runs/38305366703 |
| docs | https://github.com/gregkrsak/klyxr/actions/runs/38305366688 |
| verify | https://github.com/gregkrsak/klyxr/actions/runs/38305366727 |

Debug/release: 1,653 passed each (1,581 predecessors plus 72 new tests).
Identity reconciliation against the baseline job log found every predecessor
passing in each profile, including the compile-fail doctest. Cargo stderr target
headers can arrive before the preceding stdout tests; reconciliation associates
complete test groups with the ordered target list rather than arrival timestamps.

Three further conservation controls exercise aggregate pending construction,
protected aggregate return, complete cleanup at every fuel stop, exact storage
threshold and sampled lower storage stops, and controlled host failure during
partial abort. They retain remaining leaves rather than retrying or restoring
sources. Local formatting and Clippy are advisory only.

Additional conservation commit: `ba6207aa1baa12860cb4b3047b9d4b30c789d0ba`.
Build (38305853409) and tests (38305853260) failed to compile a new retry assertion
that cloned the deliberately non-cloneable private Event log. Docs (38305853346)
and verify (38305853303) passed. Classified as an ordinary test compilation defect.
The correction snapshots Debug text and checks exact equality after the rejected
retry; it does not add Clone or alter the resource payload or event contract.
Local Clippy passed after the correction, as advisory only.

The first correction publication (`fc25ade22981441d94ada804190fbcd1670e2a14`)
contained the ledger but omitted the staged test correction because the publisher
listed only unstaged paths. No compiler design change was involved. Publication
was repaired to enumerate all paths changed from HEAD; the exact non-cloning
assertion was published at `48236c4d22b7ead4f660c413fe8a31760baba795`.
Its build (38305977868), docs (38305977877) and verify (38305977916) passed.
Tests (38305978022) failed the new aggregate storage assertion: lazy State activation
installed its accounted bytes before reserving them. Classified as an implementation
accounting defect. The repair reserves State bytes before activation; the exact
storage and unique-leaf conservation assertions remain unchanged.

Storage repair: `a747e568686dd7a52c820d792b14ee3681f8f87a`.
All four push workflows passed: build 38306153091, tests 38306153088,
docs 38306153312, verify 38306153269. Debug/release each passed 1,656 tests:
all 1,581 predecessors and 75 new identities, with no failures/ignored/filtered.
The baseline identity inventory and reconciliation script were independently
checked against both decoded GitHub profile logs; that is analysis of remote
execution evidence, not local Rust test execution.

Final source audit found a shared-plan graph-depth defect: a cached child could
skip descendant depth on a later parent walk. The repair memoizes subtree heights
and checks the full prospective depth on cache hits, retaining shared plans.
A new control rejects a three-level graph at depth two and accepts it at depth
three. No cleanup order, ownership rule or semantic boundary changes.
