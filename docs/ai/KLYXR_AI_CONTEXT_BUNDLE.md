# Klyxr AI Context Bundle

> Generated from the canonical files in `docs/ai/`.
>
> A fresh contributor or AI model should read this entire file before making major Klyxr design proposals.
>
> **Do not reinterpret Accepted decisions unless explicitly asked.**


---

<!-- BEGIN KLYXR_CONTEXT.md -->

# Klyxr Context

> **Read this first. You are joining an existing language-design project. Do not reinterpret settled decisions unless explicitly asked.**

## Project identity

**Klyxr** is a systems programming language in development.

Pronunciation: **“CLICK-ser.”**

Primary public site: `klyxr.com`  
Secondary owned domain: `klyxr.online`  
Primary source extension: `.klx`  
CLI executable: `klyxr`

Primary public line:

> **Systems programming you can reason about.**

Core philosophy:

> **Own what moves. Constrain what exists. Prove what matters. Make every escape hatch visible.**

Canonical public summary:

> **Klyxr is a systems programming language that treats memory safety as the foundation, then lets programmers express and verify more of the rules that make their software correct.**

## Why Klyxr exists

Klyxr begins from the premise that **memory safety is the beginning of the argument, not the end**.

Systems software also depends on rules such as:

- this value must remain within a valid range;
- this function may not allocate;
- this task must not block;
- this state transition is legal only under defined conditions;
- this API must preserve an invariant;
- this routine must not panic;
- this external assumption is trusted rather than proven.

Those facts often live in comments, tests, conventions, wrapper types, code review, runtime assertions, external tools, or institutional knowledge.

Klyxr's core proposition is:

> **More of those facts should be part of the program itself.**

Once they are part of the program, the compiler and verifier should be able to reason about them.

## Current implementation status

An executable compiler prototype, written in Rust, supports ordinary
value functions with statement conditionals and ownership-stable Copy while loops alongside a narrow one-field record/signed-integer contract subset.
Explicit resolution assigns compilation-local function, parameter, local, type,
field, and top-level binding IDs. Expression and value-flow type checking produce
canonical typed HIR. Public HIR inspection remains read-only; names/spans are
diagnostic metadata.

KED-004 introduced ordinary Bool/range value functions, immutable locals, calls,
and explicit `return expression;` (KD-019). KED-005 now permits existing records
by value in ordinary signatures, call results, locals, and returns. Ordinary and
verified functions retain one function namespace and ID table. Bare integer
literals do not materialize as locals, arguments, or returns.

The dedicated ownership phase runs after type checking. Bool and named ranges
are `Copy`; records are non-`Copy` and move through bindings, by-value arguments,
and returns (KD-020). ParameterId/LocalId state is per function; a moved place
cannot be reused. The compiler does not silently clone. Record fields do not make
records Copy. KED-006 adds explicit shared (`&T`) and exclusive (`&mut T`)
whole-value borrowing. Shared references are Copy; mutable references move.
KED-006 introduced `let mut` / `let mutable` for exclusive borrowing without
reassignment; KD-026 later adds narrow Copy-safe local reassignment. Immutable reference locals carry private loan provenance. Loans
end after the last possible use of derived handles on each path; potentially
available handles at a join can retain a possibly active loan. Call arguments
remain held until the receiving call completes, including nested arguments.
Availability and loans are separate ID-based state within the same ownership phase.
Borrowing/cloning/reborrowing/coercion are never implicit. Ordinary owned parameters
remain immutable. Reference returns, nested references, explicit lifetimes,
auto-dereference, ordinary record construction/field access, partial borrowing/moves,
and destruction remain unsupported (KD-021 / OQ-024).

KED-007 adds explicit `*reference` for directly named ordinary reference parameters
and locals. Copy referents (Bool/ranges) may be read through shared or mutable
references without consuming the handle. `*reference = expression;` requires an
exclusive mutable reference and an exact Copy referent/RHS type. Non-Copy move-out
and replacement are Ownership errors; no destruction or displaced-value semantics
are implied. Reads/writes count for last use; writes hold the loan throughout the
RHS. Copied dereference arguments do not call-hold a reference, while reference
arguments retain KED-006 holds. General owner replacement, field mutation, auto-deref,
reborrowing, and broader mutation remain unsupported (KD-022 / OQ-025).

