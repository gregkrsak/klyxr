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
arguments, dereference and write-through) and all non-Copy activity (including
fresh record call results and nested consumption) are rejected pending cyclic
ownership/lifetime analysis. A reference being Copy does not bypass this boundary.
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
