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

KED-003 through KED-009 do not settle these questions implicitly.


## OQ-022 — General function and local-value semantics

KED-004 established a straight-line value-function core with Bool/range values.
KED-005 extends it with by-value records and the basic binding/call/return move
rules accepted in KD-020. Those core move rules are no longer wholly unresolved;
this does not settle the broader function/local semantics below.
KED-006 adds references and `let mut` / `let mutable` for owned locals, authorizing
exclusive borrowing only. It introduces no assignment or mutable owned parameters.
The core straight-line, non-escaping loan rules now fall under KD-021; advanced
reference semantics remain under OQ-024. KED-007 adds explicit dereference and
Copy-safe write-through as a distinct accepted operation (KD-022). `let mut`
did not enable direct owned-local reassignment under KED-007. KED-011 now settles
only direct reassignment of mutable Bool/named-range Copy locals (KD-026).

KED-008 adds ordinary statement `if` / optional `else`, with nested lexical
branch scopes. Ordinary functions still require one final function-level return;
early/multiple return paths remain unresolved. KED-010 settles owned/Copy
conditional values only in complete local initializer position, with exact branch
types and mandatory else (KD-025). Statement-containing value blocks and general
conditional-expression positions remain unresolved.

Still open:

- whether unqualified `fn` is the final surface spelling of the base `safe` assurance level;
- explicit `safe fn` syntax, if any;
- unit/no-value function return semantics;
- early returns and multiple control-flow return paths;
- conditional values in returns, call arguments, and general expression positions;
- statement-containing value blocks and broader owned branch-result semantics;
- non-Copy replacement, reference-local reassignment, and broader mutable-local semantics;
- compound assignment, projected places, and conditional values on assignment RHSs;
- mutable owned parameters;
- reference returns and escaping-reference interfaces;
- expression-bodied function shorthand;
- contextual integer-literal typing at bindings, arguments, and returns;
- broader value categories and record/reference interfaces beyond core owned records and non-escaping references;
- function values, closures, and higher-order calls;
- function overloading;
- interaction of calls and owned places with advanced borrowing and control flow;
- runtime enforcement of constrained return values outside verified code.

OQ-019 remains authoritative for mixed `safe` / `checked` / `verified` call
boundaries. OQ-021 remains authoritative for unresolved constrained arithmetic
and literal-conversion rules. Copy customization and destruction remain under OQ-023.


KED-012 permits nested statement while for ownership-stable Copy state only.
Loop-local returns, loop values, early/multiple returns and general loop control
remain unresolved.

KED-013 now permits iteration-local Move values in while bodies (KD-028),
including fresh record results, transfers and existing branch joins. Conditions
remain Copy-only; scopes and the single final return are unchanged. General
loop-carried ownership and cyclic reference use remain unresolved. KED-014 permits
iteration-local body borrowing (KD-029), with reference-free while conditions and
no use of handles present at the current header.

## OQ-023 — Copy customization, cloning, partial moves, and destruction

KED-005 establishes the core owned `Copy` / move distinction. KED-006 classifies
shared references as Copy and mutable references as Move; neither directive
settles user customization. KED-007 blocks non-Copy write replacement specifically
because displacement/destruction semantics remain unresolved; it adds no Drop.

Still open:

- eventual user-facing `Copy` trait or equivalent;
- eligibility rules for user-defined `Copy`;
- explicit cloning/duplication APIs;
- automatic or declared Copy behavior for records, enums, tuples, and arrays;
- partial moves from aggregate fields;
- ownership of destructured values;
- destructor / `Drop` semantics;
- destruction order;
- scope-exit lowering;
- interaction between destruction and panic/unwind;
- resource-owning standard-library types;
- ownership representation across FFI boundaries.

Core borrowing is accepted in KD-021. Advanced borrowing and lifetime rules
remain separate questions under KD-004, OQ-009, and OQ-024. No runtime destruction or resource release is implemented
by the core ownership checker.

KED-013 scopes away iteration-local Move availability at the backedge without
introducing destruction or resource-release semantics. Records remain non-Copy;
Copy customization, partial moves, Drop and cleanup/unwind remain open.

## OQ-024 — Advanced borrowing, reborrowing, dereference, and escaping references

KED-006 settles only whole-value, straight-line, non-escaping core borrowing.
KED-007 settles basic explicit named dereference and Copy-safe whole-value
write-through under KD-022. Advanced borrowing and dereference coercions remain
open; general mutation/replacement is tracked separately under OQ-025.

KED-008 introduced acyclic statement branches. KED-009 settles continuation-aware,
path-sensitive whole-value loan expiry across that current conditional subset
(KD-024). It does not establish arbitrary CFG lifetime inference, general NLL
completeness, or borrowing across loops. Reborrowing, reference-valued branch
results, escaping references, and advanced lifetime relationships remain open.
KED-010 permits only owned/Copy conditional initialization; reference-valued
conditional results, reference lifetime joins, escaping branch references,
reference-valued block parameters, and reborrowing remain unresolved.

Still open:

- reference returns and escaping-reference semantics;
- explicit lifetime syntax, parameterization, and elision;
- reborrowing syntax and semantics;
- whether and when `&mut T` may coerce to `&T`;
- auto-deref and dereference coercions;
- field access through references;
- field/partial borrowing and disjoint field loans;
- reference mutation beyond Copy-safe whole-value write-through;
- two-phase borrows;
- temporary lifetime extension and borrowing arbitrary temporary expressions;
- cyclic loan/lifetime analysis across loops/backedges and general CFG lifetime inference;
- reference-valued branch results;
- borrowing interaction with closures and destructors;
- interior mutability and smart-pointer borrowing;
- FFI reference/lifetime boundaries.

