# Klyxr Language Decisions

Statuses:

- **Accepted** — settled unless explicitly reopened.
- **Direction Accepted** — architectural direction is settled; details may evolve.
- **Provisional** — useful current choice, not fully locked.
- **Superseded** — historical only.

---

## KD-001 — Public project name

**Status:** Accepted

**Decision:** The public language name is **Klyxr**, pronounced **“CLICK-ser.”**

**CLI:** `klyxr`  
**Primary source extension:** `.klx`  
**Primary domain:** `klyxr.com`  
**Secondary owned domain:** `klyxr.online`

---

## KD-002 — Public positioning

**Status:** Accepted

> **Systems programming you can reason about.**

Canonical summary:

> **Klyxr is a systems programming language that treats memory safety as the foundation, then lets programmers express and verify more of the rules that make their software correct.**

Core philosophy:

> **Own what moves. Constrain what exists. Prove what matters. Make every escape hatch visible.**

---

## KD-003 — Assurance levels

**Status:** Accepted

Klyxr has three escalating assurance levels:

- `safe`
- `checked`
- `verified`

They belong to one language, not separate dialects.

---

## KD-004 — Ownership and borrowing

**Status:** Direction Accepted

Klyxr uses:

- moves;
- immutable borrowing;
- mutable borrowing;
- deterministic destruction;
- no required garbage collector.

Move semantics are the default for non-`Copy` values.

---

## KD-005 — Domain and range types

**Status:** Accepted

Klyxr supports real distinct constrained types.

```klyxr
type Percent = range 0..100;
```

Units and dimension-aware types are part of the language direction.

---

## KD-006 — Contracts and invariants

**Status:** Accepted

Klyxr supports first-class:

- `requires`
- `ensures`
- type/record invariants
- `old(...)`
- quantification where appropriate

In checked mode, contracts may become runtime checks.  
In verified mode, contracts create proof obligations.

---

## KD-007 — Sum types and recoverable errors

**Status:** Direction Accepted

Klyxr uses:

- algebraic enum/sum types;
- exhaustive `match`;
- `Option<T>` instead of universal null;
- `Result<T, E>`;
- `?`-style propagation or equivalent ergonomic syntax.

---

## KD-008 — Explicit effects

**Status:** Direction Accepted

Klyxr has compiler-visible effects.

Candidate core effects include:

- `io`
- `heap`
- `blocking`
- `network`
- `filesystem`
- `clock`
- `random`
- `unsafe`

Effects propagate transitively through the call graph.

---

## KD-009 — Trust domains

**Status:** Accepted

Klyxr distinguishes:

- `safe`
- `trusted`
- `unsafe`

`trusted` means an assumption is intentionally accepted and auditable.  
`unsafe` means normal language safety rules may be bypassed.

---

## KD-010 — Auditability

**Status:** Direction Accepted

The toolchain should provide an audit workflow such as:

```bash
klyxr audit
```

It should ultimately expose trusted assumptions, unsafe regions, transitive trust dependencies, effect-policy violations, and assurance-relevant project information.

---

## KD-011 — Unified toolchain

**Status:** Accepted

Canonical command family:

```bash
klyxr new
klyxr build
klyxr run
klyxr test
klyxr verify
klyxr bench
klyxr doc
klyxr audit
klyxr format
klyxr lint
```

Exact subcommand details may evolve.

---

## KD-012 — Compiler semantic pipeline

**Status:** Direction Accepted

```text
source
  ↓
lexer / parser
  ↓
AST
  ↓
name resolution
  ↓
typed HIR
  ↓
types / ownership / effects
  ↓
MIR
  ├── verification path → VIR → solver
  └── codegen path      → backend → machine code
```

Verification occurs before backend transformations can redefine language semantics.

---

## KD-013 — Verification is modular

**Status:** Direction Accepted

Callers rely on callee contracts rather than reproving entire implementations on every call.

Verified loops use appropriate invariants and, where needed, termination variants.

---

## KD-014 — Long-form keyword aliases

**Status:** Accepted

Selected high-frequency keywords may have approved compact and explicit spellings.

The spellings are lexical synonyms and normalize to the same semantic token.

| Compact | Explicit |
|---|---|
| `mut` | `mutable` |
| `fn` | `function` |
| `pub` | `public` |
| `mod` | `module` |
| `impl` | `implementation` |
| `const` | `constant` |

Examples:

```klyxr
pub verified fn transfer(source: &mut Account)
```

```klyxr
public verified function transfer(source: &mutable Account)
```

They must produce identical semantics.

**Default ecosystem style:** compact.

An organization may select explicit style through formatter/linter policy, e.g.:

```toml
[style]
keyword_spelling = "explicit"
```

The compiler understands both forms. Style enforcement belongs in formatting/linting unless a project promotes the lint to an error.

**Hard rule:** aliases may never acquire semantic differences.

---

## KD-015 — Deterministic arithmetic semantics

**Status:** Direction Accepted

Optimization level must not silently change arithmetic semantics.

Current operator direction:

- ordinary `+` — checked
- `+%` — wrapping
- `+?` — result-on-overflow
- `+^` — saturating

Exact spellings remain reviewable.

