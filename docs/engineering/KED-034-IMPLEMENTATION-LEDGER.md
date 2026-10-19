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

## M1: catalog and frontend (validation pending)

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
and retained Sensor custody at disabled dispatch. Passing claims await CI.

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