OQ-009 remains authoritative for the broader explicit lifetime syntax question.

KED-012 leaves reference activity through while conditions/bodies unsupported.
Untouched outside handles/loans persist unchanged and still constrain Copy owner
access; this is not cyclic lifetime inference or reference use across backedges.

KED-014 permits iteration-local borrowing in while bodies (KD-029), refining
KED-013's historical restriction. Fresh body provenance must be gone at the
backedge before allocator restoration. Untouched header handles/loans persist
unchanged and constrain new borrows; their use inside that loop, all while-condition
reference activity, generalized cyclic NLL and fixed-point lifetime inference
remain unsupported. Nested headers make outer-created handles pre-existing.
Reborrowing, reference returns/results/reassignment, explicit lifetimes and
field/partial loans remain unresolved.

## OQ-025 — General mutation, place expressions, and replacement semantics

KED-007 settles only explicit dereference and Copy-safe whole-value write-through.
KED-008 permits those operations inside `if` branches. It does not settle general
mutation across loops or complex control flow, or non-Copy replacement/destruction.
KED-011 settles only direct mutable Bool/named-range local Copy reassignment
(KD-026), with exact typing, RHS-first/write-second ordering, and active-loan checks.
It introduces no generalized place evaluation-order rule.

Still open:

- non-Copy replacement and the fate/destruction of displaced values;
- reference-local reassignment and provenance/lifetime replacement;
- general place-expression architecture;
- field/projected assignment, partial mutation, and aggregate mutation;
- compound assignment;
- swap/take/replace primitives;
- assignment-expression semantics, if any;
- richer assignment evaluation-order guarantees;
- mutation through future reborrows;
- mutation across control flow;
- interaction with destructors and unwind.

OQ-023 remains authoritative for Copy customization and destruction. OQ-024
remains authoritative for advanced borrowing, reborrowing, dereference coercions,
escaping references, and field/partial borrowing.

KED-012 permits KD-026 owned Copy reassignment in nested while/if bodies only
when ownership and loan state is stable. General mutation across cyclic ownership,
non-Copy replacement and destruction remain unresolved.

KED-013 permits transfers of iteration-local Move values, not replacement.
Direct reassignment of any non-Copy owner, including body-local owners, remains
unsupported pending displacement/destruction semantics. KED-014 permits existing
Copy-safe write-through using fresh body references, not replacement or destruction.
Pre-existing handles remain unusable inside that loop; generalized cyclic mutation
and lifetime analysis remain open.

## OQ-026 — General control flow, conditional values, and MIR dataflow

KED-008 establishes only acyclic statement conditionals and the first ordinary MIR CFG.

KED-009 improves loan precision across acyclic statement branches. Ownership
remains in the semantic typed-HIR → MIR stage, using one existing ownership
checker. Full MIR-based ownership dataflow and loop/fixed-point analysis remain
unresolved, as do general conditional expressions, phi/block parameters, early
returns, definite initialization, and unreachable-path analysis.

KED-010 settles owned/Copy value production across acyclic joins only in local
initializer position. Its first MIR strategy initializes the existing source
LocalId independently on every mutually exclusive leaf path. This is not the
final general MIR solution for all future value joins; no phi/block parameters,
SSA, general temporary values, or definite-assignment solver are introduced.

Still open:

- general conditional expressions and branch-result type unification beyond exact types;
- join values without a source local destination, arbitrary temporaries, and SSA;
- block parameters / phi-like representations;
- generalized cyclic ownership/loans and fixed-point analysis, `for`, `break` / `continue`, loop values and labels;
- early returns, multiple return paths, divergence / bottom types, and `match` lowering;
- definite initialization and uninitialized locals;
- full MIR-based ownership dataflow and maximally precise path-sensitive loan analysis;
- short-circuit Boolean lowering / evaluation guarantees and unreachable-code policy;
- destruction / cleanup edges and exceptional / unwind control flow;
- eventual MIR expression-lowering granularity.

KED-011 adds a distinct acyclic MIR Assign statement for already initialized Copy
locals. This prepares ordinary loop-carried Copy state for a separately authorized
future directive; it adds no loops, backedges, fixed points, SSA, or definite assignment.

KED-012 settles pre-test statement while and cyclic MIR for ownership-stable
owned Copy state (KD-027), including nested loops and if/while composition.
MIR is no longer globally acyclic; structural validity is not a termination proof.
The checker remains above MIR and compares header/backedge state for one permitted
iteration, without generalized cyclic dataflow. KED-013 now permits iteration-local
Move activity, but loop-carried non-Copy changes,
cyclic reference liveness, verified loops/invariants/variants and termination
checking remain open, alongside the questions above.

KED-008/KED-010/KED-011/KED-012 MIR choices do not settle or preclude these broader features.


KED-013 settles only iteration-local Move activity with per-loop header identity
(KD-028). Outer-iteration locals are loop-carried relative to an inner header;
pre-existing owners cannot change availability across any backedge. The existing
HIR-stage checker scopes away body-local state and checks semantic equality for
one iteration. It does not add general cyclic ownership convergence, fixed-point
dataflow, reference inference, MIR ownership, or a new IR representation.


KED-014 adds only iteration-local borrowing to that one-iteration HIR model
(KD-029). Finite continuation-aware liveness applies inside each permitted body;
new handles/loans must be discharged before the backedge and allocator restoration.
It does not settle stable loop-carried reference use, generalized cyclic NLL,
fixed-point ownership/loan dataflow, MIR ownership, reference-result joins,
termination, verified loops or any broader control-flow question above.