KED-008 adds statement-only `if` / optional `else` with Bool conditions, nesting,
lexical child scopes, no visible-name shadowing, and distinct sibling LocalIds.
The condition's ownership effects occur once before independent branch snapshots.
An outer Move place remains available at the join only if every incoming path
retains it. KED-009 replaces whole-conditional loan holds with continuation-aware,
path-sensitive last-use analysis (KD-024): each branch inherits its own uses plus
the shared suffix, with edge-specific expiry after condition evaluation. Possible
active incoming loans merge conservatively; a conditionally moved handle cannot
erase a loan needed on another incoming path. Loans never reactivate along the
same path. Branch-local handles/loans do not escape. Straight-line last-use, call holds, and
write holds remain intact. Ordinary typed HIR lowers deterministically to read-only
MIR basic-block tables with explicit Branch/Goto/Return terminators. KED-010 adds initializer-only value conditionals (KD-025): mandatory else, one
expression per branch, nested complete branch results, Bool conditions, and exact
owned Bool/range/record types. Reference results and contextual integer-literal
materialization remain forbidden. Each Move leaf transfers into the same source
LocalId; moved sources retain KD-023 conditional unavailability. KD-024 path-specific
loan expiry and holds apply. MIR leaves initialize the canonical destination and
Goto a common join; no IfValue remains hidden in MIR expressions. There is no
phi, block parameter, SSA, general temporary/result slot, definite-assignment
solver, early return, cleanup, or MIR-based ownership solver. KED-012 later adds narrow statement loops. Conditional
values in returns/call arguments/general expression positions remain open under
OQ-022/OQ-024/OQ-026.

KED-011 adds direct `local = expression;` statements for mutable owned Copy Bool
and named-range locals (KD-026). Parameters, immutable locals, and reference locals
are Type errors; exact-typed non-Copy record replacement is an Ownership error.
RHS typing is exact and nominal without literal materialization. The RHS evaluates
before mutation, completes operation holds and last-use expiry, then the write
checks that no shared/exclusive loan of the target remains active. Self/repeated
assignment is valid; KD-024 sibling-path and post-join liveness remain unchanged.
AST/HIR/MIR distinguish Assign from Let and DerefAssign, preserving the existing
LocalId. KED-011 MIR was acyclic; KED-012 introduces real backedges. Assignment expressions, compound/chained/projected
assignment, conditional-value RHSs, reference replacement, non-Copy displacement,
destruction, and general dataflow remain unsupported.

KED-012 adds pre-test statement `while` with exact Bool conditions, nested lexical
body scopes, and ordinary if/while composition (KD-027). Loop activity is owned
Copy Bool/range only: existing direct assignment and conditional initialization
compose. Reference creation/use/dereference/write-through and non-Copy activity
are rejected in conditions and bodies pending cyclic ownership/lifetime analysis.
One permitted iteration must preserve the header's Move availability, reference
provenance and active loans after body locals are scoped away. Outside owners and
references may remain untouched; outside live loans still constrain Copy owner
reads/writes, while dead pre-loop loans stay dead. There is no fixed-point solver.
MIR uses Goto → header Branch → body/exit, with body Goto back to header. Its
validator now accepts cycles with visited tracking; structural validity does not
prove termination. Both constant-condition edges remain. Break/continue/for,
loop values, early returns, verified loops, and general cyclic ownership/loans
remain unresolved. Ordinary loops are neither executed nor proven.

Ordinary functions are parsed, resolved, type-checked, and ownership-checked,
including loan and borrowed-access legality, and lowered to ordinary MIR, but not executed or proven. A range result type does not establish that the
produced value meets its bounds. Plain `fn` does not yet implement the complete
future `safe` assurance model. Forward and recursive calls remain structurally
valid without a termination claim. Diagnostic traversal order does not settle
runtime evaluation order.

The verifier continues to recognize the original battery-contract structure by
IDs and rejects richer well-typed expressions inside that verified path with a
verifier-support diagnostic. It ignores ordinary functions as proof targets.
Its affine proof establishes subtraction safety, field ranges, and postconditions
for every admissible input of the supported structure, and tracks state between
literal-argument harness calls. Calls across function kinds are rejected; OQ-019,
OQ-021 through OQ-026 retain the broader assurance, arithmetic, function,
Copy/destruction, advanced borrowing, and general mutation questions.
General ownership beyond core moves and acyclic whole-value loans, effects, runtime
contracts, VIR, MIR backend/verification lowering, SMT integration,
and machine-code generation remain unimplemented. Read `compiler/README.md`
before making claims about what the executable establishes.

## Intellectual lineage

Klyxr deliberately combines lessons from **Rust** and **Ada/SPARK**, while aiming to become its own coherent language.

### Rust contribution

Rust demonstrated that ownership and borrowing can make strong memory-safety guarantees practical in a high-performance systems language.

