# Klyxr Open Questions

These questions are intentionally unresolved.

When one is settled, add/update a decision in `LANGUAGE_DECISIONS.md`.

## OQ-001 — Exact effect syntax

Open: attribute vs clause syntax, inferred vs declared effects, effect polymorphism, groups/aliases, generic interaction, diagnostics.

## OQ-002 — Heap-managed ownership vocabulary

Earlier exploration considered `heap T`, `shared T`, and `pinned T`. Names are not locked.

## OQ-003 — Trait/generic syntax

Static vs dynamic dispatch distinction is desired. Exact declarations, associated types, bounds, specialization, and coherence are open.

## OQ-004 — Package/module/interface syntax

Ada-style interface/implementation separation is attractive. Compilation-unit and source-layout details remain open.

## OQ-005 — Manifest format

`klyxr.toml` is the current tooling preference but is not yet a locked decision.

## OQ-006 — Panic and exception model

`Result<T,E>` is preferred for recoverable errors. Exact panic/unwind/abort/no-panic behavior remains open.

## OQ-007 — Arithmetic operator spellings

Checked/wrapping/result/saturating semantics are directionally accepted. Exact operator spellings remain reviewable.

## OQ-008 — Verification backend

Z3 is a strong initial candidate. Solver abstraction, proof certificates, reproducibility, and trusted-base policy remain open.

## OQ-009 — Explicit region/lifetime syntax

Inference should handle most code. Exact explicit syntax remains open.

## OQ-010 — Concurrency model

Ownership-safe concurrency plus Ada-inspired tasking is the direction. Exact tasking, channels, structured concurrency, scheduler metadata, and shared-state primitives remain open.

## OQ-011 — HardRealtime profile

Need a precise list of forbidden effects, allocation rules, recursion rules, blocking rules, boundedness requirements, ISR rules, and stack-analysis policy.

## OQ-012 — FFI and representation

Need exact rules for layout, ABI declarations, bitfields, volatile, memory-mapped I/O, C interoperability, and trust at foreign boundaries.

## OQ-013 — Standard library scope

Need staged core/no-runtime, alloc-capable, hosted, networking/filesystem, and realtime-safe layers.

## OQ-014 — Keyword alias expansion

KD-014 accepts six initial pairs. Do not add aliases casually. Whether more pairs are justified remains open.

## OQ-015 — Units syntax

Units are part of the direction. Exact declarations, derived dimensions, literals, conversions, simplification, and diagnostics remain open.

## OQ-016 — ABI and library evolution

Need policy for ABI stability, semantic versioning, contract changes, effect-set compatibility, and verified API evolution.

## OQ-017 — Self-hosting milestones

First implementation likely uses Rust. Criteria and trust story for self-hosting remain open.

## OQ-018 — Syntax is not frozen by repetition

Many current examples are illustrative. Always distinguish accepted semantic concepts from provisional surface spelling.

## OQ-019 — Mixed assurance call boundaries

Need exact rules for calls among `safe`, `checked`, and `verified` code: who
establishes preconditions, which postconditions may be assumed, when runtime
checks are required or may be removed, and how unproved callees enter the trust
report. KD-003, KD-006, and KD-013 settle the direction, not these details. The
prototype's `check` and `verify` commands share a restricted proof pass; they do
not implement different assurance levels.

## OQ-020 — Mutation framing, invariant boundaries, and snapshots

Need exact rules for the state a function may change, when record invariants must
hold, and what `old(...)` snapshots for nested records, aliases, heap values, and
exceptional exits. The current prototype models only one mutable field and its
entry integer value; that scope does not settle the general rules.

## OQ-021 — General constrained arithmetic expression semantics

The initial expression layer supports a deliberately narrow constrained-range arithmetic model.

Still open:

- broader numeric base types;
- default numeric literal typing;
- generalized literal coercion/context rules;
- arithmetic result typing beyond the initial range-preserving model;
- exact runtime behavior for constrained arithmetic outside verified code;
- interaction with checked/wrapping/result/saturating arithmetic families;
- conversions between constrained types;
- units/dimension-aware arithmetic.

KED-003 does not settle these questions implicitly.
