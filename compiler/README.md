# First Klyxr compiler vertical slice

This Rust implementation is an executable **prototype**, not a general-purpose
Klyxr compiler. It implements a restricted source → lexer → parser → AST →
name resolution → expression type checking → typed HIR → integer contract proof
→ diagnostic path.

## Run it

Install Rust through rustup. `rust-toolchain.toml` pins Rust 1.80.0, rustfmt, and Clippy,
matching the workspace's minimum supported Rust version.

From the repository root:

```bash
cargo run --locked --bin klyxr -- verify examples/battery_ok.klx
cargo run --locked --bin klyxr -- check examples/battery_ok.klx
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

`check` and `verify` currently run the same restricted semantic and proof passes.
These commands are not implementations of the `safe` and `checked` assurance
levels. Success reports the proof scope and counts. Declaration-only inputs
explicitly report that there are no function contracts to verify.

Exit codes: `0` for supported obligations established, `1` for source/semantic/
proof failure, `2` for command usage or file-reading errors.

## Supported grammar

- Distinct named ranges: `type Percent = range 0..100;`. Both endpoints are
  inclusive. The prototype uses signed i64 values, including negative literals
  and the full i64 bounds. This does not settle future range representation.
- One-field records whose field type is a declared range.
- Verified functions with exactly one mutable record reference and one named
  range-valued amount parameter.
- Exactly one `requires expression` and one `ensures expression`; both must type
  as `Bool`. Contracts use ordinary expression trees, not bespoke operand records.
- Straight-line bodies containing zero or more `state.field -= expression;`
  statements. Every target resolves to the mutable record parameter's field.
  The RHS must type as that field's nominal range or as an integer literal.
- Record construction with a literal field value, followed by calls whose second
  argument is a literal. These top-level statements are a prototype execution
  harness, not a settled language entry-point design. Their source order matters.
- `fn`/`function` and `mut`/`mutable` normalize identically. The other KD-014
  aliases are not yet exercised by this grammar.
- ASCII identifiers, whitespace, and `//` comments.

Unsupported or malformed syntax is rejected. Bodies, postconditions, and unknown
names are never silently skipped. Type and function declarations are resolved
across the input; record bindings must be constructed before their use. Duplicate
names in a declaration namespace or the binding scope are rejected; shadowing
is outside this prototype. Passing an immutable binding to `&mut` is an error.

## Core expressions and types (KED-003)

Primary expressions are `true`, `false`, signed i64 literals, parameters,
`state.field`, `old(state.field)`, and parenthesized expressions. A record parameter
by itself is not a numeric expression. `old(...)` is allowed only in `ensures`
and only for the current mutable state field. Parentheses around that exact field
are permitted; arbitrary snapshots, nested objects, and aliases are not.

Only these operators are implemented, in order from highest to lowest precedence:

| Form | Associativity |
|---|---|
| Primary / `old(...)` | Grouping |
| `!` | Prefix Boolean negation |
| `-` | Left |
| `<=` | Non-associative |
| `==` | Non-associative |
| `&&` | Left |
| `||` | Left |

Unparenthesized repeated comparisons such as `a <= b <= c` and `a == b == c` are
rejected. Different comparison levels follow the stated precedence and still
must type-check. A minus sign on an integer literal preserves the existing signed
literal behavior; general unary numeric negation is not implemented.

Every HIR expression has an `ExprType`: `Bool`, `Range(RangeTypeId)`, or
`IntegerLiteral`. The last is an internal constant category, not a public numeric
base type or implicit conversion mechanism.

| Operation | Permitted operands | Result |
|---|---|---|
| `!` | Bool | Bool |
| `&&`, `||` | Bool, Bool | Bool |
| `==` | Bool/Bool; same-range/same-range; range/literal either way; literal/literal | Bool |
| `<=` | Same-range/same-range; range/literal either way; literal/literal | Bool |
| `-` | Same-range/same-range; range/literal | The left range identity |
| `field -= expression` | RHS is the target's range or an integer literal | Statement |

Named ranges remain distinct even with identical or overlapping bounds. Operator
compatibility uses `RangeTypeId`, never display-name equality. Only the two listed
subtraction combinations are enabled; literal-minus-range and literal-minus-literal
are rejected. No generalized coercion or arithmetic system is introduced.