Klyxr takes ownership as a foundational model.

### Ada/SPARK contribution

Ada/SPARK demonstrated that constrained types, contracts, static analysis, invariants, and formal proof can be practical tools for real systems engineering.

Klyxr carries that assurance culture into an ownership-based language and contemporary developer workflow.

### Klyxr's center of gravity

A useful public shorthand is:

> **Rust makes memory safety a core language guarantee. Klyxr takes that as the starting point and extends the same compile-time philosophy to contracts, system effects, domain constraints, trust boundaries, and formal verification.**

And:

> **SPARK brings formal methods to Ada. Klyxr is designed to make provable systems programming feel native to the ownership generation.**

These are explanatory simplifications, not claims that Rust or SPARK are incapable of adjacent features.

## Assurance model

Klyxr has three escalating assurance levels:

### `safe`

Ordinary language guarantees such as ownership, initialization, reference validity, bounded access, race restrictions, and deterministic destruction.

### `checked`

Additional contracts, ranges, invariants, or related rules are enforced dynamically where appropriate.

### `verified`

Declared proof obligations must be established statically by the verifier.

The same language is used across all three levels.

Key principle:

> **Verification is not a second language you graduate into. It is an assurance level you apply where it earns its cost.**

## Core language direction

Klyxr currently includes or intends to include:

- ownership and move semantics;
- immutable and mutable borrowing;
- deterministic destruction;
- no required garbage collector;
- distinct range/domain types;
- units and dimension-aware types;
- records with invariants;
- `requires` / `ensures` contracts;
- `old(...)` in postconditions;
- quantification for verification;
- Rust-like algebraic enums and exhaustive `match`;
- `Option<T>` rather than universal null;
- `Result<T, E>` and `?`-style error propagation;
- traits/generics with an explicit static-vs-dynamic dispatch distinction;
- explicit effects such as heap allocation, blocking, I/O, time, randomness, filesystem, networking, and unsafe behavior;
- auditable `safe`, `trusted`, and `unsafe` boundaries;
- embedded and realtime profiles;
- explicit data representation for hardware/protocol work;
- a Cargo-like unified toolchain.

## Public trust model

Klyxr must be precise about what a guarantee means.

A verified function does **not** mean:

> “This program is correct.”

It means:

> **The stated properties were proven under the stated assumptions.**

Klyxr should make it easy to distinguish:

- what the compiler knows;
- what the verifier proved;
- what the programmer trusted;
- where `unsafe` exists;
- and what remains unknown.

Do not market Klyxr as eliminating bugs, proving arbitrary global correctness, or making testing unnecessary.

## Flagship examples

### Application-level correctness

```klyxr
type Percent = range 0..100;

record Battery {
    charge: Percent
}

verified fn consume(
    battery: &mut Battery,
    amount: Percent
)
    requires amount <= battery.charge
    ensures battery.charge == old(battery.charge) - amount
{
    battery.charge -= amount;
}
```

A call that attempts to consume `90` from a battery at `80` charge is memory-safe but contract-invalid.

That distinction is central to Klyxr.

### Effects

```klyxr
#[deny_effects(heap, blocking)]
fn control_loop(...)
    effects clock
{
    estimate_state(...);
}
```

A transitive allocation should be diagnosable through the call graph.

The rule should be compiler-visible rather than merely documented.

## Brand system

Current locked brand direction:

- name: **Klyxr**
- pronunciation: **CLICK-ser**
- prose casing: `Klyxr`
- visual wordmark: `KLYXR`
- CLI / URL casing: `klyxr`
- icon/monogram: `KLX`
- source extension: `.klx`
- visual mode: dark-first
- primary aesthetic: premium developer tooling, precise, modern, non-hype
- palette direction: obsidian + electric cyan + signal blue + proof violet
- tone: exact, serious, plainspoken, engineer-respecting

## AI collaboration rules

When assisting with Klyxr:

1. Preserve accepted language decisions unless explicitly asked to revisit them.
2. Distinguish **accepted**, **current direction**, **proposed**, and **open**.
3. Do not silently turn a tentative idea into a language guarantee.
4. Do not overclaim verification.
5. Prefer coherent semantics over novelty.
6. Prefer diagnostics and tooling that explain *why* a rule exists.
7. When comparing Klyxr with Rust or Ada/SPARK, be respectful and technically defensible.
8. Treat the repository, not any one conversation, as canonical project memory.

<!-- END KLYXR_CONTEXT.md -->

---

<!-- BEGIN DESIGN_PRINCIPLES.md -->

# Klyxr Design Principles

