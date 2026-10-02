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

An executable compiler prototype, written in Rust, supports ordinary straight-line
value functions alongside a narrow one-field record/signed-integer contract subset.
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
records Copy. Ordinary record construction, field access, partial moves, mutable
locals, borrowing, and destruction remain unsupported.

Ordinary functions are parsed, resolved, type-checked, and ownership-checked,
but not executed or proven. A range result type does not establish that the
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
OQ-021, OQ-022, and OQ-023 retain the broader assurance, arithmetic, function,
and Copy/destruction questions.
General ownership beyond this core move model, borrowing, effects, runtime
contracts, MIR/VIR, SMT integration,
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

Relevant values/bindings conceptually transition among:

- Owned
- Borrowed immutable
- Borrowed mutable
- Moved

Regions/lifetimes should primarily be inferred.

## 5. Effect analysis

Conceptual judgment:

```text
Γ ⊢ expression : Type ! Effects
```

Effects propagate transitively.

A caller denying `heap` must not reach allocation through a hidden call chain.

## 6. MIR

MIR is control-flow oriented and suitable for:

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
and value-flow type checking → typed HIR → core ownership checking → specialized
integer contract proof → diagnostics. This implements KD-012, KD-018, KD-019,
and KD-020 only for the documented
subset; the broader pipeline above remains accepted architecture/future work.

The AST preserves textual declarations, parameter types, explicit ordinary
returns, local initializers, expression calls, contracts, and ordered prototype
statements. `FunctionDecl` retains source order across ordinary and verified forms.
The resolver collects all declarations/signatures before resolving bodies, owns
name lookup and source-order local scope, and assigns strongly typed range, record,
field, function, parameter, local, and harness-binding IDs. Locals enter scope after
resolving their initializer; no shadowing or cross-function local scope is supported.
Forward and recursive ordinary calls resolve without execution or termination
analysis. Source type names resolve to canonical references; `compiler/types`
validates declared value types, operators, initializers, calls, and returns without
textual name lookup.

One canonical `FunctionId` table contains both ordinary value and verified mutation
functions, in source declaration order. Both kinds share the existing function
namespace. Ordinary signatures/materialized locals use `ValueType::Bool`,
`ValueType::Range(RangeTypeId)`, or `ValueType::Record(RecordId)`; `IntegerLiteral`
remains expression-only. Expressions represent records by the same canonical
RecordId. Record arguments and returns require exact nominal identity; existing
operators do not accept record values. HIR
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
KED-004 and KED-005 do not execute, prove, or dynamically enforce ordinary functions.
`Range(T)` typing does not prove bounds or overflow safety. Plain `fn` is not a
claim to implement the full future `safe` model. All mixed-kind calls are rejected.

`compiler/ownership` is a dedicated phase that inspects canonical typed HIR without
rebuilding it or adding public ownership annotations. Its private classification
is Bool/range → Copy, record → Move, regardless of field types. It tracks only
ordinary by-value ParameterId/LocalId places, using separate strongly typed place
variants in a per-function state table. Record parameters begin Available. Local
initializers are analyzed before introducing a fresh local owner. Value references
consume Move places through bindings, by-value calls, and returns; Copy places
remain reusable. Call results are fresh values, without public temporary IDs.

State is Available or Moved with the prior source span and transfer destination.
Use-after-move produces the first precise `FrontendError::Ownership` diagnostic,
with the later use span and prior move location. Names are diagnostic metadata;
classification and state use canonical types and IDs. Children are traversed in
source order for deterministic diagnostics, without settling runtime evaluation
order. There are no branches, joins, fixed points, or inlined callee states.
Forward/recursive calls are checked structurally, not executed or proven terminating.
Unused owners are accepted without destruction behavior.

The ownership phase excludes the verified state parameter, fields, and BindingId
harness state. It adds no borrowing, mutable locals, ordinary field access/record
construction, partial moves, user-defined Copy/Clone, or destructors. Source names
and nominal type errors must be resolved before ownership checking.

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

MIR, VIR, ownership beyond this straight-line move subset, borrowing/effect
analysis, destruction, SMT integration, runtime lowering,
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
KD-013, KD-014, KD-015, KD-017, KD-018, KD-019, KD-020, DP-008, DP-009, and OQ-018 through OQ-023.

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

KED-003, KED-004, and KED-005 do not settle these questions implicitly.


## OQ-022 — General function and local-value semantics

KED-004 established a straight-line value-function core with Bool/range values.
KED-005 extends it with by-value records and the basic binding/call/return move
rules accepted in KD-020. Those core move rules are no longer wholly unresolved;
this does not settle the broader function/local semantics below.

Still open:

- whether unqualified `fn` is the final surface spelling of the base `safe` assurance level;
- explicit `safe fn` syntax, if any;
- unit/no-value function return semantics;
- early returns and multiple control-flow return paths;
- mutable locals and assignment;
- expression-bodied function shorthand;
- contextual integer-literal typing at bindings, arguments, and returns;
- broader value categories and record/reference interfaces beyond the current by-value record subset;
- function values, closures, and higher-order calls;
- function overloading;
- interaction of calls and owned places with future borrowing;
- runtime enforcement of constrained return values outside verified code.

OQ-019 remains authoritative for mixed `safe` / `checked` / `verified` call
boundaries. OQ-021 remains authoritative for unresolved constrained arithmetic
and literal-conversion rules. Copy customization and destruction remain under OQ-023.


## OQ-023 — Copy customization, cloning, partial moves, and destruction

KED-005 establishes only the core `Copy` / move distinction.

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

Borrowing and lifetime rules remain separate questions under KD-004, OQ-009,
and future directives. No runtime destruction or resource release is implemented
by the core ownership checker.

<!-- END OPEN_QUESTIONS.md -->
