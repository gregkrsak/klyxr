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
value functions with statement conditionals and ownership-stable while loops with iteration-local Move values alongside a narrow one-field record/signed-integer contract subset.
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
body scopes, and ordinary if/while composition (KD-027). KED-012 loop activity was owned
Copy Bool/range only: existing direct assignment and conditional initialization
compose. Reference creation/use/dereference/write-through and non-Copy activity
were rejected in conditions and bodies pending cyclic ownership/lifetime analysis.
KED-013 subsequently relaxes only iteration-local Move activity in bodies.
One permitted iteration must preserve the header's Move availability, reference
provenance and active loans after body locals are scoped away. Outside owners and
references may remain untouched; outside live loans still constrain Copy owner
reads/writes, while dead pre-loop loans stay dead. There is no fixed-point solver.
MIR uses Goto → header Branch → body/exit, with body Goto back to header. Its
validator now accepts cycles with visited tracking; structural validity does not
prove termination. Both constant-condition edges remain. Break/continue/for,
loop values, early returns, verified loops, and general cyclic ownership/loans
remain unresolved. Ordinary loops are neither executed nor proven.

KED-013 permits iteration-local Move ownership in ordinary while bodies (KD-028):
fresh record call results, transfer chains, by-value consumption, and existing
acyclic branch joins. Every loop snapshots its own pre-existing Move place set;
outer-loop locals are pre-existing at an inner header and cannot move there.
Body-local availability evolves normally and is scoped away before comparing
remaining header/backedge state. Pre-existing owners cannot transfer in the loop,
non-Copy conditions remain forbidden, references remain forbidden, and non-Copy
replacement remains unresolved. Lexical cleanup is not runtime destruction.
No syntax, HIR/MIR representation, second ownership engine, or fixed-point solver
is added. Ordinary record construction and execution remain absent.

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