---

## KD-016 — Realtime profiles

**Status:** Direction Accepted

A hard-realtime profile may reject:

- heap allocation;
- unbounded blocking;
- forbidden recursion;
- other structurally unbounded or disallowed behavior.

Initially claim only structural properties the compiler can defensibly establish.

---

## KD-017 — Public guarantee discipline

**Status:** Accepted

Klyxr never equates verification with unspecified global correctness.

Canonical wording:

> **The stated properties were proven under the stated assumptions.**

---

## KD-018 — Typed expression semantics

**Status:** Direction Accepted

Klyxr expressions are resolved and type-checked before verification or backend lowering.

Typed HIR records the semantic type of expressions.

`Bool` is a built-in semantic expression type.

Named constrained range types remain nominally distinct during expression typing;
operators do not implicitly mix different constrained type identities merely
because their representations or bounds are compatible.

Integer literals are constants and do not themselves create implicit conversions
between distinct named constrained types.

The initial executable expression subset and surface spellings remain subject to
the explicit open questions and prototype-boundary rules. Implementing an operator
in the prototype does not settle the complete Klyxr operator set.

---

## KD-019 — Explicit value returns in block-bodied functions

**Status:** Accepted

Klyxr uses explicit value returns in block-bodied functions.

A value-returning block function declares its return type with `-> Type` and
returns a value with:

```klyxr
return expression;
```

Klyxr does not use Rust-style implicit block-tail returns. The absence of a
semicolon on the final expression does not cause that expression to become the
function's return value.

Local bindings use `let` and are immutable by default; future local mutation
must be explicit. Function calls are expressions.

A future explicitly marked expression-bodied function shorthand is not ruled
out, but it would be a separate syntactic construct and would not change the
explicit-return rule for block-bodied functions.

---

## KD-020 — Core owned-value move semantics

**Status:** Accepted

Klyxr uses move-by-default ownership for non-`Copy` values.

In the initial core ownership model:

- `bool` values are `Copy`;
- named constrained range values are `Copy`;
- record values are non-`Copy` and move by default.

A non-`Copy` owned value moves when transferred through a by-value binding,
by-value function argument, or return. After a move, the previous owned place
may not be used as a value. The compiler never silently clones a moved value.

This initial `Copy` classification is semantic compiler behavior; KED-005 does
not establish the eventual user-facing `Copy` trait or customization mechanism.
A record is non-`Copy` even when all its fields are `Copy`.

Borrowing is a separate language mechanism and is not introduced by KED-005.

---

## KD-021 — Core borrowing semantics

**Status:** Accepted

Klyxr uses explicit shared (`&T`) and exclusive (`&mut T`) borrowing.
A shared loan permits multiple overlapping shared loans but conflicts with an
exclusive loan. An exclusive loan conflicts with all other active loans and with
direct access to the borrowed owner. Moves of a non-`Copy` owner are forbidden
while any loan of that owner remains active.

Stored loans remain active through the last possible use of the reference values
derived from them, rather than mechanically through the enclosing lexical block.
Shared reference values are `Copy`; mutable reference values are non-`Copy` and
move by value. Borrowing, cloning, mutable reborrowing, and reference coercions
are never inserted implicitly.

KED-006 implements only whole-value, straight-line, non-escaping borrowing.
`let mut` marked an owned local as exclusively borrowable; KED-006 itself added
no reassignment. KD-026 later permits narrow Copy-safe direct local reassignment.
KED-006 does not establish explicit lifetime syntax, reference returns, dereference
semantics, partial borrowing, or mutation through references.

---

## KD-022 — Core dereference and Copy-safe mutation

**Status:** Accepted

Klyxr uses explicit `*reference` dereference syntax. Dereferencing accesses the
referent but does not itself transfer or consume the reference handle.
Borrowed Copy values may be read through either `&T` or `&mut T`; the read copies
the referent. Borrowed non-Copy values may not be moved out through dereference.

Whole-value write-through is permitted through `&mut T` only when T is Copy.
Write-through through `&T` is forbidden. Non-Copy replacement remains forbidden
until ownership and destruction semantics for displaced values are defined.

KED-007 implements directly named ordinary reference access and dedicated
`*reference = expression;` statements only. It does not establish owned-local
reassignment, non-Copy replacement, destructors/Drop, field access or mutation,
auto-deref, reborrowing, compound assignment, assignment expressions, or control flow.
KD-026 later establishes a separate narrow Copy-safe owned-local reassignment form.

---

## KD-023 — Statement conditionals and MIR control-flow foundation

**Status:** Accepted

Klyxr initially introduces `if` / optional `else` as control-flow statements.
An `if` requires a Bool condition, creates lexical child scopes for its branches,
and rejoins after the selected branch completes. Nested statements are allowed;
conditionals do not produce values. Visible names cannot be shadowed; sibling
branches may reuse a spelling with distinct canonical local identities.

A non-Copy place visible after a join is available only if it remains available
on every reachable incoming branch. A move on any such branch prevents later
use or borrowing of that place. Mutable reference handles follow the Move rule;
shared reference handles remain Copy.

