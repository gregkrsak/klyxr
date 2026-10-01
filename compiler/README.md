# First Klyxr compiler vertical slice

This Rust implementation is an executable **prototype**, not a general-purpose
Klyxr compiler. It implements a restricted source → lexer → parser → AST →
name resolution → expression and value-flow type checking → typed HIR → integer contract proof
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
explicitly report that there are no function contracts to verify. Ordinary-only
inputs report their type-checked function count and explicitly state that they
are not executed or verified; constrained results are not proven. In mixed inputs,
only verified mutation functions contribute to the proof counts.

Exit codes: `0` for frontend acceptance and any supported verified obligations established, `1` for source/semantic/
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

## Ordinary value functions (KED-004)

```klyxr
type Percent = range 0..100;

fn remaining(current: Percent, used: Percent) -> Percent {
    let result = current - used;
    return result;
}

function wrapped(current: Percent, used: Percent) -> Percent {
    return remaining(current, used);
}

fn can_use(current: Percent, used: Percent) -> bool {
    let enough = used <= current;
    return enough;
}
```

See `examples/value_flow.klx` for these flows and a forward call.

Ordinary functions accept zero or more named immutable parameters, each typed as
built-in source `bool` (semantic `Bool`) or a declared named range. The explicit
`-> Type` return declaration permits only those same types. `bool` is a reserved
built-in type token, so user range/record declarations cannot redefine it. Ordinary
record/reference interfaces, unit, generics, and inferred return types are rejected.

The body contains immutable `let name = expression;` locals followed by exactly
one final **`return expression;`**. Its semicolon is mandatory. Klyxr deliberately
rejects implicit block tails, with or without a semicolon (KD-019). Missing returns
are type errors; bare tails and statements after return receive explicit parser
diagnostics. No general expression statements, early/multiple returns, local
annotations, `let mut`/`let mutable`, or reassignment are implemented. The existing
top-level mutable record harness is unchanged.

Parameters are visible at entry; a local enters scope after its initializer is
resolved. Later locals and the return can reference earlier locals. Self-reference,
use before declaration, parameter/local duplicates, shadowing, and cross-function
scope leakage are resolution errors. Initializers propagate their concrete Bool
or named range type. `let five = 5;` is a type error: the internal IntegerLiteral
category has no settled value-materialization rule. `let result = amount - 5;`
is valid because the existing subtraction rule computes a concrete range type.

Ordinary calls are expressions, including nested calls and operator operands.
Complete signatures are collected before bodies, so forward and direct/indirect
recursive calls resolve. No termination or recursion policy is introduced. Calls
require exact arity and exact concrete argument types; their result has the declared
return type. Returns likewise require exact concrete types. Distinct ranges cannot
be passed or returned implicitly even with identical bounds. Bare integer literals
cannot satisfy a range parameter or return type, while Bool literals already type
as Bool. Generalized contextual literal conversion remains open under OQ-021.

Ordinary functions are **frontend semantic work only**: parsed, resolved, and
type-checked, with no execution, code generation, range proof, or runtime checks.
For example, `return current - used;` can type as Percent without establishing
that every result meets Percent's bounds. The verifier ignores ordinary functions
as proof targets. Plain `fn` does not yet implement the full `safe` assurance model,
and repeated value uses imply no future Copy/move rule. Ownership remains future
work under KD-004. Ordinary calls may target only ordinary functions; verified
contracts/body expressions cannot call functions, and top-level harness calls may
target only verified functions. These rejections leave OQ-019 unresolved.

## Core expressions and types (KED-003)

Primary expressions are `true`, `false`, signed i64 literals, parameters,
`state.field`, `old(state.field)`, and parenthesized expressions. Ordinary functions additionally support locals and
ordinary calls. A record parameter
by itself is not a numeric expression. `old(...)` is allowed only in `ensures`
and only for the current mutable state field. Parentheses around that exact field
are permitted; arbitrary snapshots, nested objects, and aliases are not.

Only these operators are implemented, in order from highest to lowest precedence:

