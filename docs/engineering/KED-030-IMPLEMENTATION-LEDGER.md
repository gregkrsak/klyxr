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
They are authored; milestone GitHub execution remains pending until recorded.

`ked-030-predecessors.json` preserves the complete baseline identity set.
`verify-ked030-tests.py` requires every predecessor plus exactly the explicitly
listed additions in each complete profile, retaining affine/doctest controls.
Historical KED-027 through KED-029 manifests and reconcilers are unchanged.

## Remaining milestones

Ordinary grammar/signatures; direct/transitive checking; operation contracts and
derived summaries; checker-produced cleanup custody, MIR and approval; replacement
and temporaries; private abort domains; malformed/bounds/terminal evidence;
documentation and complete conformance remain unimplemented at this milestone.

No production resource API, unsafe permission, effectful proof semantics, new
ownership checker, source recoverable failure or unwind is introduced.
Implementation completion, independent acceptance and merge are not claimed.