A `Range(T)` result type does **not** prove overflow safety or membership in T's
bounds. Those remain verification obligations. A well-typed expression can still
be outside this prototype verifier's supported subset.

## Front-end architecture and API

`parse_source(source)` returns a source-oriented `ast::Program`: textual names,
spans, contracts, body statements, and ordered prototype statements. Parsing does
not decide which declarations those names denote. The supported keyword aliases
still normalize in the lexer.

`compile_source(source)` parses, calls `resolve::resolve(&ast)`, then calls
`types::check(resolved)`, returning canonical `hir::Program` or `FrontendError`
(`Lex`, `Parse`, `Resolve`, or `Type`). Compilation does not prove numerical
obligations. `resolve` now returns an opaque `ResolvedProgram`, consumed by
`types::check`; it no longer publishes typed HIR by itself. The resolver identifies
names and fields, while `compiler/types` owns operator legality, nominal
compatibility, result types, Boolean contracts, and subtract-assignment typing.
The type layer performs no textual source-name lookup.

The resolver collects the complete range/record/function declaration sets before
lowering references. Range and record names share the existing type namespace;
functions have their existing separate namespace. Binding lookup proceeds in
statement order and rejects use before construction and duplicate bindings.
No namespace, import, alias, or shadowing feature is introduced.

Typed HIR has separate `RangeTypeId`, `RecordId`, `FieldId`, `FunctionId`,
`ParameterId`, and `BindingId` newtypes indexing declaration tables within one
compilation. Fields carry their record and range IDs; parameters carry their
function and canonical type. `TypedExpr` stores kind, computed type, and span;
references use parameter and field IDs, and nominal types use range IDs. AST and
HIR functions each have one `requires` and one `ensures` expression. Parentheses
affect grouping and spans without introducing an extra HIR node.
Construction and calls carry record/field/function/binding IDs. Names and source
spans remain metadata for diagnostics. These IDs have no cross-compilation,
serialization, or ABI stability guarantee.

`verify::verify_report(&hir)` (and the diagnostics-only `verify`) consume canonical
HIR produced by resolution and type checking. Verification uses direct ID-based
table access and tracks current state by `BindingId`; names are used only for diagnostics and the
human-readable `final_values` report. It retains numerical initialization/argument
checks, record-ID compatibility and the existing mutable-binding check, universal
body proof, and ordered modular call-state reasoning.

`hir::Program` owns the canonical tables for its compilation-local IDs. Public
inspection is read-only: `ranges()`, `records()`, `fields()`, `functions()`,
`parameters()`, `bindings()`, and `statements()` return immutable slices, while
`range(id)`, `record(id)`, `field(id)`, `function(id)`, `parameter(id)`, and
`binding(id)` provide immutable ID-based lookup. External consumers cannot mutate
or reorder the underlying tables. Storage is crate-private so the resolver can
construct HIR directly; compiler-internal transformations are responsible for
preserving its identity and reference invariants.

The CLI follows this exact path:

```text
source → lexer → parser → AST expressions → resolver → expression type checking
       → typed HIR expressions → prototype verifier → diagnostics
```

Resolution and type errors prevent proof. Diagnostics distinguish malformed syntax,
unknown references, incompatible expression types, unsupported well-typed proof
shapes, and numerical counterexamples. Missing names are resolution errors;
non-Boolean contracts and incompatible operators are type errors. Names and spans
remain available for source diagnostics; internal IDs are not printed.

This implements KD-012 and KD-018 only for the documented subset. General type
inference, ownership/effects, MIR, VIR, and broader HIR features remain future work.

## What the proof establishes

The verifier recognizes the resolved structure of exactly this proof family:

```klyxr
requires amount <= state.field
ensures state.field == old(state.field) - amount
{ state.field -= amount; } // integer literal operands also supported
```

Parentheses that preserve this structure are accepted. Recognition uses parameter
and field IDs, not spelling. A typed conjunction such as
`requires (amount <= state.field) && true`, a different Boolean contract, or
`state.field -= amount - 0;` reaches verification but receives
`well-typed expression is not yet supported by the prototype verifier`, with the
relevant expression span. The adapter does not simplify expressions or prove
Boolean formulas. It lowers only supported body operands to private numerical
inputs; there is no second contract representation or broadened proof kernel.