| Form | Associativity |
|---|---|
| Primary / calls / `old(...)` | Grouping |
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
bounds. Those require proof or a future runtime enforcement rule; ordinary functions
add neither. A well-typed expression can still be outside this prototype verifier's supported subset.

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
names, fields, callees, and source-ordered local scope. It resolves type names to
canonical type references. `compiler/types` validates concrete declared value types,
operator legality, nominal compatibility, result types, Boolean contracts,
subtract-assignment, initializer propagation, call arguments, and explicit returns.
The type layer performs no textual source-name lookup.

The resolver collects the complete range/record/function declaration sets before
lowering references. Range and record names share the existing type namespace;
ordinary and verified functions share the existing separate function namespace.
Binding lookup proceeds in statement order and rejects use before construction and duplicate bindings.
No namespace, import, alias, or shadowing feature is introduced.

Typed HIR has separate `RangeTypeId`, `RecordId`, `FieldId`, `FunctionId`,
`ParameterId`, `LocalId`, and `BindingId` newtypes indexing declaration tables within one
compilation. Fields carry their record and range IDs; parameters carry their
function and canonical type. `TypedExpr` stores kind, computed type, and span;
references use parameter, local, field, and function IDs, and nominal types use
range IDs. AST `FunctionDecl` and HIR `Function` each distinguish Ordinary and
Verified forms in one source-ordered function table. `functions()` and `function(id)`
now return the common HIR `Function`, with `id()`, `name()`, `as_ordinary()`, and
`as_verified()` read-only views. This is an intentional prototype API change.
Verified functions retain one `requires` and one `ensures` expression; ordinary
functions carry parameter IDs, a `ValueType` return, typed lets and a typed explicit
return. `ValueType` permits only Bool/Range, excluding IntegerLiteral. Ordinary
parameters use `ParameterType::Value(ValueType)`; verified parameter types are
unchanged. Locals carry their owning FunctionId and concrete type; BindingId
continues to denote only top-level record bindings. Parentheses
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
`parameters()`, `locals()`, `bindings()`, and `statements()` return immutable slices, while
`range(id)`, `record(id)`, `field(id)`, `function(id)`, `parameter(id)`, and
`local(id)` and `binding(id)` provide immutable ID-based lookup. External consumers
cannot mutate or reorder the underlying tables. Storage is crate-private so the resolver can
construct HIR directly; compiler-internal transformations are responsible for
preserving its identity and reference invariants.

The CLI follows this exact path:

```text
source → lexer → parser → AST expressions/functions → resolver → expression and value-flow type checking
       → typed HIR expressions → prototype verifier → diagnostics
```

Resolution and type errors prevent proof. Diagnostics distinguish malformed syntax,
unknown references, incompatible expression types, unsupported well-typed proof
shapes, and numerical counterexamples. Missing names are resolution errors;
non-Boolean contracts and incompatible operators are type errors. Names and spans
remain available for source diagnostics; internal IDs are not printed.

This implements KD-012, KD-018, and KD-019 only for the documented subset. General type
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

For **each verified function in that proof family**, including uncalled functions, the checker proves
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

This work implements portions of KD-005, KD-006, KD-012, KD-013, KD-014, KD-015, KD-018, and KD-019,
subject to KD-017 and DP-008/DP-009. It does not reopen accepted language decisions
or freeze the broader language's syntax (OQ-018). Mixed assurance boundaries,
general mutation framing, and snapshot semantics beyond this one-field subset
remain design work under OQ-019/OQ-020. OQ-022 records remaining ordinary-function
and local-value questions; KED-004 does not settle them.

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
Value-flow tests inspect immutable canonical views, one shared source-ordered
function identity space, local IDs, forward/recursive calls, scope, exact signatures,
call/return types, explicit-return diagnostics, literal restrictions, and mixed-kind
rejection. Privileged metadata-renaming tests preserve function/local identity
without exposing mutable public storage. CLI tests demonstrate ordinary-only
acceptance with zero proof counts and explicit non-execution/non-proof wording.
CI runs the tests in debug and optimized builds and Clippy with warnings
treated as errors.

The failing examples are intentional:

- `battery_fail.klx`: invalid first-call precondition.
- `battery_sequence_fail.klx`: invalid second-call precondition after mutation.
- `battery_body_fail.klx`: uncalled body fails its postcondition.