KED-008 establishes acyclic ordinary MIR basic blocks and explicit Branch, Goto,
and Return terminators. Ordinary functions still require one final function-level
return. It establishes no loops, early/multiple returns, SSA, phi nodes, block
parameters, or generalized control-flow dataflow. Value-producing conditionals
remain unresolved, and the MIR architecture leaves their future extension open.

---

## KD-024 — Path-sensitive acyclic loan liveness

**Status:** Accepted

For the current acyclic statement conditional subset, whole-value loan liveness
is control-flow-path-sensitive. A loan remains active on a path while an available
reference handle carrying its provenance may still be used on that path, or an
explicit operation hold requires it. Lexical coexistence of reference bindings
does not imply overlapping loans. Sibling-only uses do not keep a loan active
on the opposite branch.

A loan needed by any possible successor stays active through condition evaluation
and dispatch. After the split it may expire independently on an edge with no
possible future use of a surviving handle. Post-join future uses propagate into
every incoming path where that handle may remain available. Loans cannot expire
and later reactivate along the same path; both branch edges remain possible,
including constant Boolean conditions.

At joins, Move availability retains KD-023's all-path rule. Potentially active
incoming loans merge conservatively: an operation must be safe on every incoming
path. Conditional unavailability of a handle does not itself erase a loan still
needed on a path where the handle was not moved. Loans inactive on all incoming
paths are dead after the join. Shared copies retain provenance; mutable transfers
remain moves. Call/write holds remain authoritative and branch-local handles
and loans do not escape.

KED-009 refines the existing ownership phase above MIR. It adds no syntax, loops,
reborrowing, reference returns, explicit lifetimes, conditional values, destruction,
or generalized CFG lifetime inference. General MIR dataflow and cyclic loan analysis remain
unresolved under OQ-024/OQ-026; KD-027 later settles narrow statement loops.


---

## KD-025 — Value-producing conditional initialization

**Status:** Accepted

An ordinary local initializer may select an owned value with:

```klyxr
let chosen = if flag { first } else { second };
```

`let mut` / `let mutable` may use the same one-time initialization. Braces and
`else` are mandatory. Each branch contains exactly one value expression, without
a trailing semicolon or statements. A nested conditional may be the complete
branch result. Statement `if` remains separate and retains optional `else` and
lexical statement branches.

The condition must be Bool. Both branches must have exactly the same permitted
concrete owned type: Bool, a canonical named range, or a canonical record. Distinct
nominal declarations are incompatible even with identical bounds or structure.
There is no contextual integer-literal materialization, implicit conversion, or
reference-valued conditional result. Copy dereference results follow existing
rules; non-Copy move-out through references remains forbidden.

Condition ownership effects occur once before independent branch snapshots. A
Copy result preserves its source; a Move result transfers into the destination
on that path. Every reachable leaf initializes the same source local. The
destination becomes available after the join; source availability retains
KD-023's all-path rule. The same source may move into the destination on both
mutually exclusive paths, but cannot then be used again. KD-024 continuation-aware
loan liveness and call/write holds apply without relaxation.

Typed HIR retains the conditional and its exact type. The first MIR strategy
emits explicit Branch/Goto edges, initializes the canonical destination LocalId
on each mutually exclusive leaf, and rejoins without a value merge instruction.
This is one-time initialization, not reassignment or SSA. No phi, block parameters,
general temporaries, result slots, or definite-assignment solver are introduced.

KED-010 permits this form only as a complete local initializer or recursively as
a complete value branch. Returns, call arguments, unary/binary operands,
parenthesized conditional values, dereference writes, and general expression
positions remain unsupported. It settles no statement-containing value blocks,
reference lifetime joins, early returns, loops, destruction, execution, or general
MIR value-flow strategy. These boundaries remain open under OQ-022/OQ-024/OQ-026.


---

## KD-026 — Copy-safe direct local reassignment

**Status:** Accepted

An ordinary local declared with `let mut` / `let mutable` may be directly
reassigned when it holds an owned non-reference Copy value. Initially the
permitted target types are Bool and named constrained ranges:

```klyxr
let mut current = start;
current = next(current);
```

The target is one directly named, in-scope mutable local. Parameters and immutable
locals are not assignable. The RHS must have the exact same concrete type,
including nominal range identity. There is no contextual integer-literal
materialization, conversion, structural matching, or implicit clone.

Assignment is a statement with a required semicolon and produces no value.
It remains distinct from Let initialization and explicit DerefAssign write-through.
Chained/compound assignment, projected/field/index targets, assignment expressions,
and conditional values on an assignment RHS remain unsupported.

For this directly named form, RHS evaluation occurs before mutation. Existing
RHS ownership effects and call holds complete, last-use loans may expire, and
then the write requires exclusive mutation access: any active shared or exclusive
loan of the target conflicts. This allows Copy self-assignment and final reference
reads/calls on the RHS before the write when no future handle use keeps the loan
active. KD-024 continuation-aware path-sensitive expiry remains authoritative in
branches; a post-join reference use protects the owner on every relevant path.
Assignment creates no loan or new local identity, moves nothing out of the target,
and leaves the Copy owner available. This rule does not settle evaluation order
for future generalized place assignments.

