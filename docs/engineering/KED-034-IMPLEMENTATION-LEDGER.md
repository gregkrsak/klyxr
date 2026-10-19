# KED-034 implementation ledger

Frozen authority: Issue #82 Draft 2; freeze 6274604644; implementation
authorization 6274641666; accepted K34-P01 through K34-P09. GitHub Actions is
the sole authorized Rust execution environment under directive 6275740196.
No local Rust execution is permitted. Existing failure evidence is preserved.

## Frozen baseline and M0

- Commit: `372230df1d359b13d28f5c136130ac031507716a`
- Tree: `a646e9576a9b7650ff9a95275f1080b591d06986`
- Branch: `ked-034-explicit-fallible-completion`; draft PR #83, target `prod`.
- Empty published M0 commit: `5ba9cebc8ba0d5dff672ec4f6e25354d2a358c5e`.
- M0 commit tree is exactly the frozen tree; changed files: zero.
- All eight fresh push/PR runs passed, including every job and step.
- Push and PR debug/release logs independently reconcile 1,981 identities:
  all 1,875 pre-KED-033 predecessors plus 106 KED-033 additions, with zero
  failed, ignored, filtered or migrated identities. Authority doctest and
  1,800-case affine checks passed in each profile.
- Build, formatting, Clippy, Rustdoc, CLI, assets, scaffold and context
  reproducibility passed through the unchanged repository workflows.

