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