These principles guide language and toolchain decisions.

## DP-001 — Own what moves

Ownership and borrowing are foundational, not optional library conventions.

## DP-002 — Constrain what exists

Types should be capable of describing the program's domain: bounded integers, percentages, voltages, durations, byte counts, physical units, legal state values, and invariants.

## DP-003 — Prove what matters

Formal verification should be available without requiring every function to be formally verified. Proof effort must be proportional to consequence.

## DP-004 — Make every escape hatch visible

Trust should not disappear into implementation detail. `trusted`, `unsafe`, FFI, hardware assumptions, allocation, blocking, and important effects should be discoverable and auditable.

## DP-005 — Memory safety is foundational, not sufficient

A program can be memory-safe and still violate a precondition, invariant, range, scheduling constraint, effect restriction, or protocol rule.

## DP-006 — One language, escalating assurance

`safe`, `checked`, and `verified` belong to one semantic language.

## DP-007 — Effects are part of an API

Whether a function allocates, blocks, performs I/O, accesses the clock, reads randomness, or crosses an unsafe boundary may be as important as its return type.

## DP-008 — The toolchain must explain itself

Diagnostics should explain what failed, what is known, what assumption is missing, which call introduced an effect, and what the developer can do next.

## DP-009 — Semantic honesty over marketing

Never claim that verification proves unspecified global correctness, that Klyxr eliminates bugs, or that testing is unnecessary.

## DP-010 — Explicitness without mandatory verbosity

Klyxr may provide standardized compact and explicit spellings for selected high-frequency keywords. The spellings are semantically identical.

## DP-011 — High assurance should feel like normal development

```bash
klyxr build
klyxr test
klyxr verify
klyxr audit
```

## DP-012 — Do not hide computational cost

Allocation, dynamic dispatch, blocking, and similar operational costs should not become invisible when they matter to systems programmers.

## DP-013 — Realtime claims must be defensible

Initially prove structural restrictions that the compiler can actually establish. Do not claim arbitrary WCET proof until the toolchain truly supports it.

## DP-014 — Project knowledge must be portable

If Klyxr can only be understood because one conversation remembers its history, the project is under-documented. A fresh expert should be able to read the repository context and become productive quickly.

<!-- END DESIGN_PRINCIPLES.md -->

---

<!-- BEGIN LANGUAGE_DECISIONS.md -->

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

<!-- END LANGUAGE_DECISIONS.md -->

---

<!-- BEGIN ARCHITECTURE.md -->

# Klyxr Compiler and Verification Architecture

## 1. High-level pipeline

```text
source (.klx)
   │
   ▼
lexer
   │
   ▼
parser
   │
   ▼
AST
   │
   ▼
name resolution
   │
   ▼
expression type checking
   │
   ▼
typed HIR
   │
   ├── type checking
   ├── ownership checking
   ├── region constraints
   └── effect checking
   │
   ▼
MIR
   │
   ├──────── verification path ────────▶ VIR ─────▶ SMT / prover
   │
   └──────── code generation path ─────▶ backend ─▶ machine code
```

The backend must not define Klyxr's source-level semantics.

## 2. AST

The AST preserves source structure for diagnostics, formatting, documentation, and syntax-aware tooling.

## 3. HIR

Typed HIR resolves source conveniences into clearer semantics.

Responsibilities may include:

- resolved names;
- canonical types;
- desugaring;
- normalized keyword aliases;
- generic/trait metadata;
- effect declarations;
- contracts.

Example:

```text
mut
mutable
   ↓
KW_MUTABLE
```

Original spelling remains available for source-aware tooling.

## 4. Ownership and regions

Availability (Available/Moved) and active shared/exclusive loans are orthogonal.
The current prototype checks moves, acyclic loans, and narrow Copy loop stability together in `compiler/ownership`.
Stored loans follow derived reference handles through last use; direct reference
arguments remain call-held through the receiving call. Broader regions/lifetimes
are future architecture; explicit syntax remains open under OQ-009/OQ-024.

## 5. Effect analysis

Conceptual judgment:

```text
Γ ⊢ expression : Type ! Effects
```

Effects propagate transitively.

A caller denying `heap` must not reach allocation through a hidden call chain.

## 6. MIR

The accepted long-term MIR direction is control-flow oriented and suitable for:

- ownership analysis;
- drop elaboration;
- borrow checking;
- effect propagation;
- contract instrumentation;
- verification lowering;
- backend lowering.

## 7. VIR

VIR is the restricted verification representation.

Expected concepts include:

- `assume`
- `assert`
- `forall`
- `exists`
- `old`
- `invariant`
- `variant`
- `havoc`