Reference-local reassignment is rejected during typing because provenance/lifetime
replacement remains unresolved. Mutable record locals remain non-Copy: exact-typed
replacement is rejected during ownership checking before consuming either owner,
until displacement/destruction semantics are defined. No destruction, leak, clone,
swap, take, or replace behavior is implied.

AST, resolved statements, typed HIR, and MIR use a dedicated Assign node. HIR/MIR
retain the canonical destination LocalId, typed RHS, and statement span. Straight-line
lowering adds no blocks; branch lowering uses the current branch block. MIR remains
deterministic with explicit Branch/Goto/Return terminators (acyclic in KED-011;
KD-027 later adds loop backedges). KED-010
conditional initialization still emits mutually exclusive Let statements, not Assign.

KED-011 adds no loops/backedges, fixed-point analysis, general ownership dataflow,
SSA, phi, block parameters, general temporaries/result slots, definite assignment,
non-Copy replacement, reference replacement, execution, or verification expansion.
OQ-022/OQ-024/OQ-025/OQ-026 retain broader mutation and control-flow design.


## KD-027 — Ordinary while loops with loop-stable ownership

**Status:** Accepted

Ordinary `while condition { statements }` is a pre-test, zero-or-more-iteration
statement. The condition must be exactly Bool. Braces are required; the block has
no trailing semicolon or result value. Nested loops and statement conditionals
are permitted. Each body is a lexical child scope: declarations are ordered,
visible names cannot be shadowed, and canonical LocalIds are static source
identities, not new identities for each iteration. One final function-level return
remains required; loop-local returns are unsupported.

KED-012 permits ownership-stable owned Copy Bool/named-range activity in the
condition and body, including KD-026 direct reassignment, Copy calls/locals,
and KD-025 Copy conditional initialization. Header and backedge ownership/loan
states must agree after body-local state is scoped away. Untouched outside Move
owners and references may persist unchanged. Outside live loans still constrain
owner access and assignment; a dead pre-loop loan is not resurrected.

All reference activity in the condition/body (creation, handle use/transfer,
arguments, dereference and write-through) and, under KED-012, all non-Copy activity (including
fresh record call results and nested consumption) are rejected pending cyclic
ownership/lifetime analysis. KD-028 later permits iteration-local Move activity
in the body while preserving the condition and reference boundaries. A reference
being Copy does not bypass this boundary.
Finite future-use counts remain outside-continuation bookkeeping, not cyclic NLL.
Ownership remains above MIR with one permitted-iteration stability check and no
fixed-point solver or second checker.

MIR lowers while to existing Goto/Branch primitives: preheader to header, header
to body or exit, and body back to header. Mutable locals retain source LocalIds;
Let remains initialization and Assign remains mutation. MIR is no longer globally
acyclic. Structural validation accepts cycles, checks targets/reachability/types,
and makes no termination claim. Constant conditions do not remove either edge.

No break/continue/for/unconditional loop, labels, loop values, early returns,
phi/SSA/block parameters, generalized temporaries, destruction, reference returns,
reborrowing, execution, code generation, verified loops, invariants/variants, or
termination checking is introduced. OQ-024/OQ-026 retain cyclic lifetime/ownership
and broader control-flow questions.


---

## KD-028 — Iteration-Local Move Ownership

**Status:** Accepted

Non-Copy ownership activity may occur inside an ordinary while body when it
originates and stays entirely within the current iteration and leaves all
pre-existing ownership state unchanged at the backedge. Fresh non-Copy call
results may be bound, moved through iteration-local bindings and by-value calls,
or consumed by calls returning Copy values. Existing KD-020 move rules and
KD-023 acyclic branch joins apply normally, including use-after-move rejection.
Existing conditional initialization may select iteration-local owned values.

A body-local declaration has one static canonical LocalId and represents fresh
initialization on each dynamic iteration. Its tracked availability is scoped
away before the KD-027 header/backedge semantic comparison. This compile-time
scope cleanup introduces no runtime destruction, Drop, cleanup or resource-release
semantics. Unused owned locals retain the existing prototype scope-exit behavior.

Parameters and locals present at entry to the current loop are pre-existing state.
They cannot move through bindings, calls, or nested branches inside that loop.
Each nested loop establishes its own boundary: an outer iteration's local is
pre-existing relative to an inner loop and cannot be consumed there. It may remain
untouched by the inner loop and be consumed later in its containing iteration.
Untouched outside owners and live outside loans retain their existing protection.

The condition remains limited to KD-027 ownership-stable Copy activity. Reference
creation, use/alias/transfer/call arguments, dereference and write-through remain
forbidden in conditions and bodies. Non-Copy replacement remains forbidden even
for iteration-local owners. Exact nominal typing, source ordering, no shadowing,
non-escaping scopes and conservative constant-condition edges are unchanged.

KED-013 refines the single typed-HIR ownership checker with per-loop header place
identity, analyzes one permitted iteration, scopes away iteration-local state,
and requires semantic equality of remaining header/backedge state. No fixed-point
solver, general loop-carried Move joins, second checker, MIR ownership annotations,
SSA/phi/block parameters, syntax, conversions, execution, or termination proof is
introduced. MIR retains KD-027's existing lowering and cyclic validation.
OQ-022/OQ-023/OQ-025/OQ-026 retain broader ownership/control-flow questions;
KD-029 subsequently permits iteration-local body borrowing; cyclic lifetimes and
KD-030 subsequently permits stable carried access; generalized cyclic lifetime
inference remains open under OQ-024.


