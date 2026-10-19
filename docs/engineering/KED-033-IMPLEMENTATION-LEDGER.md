# KED-033 implementation ledger

## Authority and fixed baseline

Greg separately authorized implementation of frozen Draft 2. Governing records:

- [Issue #78](https://github.com/gregkrsak/klyxr/issues/78)
- [Freeze and mandatory constraints](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6258324041)
- [Implementation-pressure review](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6258241641)
- [Sarah's focused confirmation](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6258020728)
- [Sarah's original review](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6257518197)
- [Auditability-first doctrine](https://github.com/gregkrsak/klyxr/issues/52)

Baseline commit: `62023217b3f93e53cae302731643cbcea1fb3d17`.
Baseline tree: `3998bcad9b34d66e960b863eb20b7b78680da07d`.
Branch: `ked-033-trusted-effectful-operations`; PR target: `prod`.

Remote prod and the verified signed Git object matched the assigned baseline.
The object was fetched through the connected Git API, its exact Git hash verified
locally, and the branch created once from that exact commit. The preceding
checkout/index were clean. No accepted implementation was discarded or rewritten.
The production Arnold charter remains governance commit
`2887c9239fb6f82ded4fd7b2d8f5f592b725cccc`, blob
`b04c4ba5251fdcacb5278ba26c0aaf8a06b4db7e`. Exact-tree AGENTS.md and the five
canonical documents were read in order during the immediately preceding pressure
review; their identical tree and controlling requirements remain verified.

Freeze does not constitute implementation acceptance. Greg retains acceptance
authority; only a human may merge. No KED-034 work is authorized.

## M0 environment and predecessor evidence

The accepted Rust 1.80.0 installation remains available at
`/tmp/rust-clean-vywtlaad/toolchain/bin`. All seven compiler/component executable
sizes, modes and SHA-256 hashes matched the accepted snapshot before use. No
installation, repair, permission change or process-table inspection was performed.
One task-owned target is used: `/tmp/klyxr-ked033-target-ltwgzadw`.

The complete local predecessor gate passed at the untouched exact baseline:
**1,875 identities in debug and 1,875 in release**, independently reconciled by
`scripts/verify-ked032-tests.py` as 1,846 predecessors plus 29 KED-032 additions.
The compile-fail authority doctest and 1,800-case affine cross-check passed in each
profile. Locked all-target checking, formatting, warnings-denied Clippy/Rustdoc,
Result and propagation CLI controls, ordinary/replacement/effect examples, battery
positive/negative diagnostics, README assets, scaffold and byte-identical generated
context all passed. Worktree, index and diff checks were clean. Negative controls
returned expected class 1; every positive command returned zero.

This uses the normal local workflow while it remains reliable. GitHub Actions is
authoritative if the known local execution problem recurs; no toolchain repair or
experimental target replacement is permitted. Historical ps/procfs namespace
limitations remain ancillary; ps and related process-table utilities are not used.
The historical shell Git proxy is not repaired: verified local Git objects may be
published using the connected API. CI is not substituted for a falsely claimed
local result. Exact milestone commit/tree and run/job evidence is recorded as
it becomes available, without self-referential commit hashes in this file.

## Mandatory constraint map

| Constraint | Implementation milestones | Evidence location planned |
| --- | --- | --- |
| P01 catalog collection and honest origins | M1 | trusted catalog/resolver tests; manifest |
| P02 genuine Resource category and exclusions | M2, M4 | type/publication/plan/eligibility tests |
| P03 canonical calls and argument destinations | M3, M4 | signature/ownership/hold/outcome tests |
| P04 no-gap receipt and trusted Result provenance | M4, M5, M7 | actual invocation/receipt/publication probes |
| P05 persistent stopped session | M5, M7 | session/census/terminal non-delivery tests |
| P06 ownership-derived cleanup effects | M3, M6 | retire versus relay and structured-exit tests |
| P07 exact grants, release and binding approval | M4, M7 | independent authority/descriptor mutations |
| P08 explicit roots and structural audit reachability | M4, M8 | graph/root/grant/audit tests |
| P09 complete bounded accounting | M1-M9 | exact neighbors, allocation probes, audit/diagnostic tests |

No acquisition, including the deterministic fixture, may execute before M5 has
both the pre-reserved receipt and persistent terminal-session owner in place.

## Frozen section 20 evidence plan

These are planned obligations, not passing tests at M0. S = ordinary source through
the genuine selected catalog/binding; F = fault at an actual boundary; M = private
malformed artifact. The final manifest will enumerate exact identities and lanes.

| Items | Required witness | Milestones / lanes |
| --- | --- | --- |
| 1-5 | exact operation resolution, denied/missing entries, collisions, cross-namespace and case/unselected controls | M1, M3, M7 / S,M |
| 6-8 | nominal range domains/operations, exact operation Error subsets, invalid imported values retained with Integrity | M1, M5, M7 / S,F,M |
| 9-10 | open Ok fresh unique owner; each Error no owner or cleanup | M5-M7 / S,F |
| 11-12 | read alternatives preserve the Sensor and aliases | M5-M7 / S,F |
| 13-15 | full propagation, protected Ok/Error cleanup, Limit/failure non-delivery | M6-M7 / S,F |
| 16-17 | holds, moves, duplicate cleanup, forged/stale handles | M3, M6-M7 / S,M |
| 18-19 | parameters/relay, excluded references/conditional/replacement/fields/egress | M2-M7 / S,M |
| 20-23 | complete effects, retire/relay, grants versus io, exact cleanup-only binding | M3-M7 / S,M |
| 24-29 | typed terminal separation, no fallback, correspondence, no-attempt/pre-prefix and retained post-prefix stops | M4-M7 / S,F,M |
| 30-32 | every structured exit/destination and exact resource/work/storage neighbors | M6-M7 / S,F |
| 33-35 | rooted grant/use audit, bounded rendering, verified/harness exclusion and proof preservation | M8-M9 / S,F,M |

## Frozen section 21 malformed controls plan

Each row requires independent controls, not one compound corrupted artifact.

| Mutation | Milestone / expected boundary |
| --- | --- |
| operation/resource IDs | M4,M7 / pre-dispatch rejection or retained malformed receipt |
| catalog/profile identity | M4,M7 / immutable approval |
| parameter order/type/mode | M3,M4,M7 / signature and destinations |
| result/Result-plan identity | M2,M4,M7 / publication and correspondence |
| effects | M3,M4,M7 / complete independent contracts |
| acquisition/release pairing | M2,M4,M7 / exact cleanup-only grant/binding |
| fresh identity | M5,M7 / receipt-held payload and session realm |
| source/HIR/MIR/catalog origins | M1,M4,M7 / exact provenance |
| runtime binding identity | M4,M5,M7 / no substitution/fallback |
| destination/protected holder | M4-M7 / no duplicate transfer or premature delivery |
| cleanup plan/realization | M2-M7 / actual scopes and completed prefix |
| returned tag/active payload | M5,M7 / Integrity without lost custody |
| authority membership | M4,M7,M8 / independent copies and rooted authority |

## Milestone status

M0: baseline and branch established; ledger created before semantic changes.
M1 and M2: catalog/range/origin admission and canonical Resource eligibility are
implemented; milestone validation below. M3-M9 remain pending. No KED-033 invocation path is enabled at
this checkpoint. In particular selected trusted calls still reject explicitly;
they cannot route to an ordinary function or fixture before M5.

### M0 published checkpoint

Commit `3c88f3f38a86182ad5193db9b16521f6ee9fd272`, tree
`ba6e05ba571b00dd96e50b1fab233e2d66091a3f`, establishes this ledger. All four
push workflows and every job/step passed:

| Workflow | Run | Job |
| --- | --- | --- |
| build | [39190661833](https://github.com/gregkrsak/klyxr/actions/runs/39190661833) | 117862503061 |
| tests | [39190661856](https://github.com/gregkrsak/klyxr/actions/runs/39190661856) | 117862503113 |
| verify | [39190661865](https://github.com/gregkrsak/klyxr/actions/runs/39190661865) | 117862503206 |
| docs | [39190661926](https://github.com/gregkrsak/klyxr/actions/runs/39190661926) | 117862503128 |

[Issue progress record](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6258480234).

### M1 implementation details and evidence

One explicit immutable sensor-demo selection records its scenario, independent
open/read/release grants and bounded named entry roots. It supplies only fixed
compiler-owned rows; ordinary source cannot author descriptors or bindings.
Range rows precede source declaration/signature collection, with exact domains
and nominal identities. Resource and operation IDs carry distinct checked
collection serials; matching numeric row offsets in another collection are not
authority. Range declarations now distinguish Source spans from Catalog origins;
no catalog row is assigned a synthetic source location.

Selected operations occupy only callable names and selected types occupy only
type names. Denied rows still occupy their names, collection rejects collisions
without overwriting the catalog, and ordinary values/fields retain their separate
namespaces and original case rules. Imported ranges retain existing literal,
subtraction, comparison, Copy and field behavior. Every row is validated even
when unused/denied; fixed release has no source-callable name. No private resource
record fixture or acquisition path is activated.

The source-independent admission envelope adds 65,536 bytes plus 4,096 bytes per
root plus 16 times root-name bytes, 65,536 work plus 4,096 work per root, and 4,096
diagnostic bytes plus 16 times root-name bytes. The conservative terms cover
catalog rows, selected configuration, retained approval copies, root scans and
diagnostics. They are summed with frontend, outcome and cleanup envelopes, rather
than compared in isolation. Exact sufficient and immediately insufficient
combined neighbors are tested independently. The no-profile terms stay zero.

Sixteen new identities are enumerated and classified in
`ked-033-evidence.json`; `ked-033-predecessors.json` preserves all 1,875 exact
baseline identities and `verify-ked033-tests.py` reconciles both profiles without
permitting unclassified additions. Predecessor fixtures receive only their honest
Source-origin wrapper; no expectation or numeric protocol oracle changes.
Ordinary development corrections were limited to test fixtures initially using
unsupported syntax (`+`, missing `range`, and a reserved Result-family name).
Those fixtures were corrected to the existing syntax rather than changing it.

The complete M1 local gate passed: **1,891 exact debug identities and 1,891
release identities** (1,875 predecessors plus 16 additions), authority doctest,
both 1,800-case affine checks, all-target checking, formatting, warnings-denied
Clippy/Rustdoc, all predecessor CLI/examples/rejection diagnostics, README assets,
scaffold, context reproducibility and diff integrity. All existing numeric
protocol oracles passed unchanged. The development diff is the authorized M1
change, not a new claim of baseline cleanliness; index/worktree identity is
reconfirmed at the published checkpoint.

M1 does not claim typed Sensor eligibility, trusted execution, stopped custody,
structural audit, or completion of P02-P09. Those remain later milestones.

### M1 published checkpoint

Commit `dfc63bb22166eb3335d4ae870977b69ca393caec`, tree
`34116e0ffc71c91c03fc7b6a90a4abcda161a10b`. All four push workflows and every
job/step passed:

| Workflow | Run | Job |
| --- | --- | --- |
| build | [39191315743](https://github.com/gregkrsak/klyxr/actions/runs/39191315743) | 117864549739 |
| tests | [39191315678](https://github.com/gregkrsak/klyxr/actions/runs/39191315678) | 117864549494 |
| verify | [39191315629](https://github.com/gregkrsak/klyxr/actions/runs/39191315629) | 117864549001 |
| docs | [39191315649](https://github.com/gregkrsak/klyxr/actions/runs/39191315649) | 117864549143 |

[Issue progress record](https://github.com/gregkrsak/klyxr/issues/78#issuecomment-6258579732).

### M2 implementation details and evidence

Sensor now has its own canonical Resource type in value, expression, referent,
resolution and Result-child vocabularies. It has no RecordId, public fields or
representation constructor. The fixed descriptor records its exact cleanup-only
operation, io contract, catalog origin and conservative 64-byte binding-payload
bound. Typed cleanup references distinguish Record from Resource without changing
the private synthetic catalog or enabling its record-field permissions.

The existing ownership checker treats Resource owners as Move and shared handles
as Copy, preserves last-use/held-argument behavior and rejects duplicate moves.
Canonical resource interface validation checks unused/foreign types and excludes
mutable Resource references. Mutable borrows, conditional initialization,
replacement, fields, representation inspection, arithmetic, comparison and
dereference cannot construct or duplicate Sensor. Resource-bearing public Result
egress rejects before machine entry, even on an inactive resource alternative.
Ordinary owned helper transfers, non-escaping shared helpers and Resource Result
payloads pass through the existing pipeline.

The existing custody product now carries typed plans. Its actual successful
transfer gate removes a relayed Sensor before parameter retirement; a retained
Sensor instead emits io cleanup evidence. Thus retire requires io while relay
does not, already proven without invoking a fixture or introducing a second
ownership/effect checker. MIR cleanup and temporary plans preserve Resource
identity. The private runtime value shape is present but deliberately cannot be
constructed yet; receipt/session dispatch remains disabled until M5.

Nine additional identities bring the inventory to 25 KED-033 additions. Complete
local validation passed: **1,900 exact debug identities and 1,900 release
identities**, all 1,875 predecessors, authority doctest, both 1,800-case affine
checks, check/fmt/Clippy/Rustdoc, CLI/examples/rejection diagnostics, assets,
scaffold, context reproducibility and diff integrity. Numeric protocol oracles
remain unchanged. Existing malformed Record-plan fixtures receive only the typed
Record wrapper, retaining their original corrupt IDs, expectations and outcomes.

Ordinary development corrections included extending the canonical borrow
validator for shared Resource referents and checking foreign Resource interfaces
at the typed-HIR publication boundary rather than adding type inference to the
ownership checker. No frozen source behavior was revised. M3-M9, actual trusted
calls/receipts/binding, structural audit and independent acceptance remain open.

Future entries must record meaningful changes, validation and exact remote evidence,
ordinary development corrections, any K33-I escalation, predecessor adaptations,
accounting constants, documentation/context outcomes and residual limitations.