An initial verifier may target Z3 or another SMT solver. Solver choice is not language semantics.

## 8. Contract semantics

Conceptually:

- caller establishes `requires`;
- callee establishes `ensures`;
- invariants hold at defined semantic boundaries.

Checked mode may lower contracts to runtime checks.  
Verified mode generates proof obligations.

## 9. Loop verification

Verified loops may require invariants and termination variants where termination must be proven.

## 10. Trust tracking

`trusted` statements/declarations should retain:

- source location;
- rationale;
- direct dependents;
- transitive dependents where feasible.

A future command may support:

```bash
klyxr audit --trust
```

## 11. Unsafe boundaries

Unsafe operations should be lexically visible.

Possible future direction:

```klyxr
unsafe
    assumes ptr != null
    assumes ptr aligned_for T
{
    ...
}
```

Exact syntax remains open.

## 12. Realtime / embedded profiles

Initial support should focus on structural restrictions such as:

- no heap;
- no unbounded blocking;
- constrained recursion;
- bounded loops where policy requires it.

Do not claim arbitrary WCET analysis until it truly exists.

## 13. Runtime strategy

Klyxr should support modular runtime configurations from hosted targets to bare metal.

## 14. Implementation direction

Current direction:

- first compiler likely implemented in Rust;
- LLVM is a plausible initial code-generation backend;
- Z3 is a plausible initial SMT backend;
- self-hosting is a long-term possibility.

These are implementation choices, not permanent language semantics.

## 15. Proposed repository structure

```text
compiler/
├── lexer/
├── parser/
├── ast/
├── resolve/
├── types/
├── ownership/
├── effects/
├── hir/
├── mir/
├── vir/
├── verify/
├── codegen/
└── diagnostics/

runtime/
std/
tools/
tests/
examples/
docs/
rfcs/
```

## 16. MVP boundary

A narrow first compiler should target:

- integers and `Bool`;
- bindings and mutation;
- functions;
- records;
- `if` / `while`;
- range types;
- basic `Option` / `Result`;
- basic matching;
- moves and borrows;
- `requires` / `ensures`;
- deterministic checked arithmetic;
- machine-code backend;
- simple integer verification.

The MVP should demonstrate the difference between memory safety and application-level verified correctness before pursuing breadth.

## 17. Current executable prototype

The compiler prototype is implemented in Rust, pinned to 1.80.0. The executable
pipeline is source → lexer → parser → AST → explicit name resolution → expression
and value-flow type checking → typed HIR → core ownership, loan, and borrowed-access checking →
ordinary MIR CFG lowering (ordinary functions) or specialized
integer contract proof (verified functions) → diagnostics. This implements KD-012, KD-018, KD-019,
KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, and KD-027 only for the documented
subset; the broader pipeline above remains accepted architecture/future work.

The AST preserves textual declarations, parameter types, explicit ordinary
returns, statement conditionals with optional else/nested lexical branches,
local initializers/mutability, flat source reference types, direct named
borrow/dereference targets, dedicated ordinary dereference writes, expression calls,
contracts, and ordered prototype
statements. `FunctionDecl` retains source order across ordinary and verified forms.
The resolver collects all declarations/signatures before resolving bodies, owns
name lookup and source-order local scope, and assigns strongly typed range, record,
field, function, parameter, local, and harness-binding IDs. Locals enter scope after
resolving their initializer; visible-name shadowing and cross-function local scope are rejected.
Each branch clones the visible resolver scope; siblings may reuse a spelling,
but every declaration receives a fresh LocalId from the compilation-wide allocator.
Forward and recursive ordinary calls resolve without execution or termination
analysis. Source type names resolve to canonical references; `compiler/types`
validates declared value types, operators, initializers, calls, and returns without
textual name lookup.

One canonical `FunctionId` table contains both ordinary value and verified mutation
functions, in source declaration order. Both kinds share the existing function
namespace. Ordinary signatures/materialized locals use `ValueType::Bool`,
`ValueType::Range(RangeTypeId)`, `ValueType::Record(RecordId)`,
`SharedRef(ReferentType)`, or `MutableRef(ReferentType)`; `IntegerLiteral`
remains expression-only. Expressions represent records by the same canonical
RecordId. Record arguments and returns require exact nominal identity; existing
operators do not accept record or reference values. Reference referents are
nonrecursive Bool/range/record identities. Borrow expressions carry canonical
ParameterId/LocalId targets and Shared/Mutable capability; locals carry owned
mutability. Nested reference signatures and reference returns are rejected before ownership. HIR
local and call references carry `LocalId` and `FunctionId`, with typed arguments,
initializers, and explicit returns. Nominal compatibility uses exact range/record IDs.
Names and spans are diagnostic metadata. `hir::Program` owns compilation-local
canonical tables, including locals, and exposes read-only slices/ID lookups.
Storage is crate-private; internal construction/transformations must preserve
identity and reference invariants. IDs have no cross-compilation stability.