---

## KD-029 — Iteration-Local Borrowing in While Loops

**Status:** Accepted

Ordinary while bodies may create and use whole-value references whose provenance
is created after the current loop header and fully discharged before its backedge.
Both pre-existing owners and iteration-local Move owners may be borrowed, subject
to existing exact nominal types, mutability, capability and conflict rules. Shared
aliases, mutable-handle moves, reference call arguments, named dereference and
Copy-safe write-through retain KD-021/KD-022 rules, including complete call/write
holds. KD-024 path-sensitive last-use rules apply within acyclic body branches.
An iteration-local record may move after its final loan use; borrowing does not
permit moving a pre-existing non-Copy owner or materializing/replacing a non-Copy
referent.

Reference handles present at the current header may persist untouched but cannot
be read, aliased, transferred, passed to calls, dereferenced or written through
inside that loop. Their live loans still constrain owner access and new borrows;
dead pre-loop loans never resurrect. While conditions remain reference-free and
retain KD-027/KD-028 Copy-only restrictions. Constant conditions do not prune checks.
Nested loops establish relative boundaries: an outer-created reference is
pre-existing at an inner header and must remain untouched there, though it may
be used after the inner loop in its containing iteration. Compatible fresh inner
borrows remain permitted.

The existing typed-HIR checker analyzes one cloned iteration. Lexical cleanup
removes body-local handles/availability and expires their loans. Before restoring
the header's private loan allocator, it proves that no newly allocated loan or
surviving handle provenance remains. Remaining ownership availability, handle
provenance and active loans must equal the header semantically. Diagnostic names,
spans and finite continuation counts are excluded from that equality. Allocator
reuse cannot conceal escaping provenance or collide with a surviving loan.
Cleanup is compile-time bookkeeping, not runtime destruction.

This refines KD-027/KD-028's historical body-reference restriction only. No syntax,
public HIR annotations, MIR construct, second checker, cyclic lifetime/fixed-point
solver or execution is introduced. MIR retains existing deterministic Branch/Goto
backedges. KD-030 subsequently settles stable carried access with recurrent
obligations. Reference returns, reference-valued
conditional results, reference-local reassignment, reborrowing, explicit lifetimes,
partial borrowing and generalized cyclic NLL remain unresolved under OQ-024/OQ-026.


---

## KD-030 — Stable Loop-Carried Reference Access

**Status:** Accepted

A reference present at a structured loop header may be used repeatedly within
that loop when its availability, provenance, and required loan state are preserved
across every accepted backedge. If the carried reference is used on any checked
body path, its required loan remains continuously active across every accepted
backedge regardless of constant-condition or one-shot reasoning. Loop-backedge
liveness is distinct from false-exit continuation liveness. Newly created
iteration-local provenance must still be discharged before the current loop's
backedge. This does not establish generalized cyclic lifetime inference.

Shared carried handles remain Copy and may be read, passed to existing calls or
copied into iteration-local aliases. Mutable carried handles may be dereferenced
and used for existing Copy-safe writes, but remain Move: binding transfers and
by-value call arguments reject rather than becoming implicit reborrows. Referent
Copy mutation does not change handle provenance. Existing capability/conflict,
nominal typing, operation-hold and non-Copy access/replacement rules remain intact.

Recurrent loan obligations are separate from finite continuation FutureUses and
operation holds. Discovery includes both checked branches and nested bodies.
Scoped nested obligations retain the original LoanId, owner and kind continuously;
inner false-exit removes only its own frame. Backedge cleanup and exact semantic
validation precede obligation discharge. No expiry/refresh/reconstruction, fake
infinite counts or path-specific release can implement recurrence. On false exit,
ordinary suffix liveness and enclosing obligations determine whether a loan ends.
An unused lexical handle does not itself keep a loan alive.

An outer-iteration-local reference may be carried at an inner header but its
new provenance must still disappear before the outer backedge under KD-029.
Body-local aliases of existing provenance disappear without killing an obligated
carried loan. New provenance is checked before allocator normalization. Failed
reference operations preserve availability, liveness, holds, loan and provenance
state. Names/spans remain metadata; identity and scope are canonical.

KED-015 refines the existing one-iteration typed-HIR ownership checker. While
conditions remain reference-free and carried owned Move changes remain forbidden.
No syntax, MIR ownership engine, cyclic fixed-point solver, general lifetime
inference, reference reassignment/returns/results, escaping references, reborrowing,
coercion, partial borrowing, destruction, execution or termination proof is added.
OQ-024/OQ-026 retain those broader lifetime/control-flow questions.


---

## KD-031 — Structured continue in while loops

**Status:** Accepted

