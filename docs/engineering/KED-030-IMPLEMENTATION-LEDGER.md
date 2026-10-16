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