Ordinary functions have explicit value return types and exactly one final
`return expression;`. Immutable locals propagate concrete initializer types;
call arguments and returns require exact concrete types. Bare integer locals,
arguments to range parameters, and returns do not gain implicit range types.
KED-004 through KED-011 do not execute, prove, or dynamically enforce ordinary functions.
`Range(T)` typing does not prove bounds or overflow safety. Plain `fn` is not a
claim to implement the full future `safe` model. All mixed-kind calls are rejected.

`compiler/ownership` is a dedicated phase that inspects canonical typed HIR without
rebuilding it or adding public ownership annotations. Its private classification
is Bool/range/shared reference → Copy, record/mutable reference → Move,
regardless of record field types. It tracks only
ordinary ParameterId/LocalId places, using separate strongly typed place
variants in a per-function state table. Record parameters begin Available. Local
initializers are analyzed before introducing a fresh local owner. Value references
consume Move places through bindings, by-value calls, and returns; Copy places
remain reusable. Call results are fresh values, without public temporary IDs.

State is Available, Moved with the prior source span/transfer destination, or
conditionally unavailable after a branch join with a contributing move span.
Use-after-move produces the first precise `FrontendError::Ownership` diagnostic,
with the later use span and prior move location. Names are diagnostic metadata;
classification and state use canonical types and IDs. Children are traversed in
source order for deterministic diagnostics, without settling runtime evaluation
order. KED-008 adds independent post-condition branch snapshots and acyclic joins.
An outer Move place moved on either path becomes conditionally unavailable, with
a prior move span for diagnostics. There are no fixed points or inlined callee states.
Forward/recursive calls are checked structurally, not executed or proven terminating.
Unused owners are accepted without destruction behavior.

The ownership phase excludes the verified state parameter, fields, and BindingId
harness state. KED-006 adds private loan identity/provenance alongside availability,
with continuation-aware path-specific future-use counts (KED-009) and operation-hold counts for nested
calls and (under KED-007) whole write statements.
Shared copies and mutable moves retain loan identity across LocalIds. Loans expire
when no potentially available derived handle has future uses on the current path
and no call/write holds the loan. ConditionalMove blocks handle use but does not
by itself eliminate a loan active on another incoming path. Local
transfers attach destination provenance before expiry; final-use arguments attach
call holds before expiry. Reference parameters have external provenance without
interprocedural owner reconstruction. Loan state is not published in HIR.
Exclusive borrowing requires a mutable owned local; `let mut` introduced exclusive borrowing under KED-006; KD-026 now adds Copy-safe local assignment.
There is no ordinary field access/record construction, reborrowing, reference return,
implicit reference coercion, partial move/borrow, user-defined Copy/Clone, or destructor. Source names
and nominal type errors must be resolved before ownership checking.

KED-007 represents `Deref { reference: Place }` with the exact canonical referent
ExprType and a dedicated `DerefAssign { reference, value, span }` statement. The
resolver identifies existing ordinary parameter/local targets; typing requires
reference operands, exclusive write capability, and exact RHS compatibility.
Ownership rejects borrowed non-Copy value materialization and non-Copy replacement.
No generalized projection/place architecture or additional frontend pass is added.
Read/write access checks handle availability without transferring it or creating
loans. Both count for last use. Copy reads can expire the original loan immediately
after access, and their result does not call-hold a reference. Write statements hold
the original loan through RHS traversal, then expire normally after completion;
RHS calls cannot invalidate the target handle and leave an accepted write.
No destruction, field mutation, or auto-deref is implemented. KD-026 adds separate Copy-safe local assignment.

The verifier takes only a verified-function view from the shared HIR table;
ordinary functions are ignored as proof targets and never counted as proven.
Verified AST/HIR contracts remain expressions, and subtraction statements have
expression operands. The verifier recognizes only the original resolved battery
proof structure and parameter/literal body operands. Richer well-typed expressions
inside verified functions receive spanned verifier-support diagnostics. The
private affine kernel and its independent 1,800-case cross-check retain their
existing mathematical coverage, including independently varying numeric domains.

