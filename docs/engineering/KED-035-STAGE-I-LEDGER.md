# KED-035 Stage I engineering ledger

Authority: Issue #84 comment 6296001700; frozen Draft 2 and its freeze record
6295819136; accepted K35-P01–P10 review 6295569435. The implementation branch is
`ked-035-approved-mir-execution-equivalence`; draft PR #85 targets `prod`.
Baseline commit `5f28977a42a7df1817e5cac46b3f6660a38435d3`, tree
`0b330328f3f213a26b6fc4af9f45ca06a98876b8`.

This is an implementation ledger, not acceptance or permission for Stage N.
GitHub Actions is the only Rust compilation/execution environment used. No local
Rust installation, repair, compiler, formatter, linker, or executable test is run.

## Focused K35-I01 / K35-I02 repair candidate

Sarah's independent review of M9 head
`4922296a04b479915ad58001275e491af1287d58` / tree
`08712b0f327e65f91ef39db4a19bad9311debd0e` identified two Major defects in
observation validation (Issue #84 comment 6305818246). The original handoff and
green workflows did not settle these valid cases. This section records Greg's
focused repair, not independent confirmation or acceptance. All implementation
history and the required M5 checkpoint remain; repair commits are normal
Conventional-Commit descendants of the reviewed head.

I01: `Validator::origin_pair` now resolves each source-global FunctionId through
its canonical ordinary MIR position before reading the parallel origin row. It
checks the row and charges the lookup/scan work. It does not treat a verified-ID
gap as unavailable evidence. Normal and stopped ordinary observations in mixed
files compare Equal, including function-boundary and expression/statement origins.
Foreign function/origin metadata rejects through bounded InvalidState.

I02: stored reference representation no longer creates a live Borrow link merely
because reference bits remain in a frame. Exact kind, referent type, frame/place
and canonical initializer/alias/transfer/caller-argument lineage are retained and
validated. Moved owners can leave unused shared-reference bits stored. Reached
reads/writes/reference transfers and held reference operands still require actual
available referents; operand overlap still enforces permitted shared/exclusive
access. No future-use, loan-count, recurrence or lifetime analysis is introduced.
The existing ownership checker remains the sole loan authority.
Reached reference accesses additionally match the actual captured pending
continuation; an old approved expression ordinal cannot authorize a later stale
read merely because the original owner is still available. The negative artifact
controls exercise both moved-owner and still-available-owner cases.

A pre-reserved bounded reference-value history records actual initialization and
Move state. It retains production occurrence across shared copies and exclusive
transfers, distinguishes repeated execution of the same borrow expression, and
rejects resurrection or duplicate exclusive representations of the same
production. A newly executed borrow and a dead alias from an earlier iteration
are different productions even when their frame/place bits match. Occurrence
metadata is derived from complete actual cuts; it is not a new runtime LoanId,
cleanup authority or public identity. Every scan charges comparison work; the
lineage cap bounds resolution; history count is capped by comparison nodes and
its storage/header is charged in comparison geometry. Primary runtime layouts,
logical charges, capture vocabulary and public APIs are unchanged.

The eleven repair identities are in `repair.tests` in the evidence manifest:
three mixed-layout/origin/isolation tests, seven borrowing/Move/provenance controls,
and one independent work/lineage-bound discriminator. Expected primary values are
explicit (bool true/false and exact nominal record with field 1). Positive tests
compare independent same-contract sessions as Equal under sufficient budgets.
Repeated loops include shared-to-exclusive access, record Move after field read,
and exclusive alias transfer across two iterations; repeated calls and distinct
expired exclusive borrows are also covered. Negative tests preserve foreign-frame,
wrong referent/permission, duplicate exclusive transfer, illegal expired source
use and a coordinated stale-use artifact. Equal-looking malformed copies reject.

All 2,045 predecessor identities and the 48 previously reviewed K35 controls are
retained. The map verifier pins the original 48 identity/classification pairs by
SHA-256 and checks that all additional identities are classified repair tests.
The complete expected per-profile total is 2,104 = 2,045 + 48 + 11, including the
unchanged authority compile-fail doctest and 1,800 affine cases. Tests execute
only in GitHub Actions. Initial candidate failures were new loop-fixture syntax
and Actions-reported formatting; they did not justify weakening predecessor or
ownership assertions. The final exact-head repair report supplies the fresh
complete eight-run gate and verified dual publication.

No specification or KD is reopened. The repairs implement existing KD-021/KD-024/
KD-029/KD-030/KD-039/KD-041 distinctions and K35-P03/P04/P07/P08 requirements.
No public execution/profile/resource/proof boundary or Stage N choice changes.
Finite tests are not universal conformance proof. Sarah's focused confirmation
and Greg's acceptance decision remain pending; PR #85 stays draft and unmerged.

## M0–M5 implementation and checkpoint scope

M0's exact-tree checkpoint and eight successful push/PR workflows are recorded
in `ked-035-evidence.json`. M1 published `KED-035-CONTRACT-INVENTORY.md`, including
baseline charge locations, public boundaries, domains, cuts and precedence.

M2's `execution::contract::Contract` is a bounded immutable borrowed view of the
same opaque `CheckedExecutable`. Extraction consumes the original validation and
entry gates, visits the entire structural ordinary call closure (including
empty-effect calls, recursion and unselected alternatives), and indexes exact
MIR operands/sites. The view retains all existing sealed nominal, origin,
ownership/outcome, cleanup/effect/domain and trusted facts through that checked
composite. Validation re-extracts canonical correspondence. Source similarity,
independent recompilation and independently changed schema/site/closure cannot
replace approval identity. This is no second ownership solver or execution IR.

M3 seals a versioned semantic/accounting/cut/observation/replay schema and 48
compatibility quantities into `Approval`. Reference interpreter capacities are
charged through that schedule. Accounted predecessor structs/enums have not been
expanded for instrumentation; new sidecars and scratch belong to separate
accounts. Reference-build sizes preserve the version-one implementation's logical
charges, not a native ABI. A later engine must retain logical capacity and scan
history even when physical allocation, frames or dead slots are eliminated.

M4 adds opt-in separately admitted non-owning capture on the existing Machine.
Ordinary no-profile and already selected-profile sessions use their original
execution authority. Capture is pre-reserved and records actual state, logical
continuations, counters, capacities, aliases, generations, Result parents,
active payloads, staging, replacement, temporary custody, trusted receipts,
productions, completion and retained event prefixes. Runtime custody markers
identify primary local transit and protected publication without owning, copying
or releasing a live resource. Capture failure leaves primary execution unchanged.
The terminal census and cached result come from the actual retained session;
repeated advance is read-only. Capturing does not consume reserved diagnostic
backing. A capture-complete flag alone is never a validity certificate.

M5 independently parses the flat record grammar and validates local shape,
nominal types, tags, canonical source/call/arm/region facts, active holders,
parentage, generations, aliases, payload/provenance associations, capacities,
exact compatibility storage, histories and primary delivery/stop consistency.
Both full views validate before one forward/inverse injective map is extended in
approved logical occurrence order. No raw addresses or raw cross-process IDs
are compared. Expression pointers are local immutable-MIR lookup keys only.
Malformed, truncated, foreign, stale, unmatched physical Host and exhausted
validation/comparison evidence yield Inconclusive, never Equal. A locally typed
engine defect with complete consistent evidence can yield Different.

All capture, graph, lineage, history and work traversal is bounded. Comparison
scratch is pre-reserved; successful output is a fixed scalar result. Flat Copy
records contain no recursive heap-owning payload or cleanup authority. Validation
never repairs the primary session or executes a cleanup plan. Existing whole-
Result cleanup authority remains restricted to its separately sealed domain.

## M5 evidence families

| Family | Discriminating evidence | Claim and limitation |
|---|---|---|
| Independent oracle | Hand-derived entire bool-return trace, every counter/capacity/cut, plus exact fuel 3/2 neighbors | Expected sequence is not generated by the interpreter, capture, replay or generous-run prefix. Static approved origins and sealed units are shared premises. |
| Observation boundaries | Independent exact record/storage sufficiency and one-short neighbors; exhausted capture and comparison work | Primary result/counters remain unchanged; incomplete evidence never establishes Equal. |
| Three outcomes | Complete Equal; actual well-typed wrong bool and conserving Sensor-parameter swap yield Different; malformed or incomplete views yield Inconclusive | Finite decisive fixtures, not universal equivalence proof. |
| Actual identity variation | Fresh selected realms and actual frame allocator seeds; shared alias equality and ancestry | Real interpreter executions, no native engine claim. |
| Analytic representation variation | Consistent relabeling of all dynamic record classes, including composite temporary owners and Result/provenance identities | Private schema/correspondence fixture; not independent execution or Stage N evidence. |
| Malformed independence | Foreign realm, stale generation/alias, bad tag/custody, unbound origin and coordinated corruption | Validation precedes projection; equal-looking malformed copies cannot self-certify. |
| Ordinary source | No-profile bool/range/record/Result, no-value calls, shared/mutable parameters, branch/match, short-circuit and loop fallthrough/break/continue; nested success/failure propagation | Existing source boundaries and selected-only execution retained. |
| Private protocol | Sealed synthetic Leaf acquisition, replacement and loop cleanup | Explicit private primitive/protocol evidence; grants no production source resource API. |
| Predecessors | Exact KED-034 1,981 predecessors plus 64 additions, including all inherited KED-033/032 identities, opaque-authority doctest and 1,800 affine cases | `verify-ked035-tests.py` reconciles identities rather than totals; no migrations or weakened assertions. |

## M6 — addressed replay and modeled abstract Host

`conformance/replay.rs` consumes a private bounded Input attached to the same
approval, schema, entry and ordinary execution limits. Sites use approved
function/origin or invocation identity, logical activation ancestry, site
occurrence and phase. They do not use a raw frame, ticket, allocation attempt,
source spelling, native address or backend instruction count. Positive logical
receiving growth remains an address even if a later engine preallocates; zero
growth is not a new capacity event. The analytic preallocation fixture tests this
address law without claiming a native implementation.

Static applicability, duplicate entries, response types, closed error masks,
prefix lengths and release birth associations validate before entry. Dynamic
responses are consumed only after inherited fuel/storage/depth and custody gates
reach the event. A missing reached response rejects without falling back to a
fixture. Future unvisited items remain unconsumed. Open/read/finish and existing
fixed release phases retain their original external protocol; instrumentation
adds no source operation or authority edge. Receipt-to-holder publication remains
atomic where the predecessor contract made it atomic.

Actual consumed controls and responses are independently checked against actual
receipt, production, event prefix, activation and resource association. Finish
partial failure retains its irreversible prefix; release failure retains the
protected outgoing Result without delivery. Modeled Host is a common logical
input only when its reached witness and primary stop agree. Surviving physical
Host remains unmatched/Inconclusive. Capture failure, invalid evidence and lost
substrate evidence cannot become Equal. No rollback, retry, implicit cleanup,
resume or fabricated opposite variant occurs.

M6 passed all eight exact-head workflow/job/step sets at
`79c5166f0a8178d73f1a3085f9178f89939b7e46` (37 additions, 2,082 per profile).
The preserved M5 checkpoint is
`e31f6104d8222ee201bc392a3daf87bcb03734ab`, tree
`34466f3032475e6b03931f59e61e712fab3b54e2`; its eight successful gates and
verified PR checkpoint publication preceded all M6 source changes.

## M7 — integrated authority, primary execution and presentation

`conformance/authority.rs` retains typed immutable references to the complete
approved type/MIR context, exact catalog, roots, structural call edges including
empty effects and skipped alternatives, invocation plans, cleanup facts and
temporary sites. Extraction and canonical revalidation have a separate bounded
row/work/storage account. Ordinary no-profile evidence does not obtain catalog
or execution authority. Possible structural effects and grants come from the
approved structure, never the runtime event prefix.

`Session` integrates ordinary Machine or the existing selected ExecutionSession.
The legacy run/CLI/profile/zero-parameter-root/verified/harness boundaries remain
unchanged. The only added public surface is bounded read-only contract and
conformance tooling over an existing CheckedExecutable. Controlled input and
modeled Host remain private. The summary renderer has a separate output allowance;
text, allocation failure and verbosity cannot replace primary execution or decide
semantic equality. The existing public audit renderer and its exact budgets are
unchanged. The new summary is not a serialization or external authority format.

M7's final source gate at `20d23dcafb0bceaf72e25e037c820fe6d3191143`
passed all eight workflows and every applicable job/step, including warning-free
Clippy, Rustdoc, formatting and both complete Cargo profiles. All 2,045 exact
predecessors and 48 classified additions passed: 2,093 per profile.

## M8 — complete finite evidence and documentation

`ked-035-evidence.json` is the detailed machine-readable requirement-to-code/test
matrix: §§4–18, all thirty §15 obligations, K35-P01–P10, malformed families,
M0–M9, evidence classifications, boundedness, public interfaces and deferred Stage
N evidence. `verify-ked035-contract.py` checks completeness and references.
`verify-ked035-tests.py` independently reconciles actual complete Cargo identities
in each profile, retaining the prior KED-034/033/032 maps, the opaque-authority
compile-fail doctest and all 1,800 affine cross-check cases. Existing predecessor
assertions and numeric boundaries are not weakened or migrated. Two forged
Approval fixture constructors merely supply the new original schema/schedule
fields; their malformed predicates remain unchanged.

Independent decisive evidence includes the hand-derived whole bool trace and
first-insufficient fuel trace, external event prefixes for every 1–72 fuel
neighbor of the exact predecessor direct-open/finish source, terminal mutation
and range-trap custody, actual two-resource association swaps, actual wrong read
payload with the same final Error, and completion/release-phase responsibilities.
The finite larger sample enumerates every distinct reached logical storage gate
and checks first-insufficient precedence and custody; discovery selects budgets,
not expected success snapshots. Recursive depth, short-circuit nonconsumption,
loop activation and terminal repetition are exercised. Finite fixture enumeration
is not proof of every program or every conceivable substrate failure.

Malformed evidence is altered independently of its bound authority or oracle:
schema, closure, site, nominal tag/payload, active holder, realm, alias, generation,
continuation, branch target, scope, completion phase, production/invocation,
accounting, origin and replay marker corruption all reject before projection.
Equal-looking jointly corrupted copies cannot self-certify. Full view validation
precedes mapping. A valid but wrong actual read or resource association can yield
Different; incomplete or malformed views yield Inconclusive. Analytic raw-ID
relabeling is explicitly separated from real interpreter identity variation and
from independent expected observations.

The five canonical AI documents record this staged candidate, preserve human
acceptance authority and defer native work. The generated context bundle is
regenerated using `scripts/build-ai-context.py` and checked byte-for-byte in CI.
No accepted language-decision number or human acceptance is invented.

## Binding implementation constraints

| Finding | Implementation disposition |
|---|---|
| K35-P01 | Borrow and validate complete exact approved authority; no second lowering or ownership authority. |
| K35-P02 | Sealed version-one common schedule and separate observation/audit/presentation domains; exact predecessor neighbors retained. |
| K35-P03 | Bounded pre-reserved complete actual observation; failure preserves primary and prevents Equal. |
| K35-P04 | Independent local validation before one anchored injective dynamic map; raw IDs are never cross-engine equality. |
| K35-P05 | Logical site/activation/occurrence/phase and receiving-growth shadows; future engines must preserve eliminated logical work. |
| K35-P06 | Reached-only consumption, inherited gate precedence, no-gap custody, protected non-delivery and terminal non-resumption. |
| K35-P07 | Independent whole-trace and custody oracles plus actual Equal/Different/Inconclusive controls; no interpreter self-certification. |
| K35-P08 | Ordinary and selected-profile integration without broader source/runtime authority; structured audit is separate from events/text. |
| K35-P09 | Preserve the semantic relation independently of layout, frame IDs, preallocation or future optimization; no optimizer chosen. |
| K35-P10 | Mandatory native identity/installation/actual optimized and unoptimized comparisons remain deferred Stage N evidence. |

## Bounds, accounting and custody

Contract extraction admits nodes/work/storage and accounts simultaneous canonical
re-extraction buffers. Capture uses flat non-owning Copy records, finite lineage,
record/storage/work maxima and pre-reserved storage. Comparison pre-reserves
bounded nodes, links, slots, histories, event scratch and both map directions;
validation and comparisons charge work and return a fixed scalar. Replay admits
finite sites/items and conservative work for counters, ancestry and histories
bounded by execution fuel/depth; it performs no allocation during binding.
Authority rows and revalidation buffers have their own budget. Evidence disposal
owns no resource and invokes no source cleanup plan. All size arithmetic uses
checked admission or fails conservatively. Finite maxima and exact defaults are
in the implementation; reference geometry is not a native ABI guarantee.

The 48 sealed quantities preserve predecessor receiving capacity and prospective
combined sums. Accounted Frame/Task/Value/Fault/State layouts were not expanded;
Machine sidecars are outside those existing logical allocations. Whole-Result
cleanup-plan identity does not grant authority to execute cleanup. Structural
cleanup-effect unions remain static even though only one runtime payload is
active. Primary local transit/publication markers observe custody without
creating another owner or second cleanup authority.

KED-034 numeric measurements retain audit 65,617 storage / 65,584 work;
diagnostic 126 and result 20; catalog diagnostics 9,792,
protocol bytes/site 1,320, storage 1,695,808, work 102,656; combined receiving
failure 3,848 (retained 3,672, largest 176, sum growth 680); physical growth probe
mask 1,918; preparation diagnostics 44,608, identifier bytes 13, storage
18,559,608, tokens 35, work 1,344,662,663; direct-open/finish runtime fuel 72 /
storage 4,448; stopped-report audit storage 69,969 with 4,352 retained. These are
reference Rust 1.80.0 x86_64 test-build evidence, not portable physical costs.

Audit text output measured 6,193 bytes in the fresh M0 suite and 6,199 in the
expanded Stage I suite. The unchanged renderer includes process-local catalog
serials and packed operation identities in Debug text; extra preparations alter
those identities and their decimal widths. A fixed cross-run text length is not
a semantic invariant. The original tests still require exact sufficient and
one-byte-short output allowances for each actual approved artifact, and the
renderer, allowance rules and static authority are unchanged. No normalization,
old-test relaxation or authorized-boundary migration is applied.

## Public API and changed-file accounting

Public metadata additions are `execution::contract::{Schema, Schedule, Domain,
Limits, Role, Site, Contract}` and its read-only extraction/validation/accessors;
`execution::conformance::{Limits, Incomplete, Session, Comparison,
ComparisonLimits, compare}` including Session construction, advance, capture
status/count and bounded authority-summary rendering. `Schema::V1` is sealed;
public observations cannot construct Approval. `completeness()` reports capture
completion, not a semantic validity certificate. There is no public replay input,
resource constructor, new profile or engine-selection interface.

Implementation files comprise contract.rs; execution/mod.rs; Machine and its
cleanup, replacement, results, temporaries, trusted and completion modules;
conformance.rs, authority.rs, compare.rs, replay.rs and their dedicated identity,
oracle and replay tests; and the two forged-Approval fixture modules. Documentation
comprises five canonical documents, their generated bundle, this ledger, the
contract inventory and JSON evidence map. CI changes comprise docs.yml, tests.yml
and the two new KED-035 verifiers. The final M9 report gives the exact head's full
changed-file list and tree; no unrelated repository configuration is changed.

## M9 — final gate and retained authority

The final exact-head M9 gate must independently pass complete debug/release tests,
both applicable events' build/tests/verify/docs, formatting, Clippy and Rustdoc
with warnings denied, ledger/manifest verification, generated-context clean diff,
exact predecessor identities and numeric neighbors, and clean worktree/index.
The same final report is published and read back on PR #85 and Issue #84. Its
head/tree and Actions URLs are external exact-head evidence and cannot be embedded
in their own content-addressed commit. This ledger does not claim a future gate
has passed; the verified M9 publications record that result.

Some early implementation commit subjects used non-prescribed prefixes. This is
a nonfunctional repository-process deviation; history and the required M5
checkpoint are retained without rewriting. Subsequent subjects follow AGENTS.md.
No local Rust installation/repair/compile/link/formatter/executable test occurred.

Remaining limitations: finite fixtures are not universal equivalence proof;
private Leaf/protocol fixtures are not production source permission; analytic
relabeling/preallocation are not native tests; contract/static rows use exact
in-process immutable references, not a portable transport; insufficient evidence
is deliberately Inconclusive; no performance improvement is claimed. Native
production, target/runtime installation, pre-execution artifact mismatch checks,
actual optimized/unoptimized comparisons and physical/native/substrate evidence
remain mandatory deferred Stage N obligations. No backend, target, ABI, layout,
emitter, object format, runtime ABI, linker, optimizer, serialization, signing or
packaging mechanism is selected.

Sarah's independent review and Greg's acceptance decision remain pending. No
acceptance, freeze change, ready-for-review transition, approval, issue closure,
merge or later project step is authorized. The PR remains draft and unmerged;
only a human may merge.