For **each function in that proof family**, including uncalled functions, the checker proves
that for every input within the two declared ranges satisfying its precondition:

1. Every body subtraction fits signed i64.
2. Every assigned field value remains within its declared range.
3. The final field value equals its entry value minus the amount.

`old(state.field)` means the field's value at function entry. The currently
supported function can change only that one field. A failed obligation includes
an admissible counterexample and source location.

This is a specialized affine proof, not an SMT integration. The admissible input
domain is a rectangle clipped by `amount <= state`. The checker evaluates each
affine intermediate value and the final equality at every vertex of that polygon.
Affine extrema occur at vertices, so these checks establish the obligations over
the entire domain, rather than just observed calls. Vertex coordinates are
integral. Arithmetic during proof uses i128 to detect i64 overflow without host
overflow. The implementation and its mathematical argument are part of the
prototype's trusted base; there are no independently checked proof certificates.

An empty admissible domain is explicitly rejected by the numerical proof kernel. It does not
report a vacuous proof as a useful verification result. An empty body is allowed
only if it actually establishes the postcondition (for example, when the shared
named range type of the state field and amount is `range 0..0`).

At each call, the checker validates the record's distinct type, mutable binding,
amount range, and precondition using the **current** field value. It updates state
from the proven callee postcondition. A failed executable statement stops later
call analysis, avoiding conclusions based on stale state.

```klyxr
let mut battery = Battery { charge: 80 };
consume(&mut battery, 50); // leaves 30
consume(&mut battery, 50); // rejected: 50 <= 30 is false
```

## Limits and next work

The prototype does not implement general ownership/borrowing, moves, lifetimes,
effects, runtime contracts, expressions beyond this subset, loops, quantifiers,
multiple fields, record invariants, SMT/VIR/MIR, or machine-code generation. In particular,
`examples/effects.klx` is illustrative and is rejected rather than analyzed.
A successful prototype result must not be described as establishing those
unimplemented properties or unspecified program correctness.

This work implements portions of KD-005, KD-006, KD-012, KD-013, KD-014, KD-015, and KD-018,
subject to KD-017 and DP-008/DP-009. It does not reopen accepted language decisions
or freeze the broader language's syntax (OQ-018). Mixed assurance boundaries,
general mutation framing, and snapshot semantics beyond this one-field subset
remain design work.

KED-001 clarifies the nominal arithmetic boundary under KD-005; KED-003 applies
nominal identity to expression typing. KD-018 records that direction, while
OQ-021 retains broader constrained arithmetic questions.
The same-declaration restriction, enforced by `RangeTypeId` equality, does not
settle future generalized arithmetic,
conversion, coercion, subtyping, units, or operator-overloading rules. None of
those mechanisms is introduced here.

## Regression coverage

The tests cover invalid declarations/references, unsupported syntax, postcondition
failures, signed arithmetic limits, range preservation, independent records,
source-order execution, sequential calls, keyword aliases, CLI output and exit
codes, and nominal range-type compatibility. An independent exhaustive interpreter
compares the private affine proof kernel with every admissible integer input across
1,800 small range/body combinations, varying state and amount numeric domains
independently. This is an internal numerical test, not source-language permission
to mix distinct named types. Separate source-level regressions establish
same-declaration acceptance and rejection of distinct declarations with identical,
overlapping, or disjoint bounds. Resolution tests inspect canonical IDs and types across records, parameters,
contracts, bodies, construction, and calls, including declarations after their uses.
Metadata-renaming tests check that typing and verification are independent of
display names. Expression tests cover grouping, precedence, exact operator rules,
restricted snapshots, error phases, and the narrower verifier support boundary.
CI runs the tests in debug and optimized builds and Clippy with warnings
treated as errors.

The failing examples are intentional:

- `battery_fail.klx`: invalid first-call precondition.
- `battery_sequence_fail.klx`: invalid second-call precondition after mutation.
- `battery_body_fail.klx`: uncalled body fails its postcondition.