Ordered record construction/call statements remain the prototype harness. The
checker retains distinct-record checks, range checks, and immutable-binding
rejection. Every verified body is proven independently of its call sites; its
postcondition then updates caller state in source order. Ordinary functions do
not enter that state model.

KED-009 removes KED-008's whole-conditional loan pin. Future-use accounting is
private and independent of availability, keyed by canonical ParameterId/LocalId
places. After analyzing a condition once, the remaining suffix is decomposed
into then uses, else uses, and a common continuation. Each branch starts from
the same post-condition ownership snapshot, with only its own uses plus that
continuation. Its edge expires loans with no future surviving handle uses.
Nested statement-list analysis composes this decomposition recursively, and
operation-level last-use expiry remains unchanged. No constant-path pruning occurs.

At joins, outer Move availability retains KD-023's all-path rule. Only loan
provenance that existed before the split may survive; activity is the union of
incoming states. Dead-on-all-path loans are removed. ConditionalMove can retain
possibly active provenance while rejecting any handle use. Branch-local handles
and loans are discarded; sibling-created private LoanIds are never conflated.
Future uses after the join keep needed loans continuously active along each
relevant incoming path: no loan resurrection occurs. Call/write holds remain
balanced through their operation and cannot escape statements or branch exits.

Ownership remains above MIR in `compiler/ownership`; there is no second checker,
MIR ownership pass, general dataflow framework, or fixed-point solver. This is
whole-value liveness for structured acyclic statement and initializer branches, not arbitrary CFG borrowing
or a claim of Rust borrow-checker equivalence. OQ-024/OQ-026 retain general lifetimes,
cyclic lifetime analysis, reborrowing, escaping references, and broader MIR ownership dataflow.

`compiler/mir` lowers only ordinary typed HIR after frontend ownership checks.
`mir::lower(&program)` leaves `compile_source`'s canonical HIR result unchanged.
MIR preserves FunctionId/ParameterId/LocalId and exact expression types. Public
inspection is read-only; BasicBlockId and canonical block storage are compilation-local.
Each block owns Let/Assign/DerefAssign statements and exactly one Goto, typed Bool Branch,
or value Return terminator. Expressions retain their typed HIR trees. Structured
recursive lowering allocates blocks deterministically, gives every fallthrough
an explicit Goto, and emits the final source return as a terminator. Empty else
branches use a direct false edge to the join. A structural validator checks entry,
targets, reachability, and Bool conditions; unique IDs and single
terminators follow from indexed storage and the block type. KED-012 permits cycles;
visited tracking terminates validation without claiming paths reach Return. MIR discovers no new source semantic errors. The CLI exercises lowering.

KED-010 adds initializer-only conditional values with mandatory else and exactly
one expression per branch. AST/resolved/typed HIR retain explicit IfValue nodes;
resolution uses the same visible scope without introducing branch declarations.
The destination enters scope after its whole initializer resolves. Type checking
requires Bool conditions and exact owned Bool/range/record identity; references
and IntegerLiteral results cannot materialize conditional destinations.

The existing ownership checker shares snapshot/split/join helpers for statement
and value conditionals. It evaluates the condition once, gives each branch its
own future uses plus the continuation, and transfers every leaf into the same
canonical source LocalId. Copy sources survive; moved sources become conditionally
unavailable after the join. The destination is inserted as available only after
the whole initializer completes. KD-024 provenance and operation holds are unchanged.

MIR value lowering emits explicit Branch blocks with distinct outgoing targets.
Every nested leaf emits an existing Let statement for the same source destination
and Goto to a common control-flow join. Multiple mutually exclusive initializations
are not reassignment. No hidden IfValue survives inside any MIR expression; the
structural validator checks this. Deterministic construction and mandatory value
branches guarantee one initialization per reachable path without a general
definite-assignment solver. This first destination-local strategy is not a final
general MIR solution for all future value joins (OQ-026).

The executable flow is typed HIR → ownership/path-sensitive acyclic loans → MIR
statement CFGs and destination-local value joins. MIR still has no backend or
verification consumer. The verified battery path excludes ordinary conditionals.

KED-011 extends ordinary statements with Assign. The AST preserves the direct target
spelling, RHS, and complete span. Resolution binds the target to a canonical Place;
typing rejects parameters, references, and immutable locals, requires exact RHS
identity, and publishes only a LocalId target. It creates no declaration or new ID.
Exact-typed non-Copy record replacement reaches the ownership rejection boundary.

