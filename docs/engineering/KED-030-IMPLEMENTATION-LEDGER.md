# KED-030 implementation ledger

## Authority and baseline

Frozen Draft 2: [Issue #70](https://github.com/gregkrsak/klyxr/issues/70).
Freeze: comment 6224699062. Separate implementation authorization: comment
6224708500. Binding implementation-pressure review: comment 6222126054.
Arnold charter blob: `b04c4ba5251fdcacb5278ba26c0aaf8a06b4db7e`.

Authorized baseline commit: `edf80c1e68e6fe3e2b25f6a557fc121395e45adc`.
Baseline tree: `844afe3deacf22bc55844bc8ea92d76036cb3fc2`.
Branch: `ked-030-explicit-transitive-effects`; target: `prod`.

The unchanged branch passed all four push workflows:

- [build](https://github.com/gregkrsak/klyxr/actions/runs/38968144093)
- [tests](https://github.com/gregkrsak/klyxr/actions/runs/38968143774)
- [docs](https://github.com/gregkrsak/klyxr/actions/runs/38968143743)
- [verify](https://github.com/gregkrsak/klyxr/actions/runs/38968144001)

Job 117199623757 reconciled 1,706 historical predecessors plus 37 KED-029
additions = 1,743 passing identities separately in debug and release. Each profile
passed the compile-fail authority doctest and 1,800-case affine cross-check;
failed/ignored/filtered were zero. Build, documentation and verifier job logs and
step conclusions were inspected. This is GitHub-executed baseline evidence.

Local Git HTTPS push had no credentials; branch publication uses the connected
GitHub app. No local Rust/toolchain execution or repair is used as evidence.

## Milestone 1 — Canonical atoms and bounded sets

The eight independent atoms use canonical enum identities and a private one-byte
set. Source-name resolution is separate from runtime operation dispatch. Union,
subset checking and canonical rendering allocate no semantic authority. Wider
encodings reject before narrowing. Membership, including `unsafe`, grants no
safety permission.

Three new unit-test identities cover all atoms, duplicate-insertion detection,
unknown names, all 65,536 pairs of valid sets, and all invalid wider encodings.
GitHub milestone commit `cbd35c657ec3ee45aacdeb98f4c3a844d67ef27d`, tree
`91a58f443813d61ef29a3fefdf853845dc43d07b`, passed all four workflows:
[build](https://github.com/gregkrsak/klyxr/actions/runs/38968477589),
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38968477737),
[docs](https://github.com/gregkrsak/klyxr/actions/runs/38968477669),
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38968477901).
Job 117200734355 reconciled 1,743 predecessors plus three additions = 1,746
passing identities in each profile, with both authority/affine controls passed.
All four job conclusions and relevant logs were inspected.

`ked-030-predecessors.json` preserves the complete baseline identity set.
`verify-ked030-tests.py` requires every predecessor plus exactly the explicitly
listed additions in each complete profile, retaining affine/doctest controls.
Historical KED-027 through KED-029 manifests and reconcilers are unchanged.

## Milestone 2 — Ordinary grammar and canonical declarations

Contextual clauses preserve raw member and introducer spans only on ordinary AST
functions. Resolution collects canonical contracts with all signatures before
bodies. Ordinary typed HIR and MIR retain one bounded declaration representation
with at most eight member origins; publication/structural validation checks its
internal consistency. No verified or harness metadata is added. Independent
approval correspondence includes declarations through existing exact equality.

Five integration identities cover result kinds/spellings, retained metadata and
execution of unused declarations, lexical compatibility, forward/recursive
signatures, malformed lists, and verified/harness rejection controls. One unit
identity mutates declaration mask/origin copies independently. Execution remains
pending until the milestone workflows are recorded. Direct/transitive body subset
checking is milestone 3; this milestone alone is not a completed effect system.

One advisory `cargo fmt --all` invocation could not run because Cargo is absent.
No toolchain repair/probing followed. Formatting is checked by GitHub Actions.

## Remaining milestones

Direct/transitive checking; operation contracts and
derived summaries; checker-produced cleanup custody, MIR and approval; replacement
and temporaries; private abort domains; malformed/bounds/terminal evidence;
documentation and complete conformance remain unimplemented at this milestone.

No production resource API, unsafe permission, effectful proof semantics, new
ownership checker, source recoverable failure or unwind is introduced.
Implementation completion, independent acceptance and merge are not claimed.

## Milestone 2 first push and correction

Commit `4ebdf2beeb6dc80a5ad4e1f06cb8343aacc10198`, tree
`7ad86db19b49372b28d556e6a160ef908ef6a5cc`, compiled all targets.
[Build 38969016547](https://github.com/gregkrsak/klyxr/actions/runs/38969016547)
stopped at formatting; its exact rustfmt diff was applied. Clippy and Rustdoc were
not reached on that push. [Tests 38969016526](https://github.com/gregkrsak/klyxr/actions/runs/38969016526)
passed the debug test bodies but stopped in reconciliation: the five new
integration identities were listed as `effects.rs::...` instead of Cargo's actual
`tests/effects.rs::...`. Only those new manifest entries were corrected; no test
assertion or predecessor identity changed. Release was not reached.
[Docs](https://github.com/gregkrsak/klyxr/actions/runs/38969016525) and
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38969016520) passed.
These are implementation formatting/evidence-manifest errors, not platform
failures. No later architectural layer was added on the failing milestone.

## Milestone 2 completed

Correction commit `3db5aba640885810a04df74824160ac71efbd1f7`, tree
`096fa362e59e8d543126a40bb96faa10cf2af3ec`, passed all four workflows:
[build](https://github.com/gregkrsak/klyxr/actions/runs/38969187579),
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38969187747),
[docs](https://github.com/gregkrsak/klyxr/actions/runs/38969187827),
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38969187641).
Job 117203057962 reconciled 1,743 predecessors plus nine additions = 1,752
passing identities in both profiles; both affine and authority controls passed.
All four job conclusions and relevant logs were inspected before milestone 3.
Declaration origins share an immutable fixed-size array; absent clauses allocate
none. Only ordinary function representations contain those origins.

## Milestone 3 — Direct and transitive contracts

A structural visitor collects complete canonical call declarations, without
interprocedural inference. Both ordinary call forms and all expression containers
contribute. Terminal branches, loops and both short-circuit operands remain
structurally represented. Source check/verify and both execution preparation
routes enforce subset checks after ownership approval. Private synthetic
preparation enforces the same check.

MIR retains nonempty direct call sites with canonical enclosing/callee identities,
source call ordinals, masks and origins. Resource-free empty-contract calls add no
site entries. Structural and table-aware validation plus exact deterministic
correspondence protect the metadata. No runtime effects analysis is added.

Five new identities exercise all atoms/spellings/result/call forms, three-function
propagation through unused strict supersets, structural unions, recursive bounded
contract-edge diagnostics through both preparation APIs, unsafe disclosure without
ownership permission, and nine independent direct-site mutations at approval.
These new tests are authored; execution awaits this milestone's workflows.
Cleanup contracts/realization and private abort domains are still pending.

## Milestone 3 first-push compilation correction

Commit `e228d4eb69df62dfe5654b09395c19b3002499f2`, tree
`4bc5d686b9c912544dc1a29feeee3d1eb02f093a`, failed all-target compilation in
[build](https://github.com/gregkrsak/klyxr/actions/runs/38969651618) and
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38969651702): predecessor
helpers exhaustively matched the public frontend error enum and needed its new
Effects variant. Explicit Effects classifiers were added to the eight affected
helper files (ordinary_records, copy_field_assignment, reference_field_assignment,
reference_field_reads, contextual_literals, multiple_field_records, while_loops,
local_reassignment). All original expected phases, assertions and identities stay
unchanged. No test migration, weakened expectation or platform failure is claimed.
[Docs](https://github.com/gregkrsak/klyxr/actions/runs/38969651619) and
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38969651788) passed.
No later milestone was started on this failing compilation gate.

Milestone 3 compilation correction `46a1df5337a082c68f377c2d43008477f79f42b8`
(tree `e0d3c2341a76fd4daff78d26546087efdcafac29`) passed all-target compilation.
[Build 38969904121](https://github.com/gregkrsak/klyxr/actions/runs/38969904121)
then stopped only at rustfmt layout. Its exact diff was applied to execution
preparation call layout and new test layout. [Tests 38969904096](https://github.com/gregkrsak/klyxr/actions/runs/38969904096)
passed debug test execution/reconciliation; release was still running when this
format-only correction was prepared. [Docs](https://github.com/gregkrsak/klyxr/actions/runs/38969904112)
and [verify](https://github.com/gregkrsak/klyxr/actions/runs/38969904118) passed.

## Milestone 3 completed

Commit `6d43af6e9028d189a4fcd4d0d8bf112118232a84`, tree
`36056a9dc25f70b7c8bfe01aad4a811a92b09540`, passed all four workflows:
[build](https://github.com/gregkrsak/klyxr/actions/runs/38970021292),
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38970021243),
[docs](https://github.com/gregkrsak/klyxr/actions/runs/38970021125),
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38970021253).
Job 117205821633 reconciled 1,743 predecessors plus 14 additions = 1,757 passing
identities in each profile, both affine controls and the authority doctest passed.
All job steps/conclusions and relevant logs were inspected before milestone 4.

## Milestone 4 — Registered contracts and derived summaries

Each registered operation stores its complete contract, checked for exact equality
with the bound ordinary declaration. A leaf's cached cleanup summary equals its
fixed release operation's complete contract. Acquire and release contracts remain
independent. Stub bodies are shape controls, not effect authorities.

Nonempty summaries are derived through shared canonical plans with memoization,
checked record/release identities and bounded recursion. Executable summary copies
must equal that derivation. Canonical plan validation precedes summary derivation;
it already validates fields, cycles, shared graph heights and configured depth.
An empty summary vector denotes all-empty sets only; the existing resource plans
remain present and continue controlling real cleanup. It does not classify values
as inert. All-empty predecessor and resource-free paths allocate no summary cache.

Five new identities cover all atoms, distinct-child contract unions, release strict supersets, independent
operation/function/leaf/binding mutations, aggregate unions, empty-summary plans,
and independent cached-summary mutations through actual execution approval.
These new tests await milestone GitHub execution. Ownership-produced realization
checking and private execution-domain authority remain pending.

Milestone 4 first push `3f638b0e522586eb003d41b4c4929e2bd95ca150`, tree
`42be08ec26ddfbc66424fce1ca51f6db4e33526c`, passed all-target compilation and
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38970650456): job
117207905893 reconciled 1,743 predecessors plus 19 additions = 1,762 passing
identities in each profile, with both affine/authority controls passed.
[Docs](https://github.com/gregkrsak/klyxr/actions/runs/38970650381) and
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38970650493) passed.
[Build](https://github.com/gregkrsak/klyxr/actions/runs/38970650641) stopped at one
rustfmt call-layout diff, which was applied exactly. Clippy/Rustdoc await the
corrective push; no later layer was started on this gate.

## Milestone 4 completed

Correction commit `17b382b0a76a9995857af6ba0a20a6fd654ca37f`, tree
`4aed797145f7c234c4b8c9452db1ec811180f9df`, passed all four workflows:
[build](https://github.com/gregkrsak/klyxr/actions/runs/38970841650),
[tests](https://github.com/gregkrsak/klyxr/actions/runs/38970841711),
[docs](https://github.com/gregkrsak/klyxr/actions/runs/38970841538),
[verify](https://github.com/gregkrsak/klyxr/actions/runs/38970841567).
Job 117208545567 reconciled 1,743 predecessors plus 19 additions = 1,762 passing
identities in both profiles; affine/doctest controls passed. All job steps and
relevant logs were inspected before milestone 5.

## Milestone 5 — Cleanup realization evidence and approval

The existing structured ownership checker now carries an effects-only may-custody
product. Validated initialization/consume/replacement transitions update it;
no new move or loan permission is introduced. Fallthrough alternatives union
residual custody, while effects from terminal branches remain recorded. Both-path
transfer leaves no old responsibility. Short-circuit skip custody is retained
independently of the predecessor's conservative usability state. Return results
transfer before remaining owners contribute. Loop locals contribute before
normalization removes their safety state. Displaced replacement owners contribute
before new-generation initialization; existing runtime ordering is unchanged.

The successful checker emits canonical holder/type/realization/origin evidence.
MIR receives that evidence, and opaque approval retains its immutable original.
Pre-execution validation checks shape/type/summary/order and deterministic lowering
against the retained witness without rerunning ownership or inferring runtime
effects. All compilation/preparation routes enforce the cleanup subset rule.

Nonempty cleanup effects reserve an additional conservative envelope before
checker snapshots: holder-site combinations times simultaneous snapshot depth,
plus original frontend storage; work is bounded by a checked 640*n^4 envelope.
The holder bound is the maximum within one function, not an interprocedural
ownership model. Empty-effect predecessors retain the old envelope. Diagnostic
release ancestry has a fixed 16-visit budget and stops honestly if truncated.

Seven new identities cover every ordinary exit and replacement, missing cleanup
rejection, pass-through/both-path transfer, protected return remainders,
short-circuit residual custody, independent MIR witness mutations and a canonical-looking protected-result cleanup
forgery under a legal unused superset, exact storage
neighbors, detected witness allocation failure, and expression-transaction custody
rollback. They are authored and await this milestone's authoritative CI results.
Temporary/private-domain integration and final conformance remain pending.

Milestone 5 first push `ccff5adae8a1d111e9c490afcfba06a1c19b4b20`, tree
`223de7860f68fb1878a56d439c203f40e906ba09`, compiled the implementation but failed
all-target checking in [build](https://github.com/gregkrsak/klyxr/actions/runs/38972493921)
and [tests](https://github.com/gregkrsak/klyxr/actions/runs/38972493916). One
predecessor fault-injection test called the private seal helper directly and
needed its new successful-checker-evidence argument. The fixture now obtains
ownership/effect approval before entering its unchanged allocation-failure loop.
All Host and no-artifact assertions remain unchanged; its exact identity and
reason are recorded under fixture_adaptations. [Docs](https://github.com/gregkrsak/klyxr/actions/runs/38972493924)
and [verify](https://github.com/gregkrsak/klyxr/actions/runs/38972493928) passed.
No later layer was started on this failing gate.
