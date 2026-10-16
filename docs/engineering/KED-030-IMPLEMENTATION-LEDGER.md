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