For mutable Bool/range targets, the existing ownership checker analyzes the RHS
under current state, completes call holds and last-use expiry, then checks the
write against every active shared/exclusive loan of that owner. This RHS-first,
write-second rule allows final RHS reference uses to end before mutation. KD-024
branch-specific future counts include RHS uses and the continuation; loan merging,
Move/ConditionalMove availability, call holds, and write-through holds are unchanged.
Non-Copy replacement fails before either owner is consumed or displaced.

MIR Assign retains the source LocalId, typed RHS, and span; it is distinct from
Let initialization and DerefAssign. Straight-line lowering adds no blocks; branch
assignments stay in the current block. Expression validation also inspects Assign
RHSs for hidden IfValue nodes. KED-012 subsequently replaces MIR cycle rejection with cycle-safe reachability. There is no
second ownership engine, general place model, reference provenance replacement,
destruction or fixed-point infrastructure. KED-012 adds loop backedges as described below.

The implemented ordinary path now includes initialization, conditional initialization,
Copy-safe mutable-local reassignment, and explicit dereference write-through →
ownership/path-sensitive acyclic loans and Copy loop stability → MIR Let/Assign/DerefAssign statements
with explicit Branch/Goto/Return terminators. Ordinary code remains non-executable
and unverified, and the specialized battery path remains unchanged.

There are no phi nodes, block parameters/results, SSA, general temporaries,
cleanup edges, execution, or MIR backend/verifier consumers. The specialized
verified path stays on HIR and retains its proof counts and numerical kernel.
OQ-026 records future control-flow/value-flow/MIR dataflow design.

VIR, ownership beyond core moves and acyclic whole-value loans, advanced
borrowing, effect analysis, destruction, SMT integration, runtime lowering,
and code generation remain unimplemented. This subset does not settle broader
HIR, modules, generalized inference/coercions, arithmetic enforcement, or mixed
assurance semantics.

For the supported precondition `amount <= state.field` and postcondition
`state.field == old(state.field) - amount`, the proof establishes signed i64
subtraction safety, range preservation at each assignment, and the final equality
over every admissible input. It uses affine extrema at the vertices of the
range rectangle clipped by the precondition. This is a specialized proof with
the implementation in its trusted base, not an SMT proof or proof certificate.
Empty admissible domains are explicitly rejected by this prototype.

The exact grammar, mathematical argument, CLI behavior, and limitations live in
`compiler/README.md`. The broader language's representation, assurance-boundary,
invariant, and snapshot rules remain open. Relevant entries: KD-005, KD-006, KD-012,
KD-013, KD-014, KD-015, KD-017, KD-018, KD-019, KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, DP-008, DP-009, and OQ-018 through OQ-026.


### KED-012: while and cyclic MIR

AST, resolved statements and typed HIR preserve explicit While condition/body/span.
Resolution gives the body a child scope using the existing canonical LocalId
allocator; type checking establishes exact Bool before ownership. Ordinary branch
and loop bodies cannot contain returns. Verified syntax remains separate.

The existing HIR ownership checker enters a private loop mode and analyzes one
permitted iteration from a header snapshot. Every nested expression/statement must
use only owned Copy values; reference activity and non-Copy values reject with
source-oriented future cyclic-analysis diagnostics. The check compares outer Move
availability, handle provenance, loans (including balanced operation holds), and
loan allocation state at the backedge after body-local state is scoped away.
Nested loops use the same check independently. FutureUses only advances the finite
outside suffix; prohibited recurring handles are never approximated by one count.
Outside live loans are preserved and still checked by Copy owner access/assignment.
There is no fixed-point, MIR ownership pass, or second engine.

Deterministic lowering reserves a header/body/exit, emits Goto from the preheader,
Branch at the header, and Goto from the completed body back to the header. Nested
if/value-initialization joins and nested loops compose recursively. Existing typed
expressions and canonical source IDs survive; no MIR While instruction is needed.
The validator uses visited marks to accept cycles while retaining target,
reachability, Bool and hidden-IfValue checks. It makes no termination claim and
retains body/exit edges for constant conditions. The ordinary pipeline is source
→ AST → resolution → typed HIR → ownership/acyclic loans/Copy loop stability →
MIR CFG. Backend and verification lowering remain unimplemented; the specialized
battery proof path and numerical kernel remain unchanged.

<!-- END ARCHITECTURE.md -->

---

<!-- BEGIN OPEN_QUESTIONS.md -->

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
iteration, without generalized cyclic dataflow. Non-Copy activity in loops,
cyclic reference liveness, verified loops/invariants/variants and termination
checking remain open, alongside the questions above.

KED-008/KED-010/KED-011/KED-012 MIR choices do not settle or preclude these broader features.

<!-- END OPEN_QUESTIONS.md -->