Exact run/job inventory and independent log reconciliation are published in
[PR M0 report](https://github.com/gregkrsak/klyxr/pull/83#issuecomment-6275856224)
and [Issue M0 record](https://github.com/gregkrsak/klyxr/issues/82#issuecomment-6275856656).
Both comments were retrieved and verified before functional editing began.

Git HTTPS publication lacked credentials. The API published the single remote
M0 commit; the unpublished local candidate was aligned to its exact object.
No shared history was rewritten. Local parallel and serial generated-artifact
failures remain recorded separately and are not Klyxr nonconformance evidence.

## M1: catalog and frontend (Actions verified)

The same closed catalog gains one owned completion operation, exact singleton
FinishReceipt range, canonical Result and finish-only Error subset. Existing
open/read/release row IDs and catalog origins remain intact; the new range and
operation use distinct unused catalog rows. Exact release authority is required
for consuming completion as well as acquisition. Existing separate namespaces,
nominal ranges and Result rules remain authoritative.

Completion dispatch is explicitly disabled until M4. M1 adds only a terminal
guard at that runtime seam; no external finish operation is implemented or
simulated. Twelve proposed regression identities cover the contract, source
composition, independent denial/release grants, receipt status formation,
nominal/literal/type rejection, direct effects, malformed unused descriptors
and retained Sensor custody at disabled dispatch. All eight Actions runs passed at `65f9b8344768019ecd4fc686f0b8bd5e9a920f34`
(tree `0ae44bfa930dd87ada97aecb90f9c74a7eeda73d`). Push and PR logs
reconcile 1,993 tests per profile: all 1,981 predecessors plus twelve additions,
zero failed, ignored, filtered or migrated identities. Run/job inventory:
39314892235/118273678532, 39314892259/118273679128,
39314892229/118273679209, 39314892231/118273678841,
39314885456/118273658747, 39314885521/118273660359,
39314885486/118273660273, 39314885515/118273658103.
All eight job conclusions and steps were inspected.

The predecessor selected-range inventory assertion is extended from two to
three and its loop now verifies all three imported ranges. All original
Reading/SensorError ID, domain and origin assertions remain. This is explicit
inventory growth required by the frozen feature, not a weakened predicate or
test-identity migration. Explicit predecessor grant fixtures retain their old
open/read/release booleans and deny the new finish operation unless ALL is used.

The new reconciler checks every one of the 1,981 frozen predecessor identities,
all classified additions, zero failures/ignores/filters/migrations, the complete
KED-033 evidence map and authority/affine identities. The workflow retains both
complete unfiltered profiles. The staged manifest is not a completed M9 map.

## M2: sealed checker and MIR correspondence (Actions verified)

The existing successful ownership consume path emits a distinct invocation
cleanup holder. It is not a source Place and confers no move permission.
The same cleanup-effect table carries the release obligation independently
from the direct finish effect. Approval seals input and completion holders,
protected outcome, fixed pair, grants, effects, all four continuation origins,
and the ordered eight-phase protocol. Derivation requires exactly one real
checker witness per completion and rejects orphan witnesses. Private controls
mutate holder roles, protocol order, contracts, origins, all retained copies
and cleanup witnesses at the opaque validation seam. Dispatch remains disabled.

All eight runs passed at `23db19e921f4d803a9a7246d96560d1ede299460`
(tree `36d0274ccf9a89bbdde4c3e3a2ffe29461b0c288`): 1,999 tests in each
profile, all 1,981 predecessors retained. Run/job pairs:
39316093077/118277825920, 39316093242/118277826759,
39316093262/118277835745, 39316093226/118278041914,
39316086017/118277800051, 39316086055/118277800149,
39316086032/118277800290, 39316085803/118277799443.

## M3: receiving envelope (Actions verified)

A pending record retains the exact current temporary identity and future
completion/protected-result roles. One checked sum reserves provenance, seven
protocol events, Result cell, operand, transfer/register events, temporary owner,
fixed cleanup work and two continuations together. Actual diagnostic storage
is reserved before handoff and moves into terminal reporting without allocation.
The pre-dispatch guard remains enabled: no finish attempt or consume occurs.
Tests pressure direct and relay-returned owners, a real diagnostic Host cut,
simultaneous capacities and measured storage neighbors. All eight Actions runs passed at `7498484d709f4cd4863591e07de797c860a2ea13`
(tree `e06c93c43fc1383e115bfebf7b92d4c59234e4b3`), 2,003 tests per profile.
Run/job pairs: 39317058163/118281154313, 39317058161/118281154457,
39317058091/118281154246, 39317058117/118281153996,
39317048287/118281119343, 39317048289/118281119864,
39317048423/118281120954, 39317048447/118281121438.

## M4: sealed completion enabled (Actions verified)

Only after the complete M3 snapshot passed all eight workflows, completion
was enabled. The current temporary transfers once into pending completion
custody; the physical Sensor remains in the machine entry table. The fixed
binding records attempt, optional nonempty partial prefix, and ordinary response.
A separate protected Result cell contains only receipt/error data. Both
alternatives run the existing cleanup work/current cursor and exact fixed
release before the delivery continuation publishes the Result. Every phase
is terminally retainable; no retry or owner restoration exists. New tests
cover all three scenarios, malformed response retention, protected outcomes
at Host/release failure and every successful-trace fuel cut. M1/M3 guard
assertions remain as private test-only boundary cuts; their exact pre-enable
Actions snapshots remain historical evidence. No frozen predecessor changes.

All eight Actions runs passed at `5803b6af6b9340da0c36fc679f8a8db14a990179`
(tree `64cf0f988cb2d5fabefc59a651dc4d119ea25c0f`), 2,008 tests per profile.
Run/job pairs: 39317881855/118283983575, 39317881805/118283983117,
39317881600/118283985479, 39317881628/118283981807,
39317874839/118283958364, 39317874808/118283958677,
39317874705/118283957830, 39317874676/118283957652.

## M5: source composition checkpoint (validation pending)

Thirteen source tests add the defining Sample, last shared use/live-handle
rejection, owned helpers, relay temporaries and nested protected returns, both
outcome selectors, consumed-owner exclusions, branch finish versus release-only
exit, iteration-local finish and existing loop-carried rejection, written
Result-argument order, later outer range failure, source-formed status data and
verified-domain exclusion. Replacement/reinitialization remain forbidden under
the existing Sensor destination policy; this is not a new replacement mechanism.
The defining source and two discriminating rejection examples are retained.
M5 must obtain complete exact-head workflow evidence before M6-M9 begins.

## Remaining gated milestones

M2 seals invocation custody/effect evidence; M3 establishes the complete receiving
envelope while invocation remains disabled. M4 enables sealed completion and the
existing fixed release. M5 adds full source composition and must be preserved as
a distinct exact checkpoint with complete workflow evidence before M6-M9.
M6 completes malformed controls, M7 exact bounds, M8 audit/docs and M9 full
conformance. Every frozen section 13/14 obligation, P01-P09 constraint and
milestone must receive production-code and discriminating-test mappings.

No semantic revision, acceptance, approval, ready transition, issue closure or
merge is authorized. Independent Sarah review and Greg's acceptance remain
separate. Only a human may merge.