An unlabeled `continue;` statement transfers control to the innermost enclosing
ordinary while header. It is an independently checked explicit backedge: carried
availability, handle mappings, exact continuously active LoanId/owner/kind,
balanced operation holds, and scoped recurrent obligations must equal the header
state after iteration-local cleanup. Fresh provenance must be absent before safe
allocator normalization. A continue never discharges current or enclosing
recurrent obligations. Existing carried Move restrictions remain unchanged.

Continue has no same-iteration finite continuation, but retains the finite
post-loop continuation because the condition-false exit remains a real successor.
Path-specific summaries must establish this distinction before operations preceding
a continue. A body-local loan needed only by a skipped suffix may expire on that
path; a recurrent carried loan or post-loop use still protects the owner. Finite
future uses, recurrent obligations, and operation holds remain separate reasons
for loan activity. No loan may expire and later reactivate on one path.

Direct continue terminates its current lexical block. A statement conditional
whose checked arms both terminate has no fallthrough; a following statement in
that block is rejected structurally, including nested conditionals. Only actual
fallthrough arms participate in a suffix join; a single survivor retains its
ownership effects. No constant-path pruning or generalized unreachable-code
policy is established. A while retains its checked false exit even if every body
path continues, independently of any body fallthrough. False exit discharges only
that loop's obligation frame and applies outside suffix liveness; enclosing
obligations remain authoritative.

AST/resolution/typed HIR contain an explicit statement form. The single ownership
checker uses bounded recursive structured flow and transactional backedge cleanup;
a failure restores availability, provenance, finite uses, obligations, holds,
allocator, lexical scope, and loop target/header context. MIR reuses Goto to the
innermost header, emitting joins/suffixes only for real fallthrough paths. This
settles no labels, break, loop values, early returns, generalized CFG/fixed-point
ownership, MIR borrowing, reborrowing, destruction, execution or verification.


---

## KD-032 — Structured `break` in `while` Loops

**Status:** Accepted

An unlabeled `break;` transfers control to the continuation of the innermost
enclosing ordinary `while`. Every break edge is an explicit loop-exit edge and
must normalize to the same canonical ownership/reference exit state as the loop's
ordinary condition-false exit. Before any permitted normalization reduction that
could erase evidence, the checker must validate the corresponding ownership,
provenance, recurrent-frame, nesting, and operation-hold integrity against the
saved target-loop context. Target-loop iteration-local state is then cleaned,
exactly the target loop's recurrent-obligation frame is discharged, enclosing
recurrent frames are preserved, and the target loop's outside finite continuation
is applied before expiry and canonical-exit comparison. Recurrent protection
remains active until the break statement is reached; KED-017 does not retroactively
weaken earlier operations on a path that later breaks. An unconditional break
terminates its current lexical statement block. KED-017 introduces no labels,
break values, loop values, generalized unreachable-code analysis, generalized
cyclic ownership/lifetime fixed point, arbitrary exit-state merging, path-sensitive
relaxation of pre-existing non-Copy loop ownership, reborrowing, or new MIR terminator.

The canonical false exit is derived independently from the checked header
condition, outside finite continuation, and enclosing frames before inspecting
break candidates. Normalization is a restricted, non-repairing projection of the
actual candidate: carried availability/provenance and enclosing obligations cannot
be reset to manufacture equality. Integrity checks precede local cleanup, exact
frame discharge, expiry, and safe allocator normalization whenever those reductions
could erase corruption evidence. Holds must balance; newly allocated provenance
must be absent before allocator reuse. Failed validation restores the complete
entering candidate, including an injected defect, rather than repairing it.

Finite same-iteration suffix uses skipped by break are removed before preceding
operations on that path; outside finite uses and separate recurrent LoanId
obligations remain authoritative. Recurrence is not discharged early. Nested
break removes exactly one frame even when that frame is empty. Original carried
LoanId/owner/kind remain continuously active whenever an enclosing obligation or
post-loop use requires them; recurrence-only loans may end at exit without
reactivation. Lexical cleanup adds no runtime destruction.

Only actual fallthrough arms participate in a suffix join. Both-break and mixed
break/continue conditionals have zero lexical fallthrough. A while retains its
checked condition-false exit even if all body paths terminate or its condition is
a Boolean constant. Bottom backedges and explicit continue backedges keep the
existing exact header-state rule; break exits instead compare to canonical false
exit. Pre-existing non-Copy Move restrictions remain unchanged on terminal paths.
AST, resolution, and typed HIR preserve Break/span and canonical source identities.
MIR uses existing Goto to the innermost loop exit, with no synthetic join for
all-terminal arms. Ownership remains above MIR in the single structured checker.
OQ-022/OQ-024/OQ-025/OQ-026 retain broader control-flow and lifetime design.

## KD-033 — Structured Early Return in Ordinary Functions

**Status:** Accepted (frozen KED-018, Issue #37).

> An explicit `return expression;` may terminate any statement path in an ordinary
> function. After lexically targeted loop transfers are consumed, an ordinary function
> must have no structural fallthrough to its closing brace; every finite
> function-completing path ends in an explicit correctly typed return, without asserting
> that execution terminates. Structured `while` always retains its condition-false path
> for this analysis regardless of constant conditions. A return terminates its lexical
> block and contributes no ownership state to later branch, loop, or suffix joins. The
> finite continuation immediately before `return E;` consists only of the finite uses
> required to evaluate `E`; every bypassed lexical, loop-outside, enclosing-loop, and
> function continuation is excluded on that path. All active current and enclosing
> recurrent obligations remain authoritative throughout evaluation of the actual return
> operand, including nested calls and arguments. Terminal-return permission suppresses
> only the existing restriction on transferring pre-existing owned non-reference Move
> places within the actual return-expression tree; it does not retroactively authorize
> earlier loop-carried Move changes and does not bypass availability, borrowing,
> reference-transfer, provenance, or operation-hold checks. Exact return typing remains
> in the existing typing phase before ownership. On the ownership side, the return-entry
> snapshot is taken after operand-only finite-summary installation and ordinary expiry
> but before terminal-return permission is enabled; all fallible return-specific
> ownership validation remains inside that transaction, and terminal permission is
> restored on both success and failure. Only after successful operand and
> return-specific ownership validation does the function path terminate, with no
> canonical local successor normalization or ownership-state merge. KED-018 reuses the
> existing MIR `Return` terminator and introduces no implicit returns, divergence/bottom
> semantics, generalized unreachable analysis, arbitrary function-exit ownership
> joining, generalized cyclic ownership/lifetime fixed point, reborrowing, destruction,
> SSA/phi machinery, second type checker, or new MIR terminator.

KD-033 refines the earlier single-final-return boundaries of KD-019, KD-023,
and KD-027; their other typing, scope, loan, and loop rules remain unchanged.


## KD-034 — No-Value Ordinary Functions and Call Statements

**Status:** Accepted (frozen KED-019, Issue #39, including K19-01 and accepted implementation-pressure constraints).

> An ordinary function with no declared `-> Type` is explicitly NoValue: it
> produces no expression value and may complete either by reaching its function
> closing brace or by executing `return;`. A call to a NoValue ordinary function
> may appear only as a standalone call statement; value-producing calls may not
> be silently discarded, and NoValue calls may not enter expression typing.
> No-value call statements preserve the complete existing ordinary-call ownership
> transaction, while bare return is a terminal function edge with an empty finite
> continuation, no operand, no terminal-expression Move permission, no local
> successor, and no destruction/cleanup semantics.

Function results carry explicit NoValue or Value(T) metadata across AST,
resolution, signature lookup, typing and HIR, separate from expression types.
Omitted arrows do not infer result types. Exact argument and return typing remains
in typing; manually constructed ASTs must receive the same diagnostics. Forward
and recursive statement calls are structurally fallthrough-capable, without
termination or divergence inference. Nested closing braces remain local fallthrough.

K19-01 requires a whole prepared-entry statement-call transaction covering all
arguments, receiving holds (including nested-call preservation), release, expiry,
and fallible final validation. Failure exactly restores availability, mappings,
loans/holds/provenance, finite uses, allocator, recurrence, loop and terminal
context. Arguments participate in both finite-use counting and recurrent-reference
discovery. Neither preceding/final calls nor bare return receive KD-033 terminal
Move permission. `finite_before(return;) = ∅`; ordinary expiry precedes the bare
return snapshot, and completion validation/rollback stays within that boundary.
Current/enclosing recurrence remains authoritative. NoValue top-level fallthrough
requires balanced holds, without repair, frame normalization, synthetic exit
joining, cleanup or destruction. Existing loop false exits and local transfers,
real suffix joins, and KD-033 value-return behavior remain unchanged.

MIR uses distinct no-value call statements and honest no-value completion,
without dummy expressions, hidden result locals, or expression-type additions.
Lowering is function-kind-sensitive; already-terminal flow emits no unreachable
completion, and call argument trees receive exhaustive existing validator checks.
Ownership remains in the single structured checker above MIR. Unit/void types,
general expression statements, reference returns, destruction, generalized CFG
ownership/fixed points, interprocedural termination, and execution remain outside
this decision. Earlier KD sections retain their historical boundaries; KD-034
refines ordinary result/call/completion behavior without reopening other decisions.


## KD-035 — Ordinary Record Construction and Copy-Field Reads

**Status:** Accepted (frozen KED-020 Draft 4, Issue #41; K20-S01–S03,
K20-A01–A07 and K20-F01–F04 accepted).

> An ordinary record value may be constructed from an exactly typed field
> expression and may expose its Copy field through a direct named owned place.
> Construction produces a fresh owned record and participates in the existing
> complete-expression transaction. Copy-field access requires an available root
> that is not protected by an active exclusive whole-record loan, copies the
> field without moving or partially consuming the record, creates no field loan
> or reference provenance, and preserves the record's ownership state. Shared
> whole-record loans remain compatible with the read. Direct Copy-field reads
> may participate in ordinary Copy-valued conditions, while construction and
> other prohibited non-Copy activity remain excluded from recurring while conditions.

The current declaration schema remains exactly one field of a declared named
range. `RecordName { field: initializer }` is an ordinary expression wherever the
existing position rules permit that record value; nested conditional values do
not become legal beneath it. RecordId/FieldId and exact nominal initializer typing
are established before ownership. Equal bounds or field spellings do not confer
compatibility. Bare integer literals remain non-contextual (OQ-021); already
range-typed arithmetic may initialize a field without proving its bounds.

Construction is a fresh owned Move value, not an alias/reference, harness binding,
anonymous public local, new record type, allocation, executable constructor or
runtime hook. Initializer effects inherit complete-expression/KD-033/KD-034
rollback, including successful well-typed prefixes before a later ownership failure.
There is no parallel transaction engine. Recurrence and terminal permission rules
are unchanged; wrapping a prohibited operation in construction does not authorize it.

Copy-field roots are canonical ordinary owned ParameterIds/LocalIds only. Reads
require available roots, including all-path conditional availability, and reject
active exclusive loans required by finite uses, receiving holds or current/enclosing
recurrence. Shared loans allow immutable Copy reads without being weakened.
Reads create no field-level availability, transfer history, partial move, field
loan, provenance or hold. Whole-record moves and lawful loan expiry retain their
existing rules. Copy reads are legal in while conditions and stable body/backedge
states; construction is prohibited at any depth in a recurring condition.

The typed-HIR boundary validates canonical owner/record/field/result relationships
before ownership. Recursive visitors traverse construction children; MIR validates
structural trees, including hidden conditionals, without reconstructing HIR tables.
Public exclusive-loan witnesses require independently valid controls; recurrent
zero-finite/zero-hold protection and malformed Move-result metadata have separate
private evidence. Public field-declaration rejection is not partial-move evidence.
Verified battery behavior and harness literals remain isolated and unchanged.

Multiple/default/positional fields, broader field types, structural compatibility,
contextual literals, field borrowing/mutation, reference/temporary/nested projection,
general places, partial moves, non-Copy replacement, destruction, allocation,
execution, code generation and generalized cyclic analysis remain excluded. This
settles no general runtime initializer ordering or invariant/mixed-assurance rule.


## KD-036 — Multiple-field records and complete named construction

**Status:** Accepted

KED-021 Draft 3 (Issue #43) is frozen at baseline
`d83f0067eadd784cda2037340c9ff03276eeb0c5`, including K21-S01–S02,
K21-A01–A09 and K21-F01–F05. It extends KD-035's historical single-field
ordinary schema to one or more fields, each of a declared named-range type.
Field spellings are unique within a record. Commas separate declarations and
construction entries; a trailing comma is optional. Empty records are unsupported.

Declaration order is canonical metadata, determining FieldId allocation,
enumeration and missing-field diagnostic order. It establishes no layout, ABI,
serialization, reflection or destruction order. Named construction supplies every
field exactly once. Entries may appear in any order and retain written source
order through AST, resolution, typed HIR, ownership and MIR. Membership and
duplicates are checked in written order, completeness in declaration order, then
initializer types in written order, before ownership begins. Each initializer
must already have its selected field's exact nominal range type. Equal bounds,
structural record similarity and bare literals establish no implicit compatibility.
Copying an exact-typed value from another record's field is permitted; it does
not reuse the source FieldId as the destination's identity.

Initializers are evaluated left to right in written order inside the existing
complete-expression transaction. If a later initializer fails, all successful
prefix effects restore to the prepared entry: availability, loans, provenance,
holds, recurrence, allocator, target context and terminal permission. No destination
is published before all entries succeed. Completion adds no fallible balancing
stage, independent hold, transaction engine or permission, and preserves legitimate
enclosing call holds and recurrent obligations. KD-033 return and KD-034 whole-call
transactions remain authoritative. This orders record initializers only, not
arbitrary argument/operator evaluation, destruction or runtime unwinding.

A successful construction produces one fresh whole-record Move value, without
field owners, partial availability, field loans/provenance, harness bindings or
anonymous public locals. Any declared field supports direct named-owned
ParameterId/LocalId Copy access with its exact range result. Availability and
exclusive whole-record loans are checked; shared loans are compatible. The root
remains available until an ordinary whole-record Move. Copy reads may participate
in recurring while conditions; construction remains prohibited there at every
depth. Loop-body construction preserves existing recurrence and backedge rules.

Typed-HIR publication first validates every record/field declaration table,
including unused declarations: canonical table IDs, nonempty unique member vectors,
unique field spellings, valid range types, reciprocal ownership and exactly-once
field coverage. Checked lookup rejects malformed tables without panic or repair.
Construction validation checks nonempty complete unique field sets, canonical
membership, exact metadata, eligibility and recursive expression placement.
MIR preserves ordered typed trees and recursively validates every child without
reconstructing unavailable HIR tables. Verified state and the literal harness
remain explicitly restricted to their supported single-field shape.

Direct construction and field reads are transparent data primitives, not an
accessor/factory doctrine. Field syntax invokes no user code, property, allocation,
I/O, locking or invariant hook. Readability, constructibility and mutability remain
separate future authorities. Broader field types, defaults/positional/spread forms,
partial moves, field borrowing/mutation, reference/temporary/nested projection,
generalized places, contextual literals, invariants, visibility, layout,
destruction, execution, code generation and generalized cyclic ownership remain
outside this decision. KD-006 and OQ-004/OQ-012/OQ-016/OQ-019–OQ-026 retain their
unresolved scope. Ordinary records are checked/lowered, not executed or proven.
