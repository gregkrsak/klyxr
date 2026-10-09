# Klyxr compiler prototype

This Rust implementation is an executable **prototype**, not a general-purpose
Klyxr compiler. It implements a restricted source → lexer → parser → AST →
name resolution → expression and value-flow type checking → typed HIR → core
ownership, loan, and borrowed-access checking → ordinary MIR CFG lowering or
specialized verified integer contract proof → diagnostics.

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
inputs report their type-checked function count and core Copy/move, whole-value loan, and Copy-safe borrowed-access check results,
and explicitly state that they
are not executed or verified; constrained results are not proven. In mixed inputs,
only verified mutation functions contribute to the proof counts.

Exit codes: `0` for frontend acceptance and any supported verified obligations established, `1` for source/semantic/
proof failure, `2` for command usage or file-reading errors.

## Supported grammar

- Distinct named ranges: `type Percent = range 0..100;`. Both endpoints are
  inclusive. The prototype uses signed i64 values, including negative literals
  and the full i64 bounds. This does not settle future range representation.
- Records with one or more named-range fields and complete named ordinary
  construction (KD-036). The specialized verified-state/harness path requires
  exactly one field.
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

## Ordinary value functions (KED-004 through KED-011)

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
built-in source `bool` (semantic `Bool`), a declared named range, or an existing
record by value, or an exact shared/exclusive reference to one of those types. The explicit
`-> Type` return declaration permits only owned Bool/range/record types; reference returns are rejected. `bool` is a reserved
built-in type token, so user range/record declarations cannot redefine it. Omitting `-> Type` explicitly declares NoValue under KD-034; it never infers a
result. Unit/void expression types, `()` values, generics, and inferred return
types remain unsupported.

The body contains `let name = expression;` locals, immutable by default, and
explicit **`return expression;`** statements. KD-033 permits return in ordinary
branches and while bodies and requires no structural closing-brace fallthrough.
For value-returning functions, its semicolon is mandatory. NoValue functions
instead permit `return;` or validated top-level closing-brace completion. Nested
branch/body closing braces remain local fallthrough. Klyxr deliberately
rejects implicit block tails, with or without a semicolon (KD-019). Missing value returns
are type errors; bare tails and statements after return receive explicit parser
diagnostics. No general expression statements or local annotations are implemented. KED-006 introduced `let mut`/`let mutable` for owned
locals eligible for exclusive borrowing; KED-011 adds Copy-safe direct local
reassignment, as described below. The existing
top-level mutable record harness is unchanged.

Parameters are visible at entry; a local enters scope after its initializer is
resolved. Later locals and the return can reference earlier locals. Self-reference,
use before declaration, parameter/local duplicates, shadowing, and cross-function
scope leakage are resolution errors. Initializers propagate their concrete Bool,
named range, record, or reference type. `let five = 5;` is a type error: the internal IntegerLiteral
category has no settled value-materialization rule. `let result = amount - 5;`
is valid because the existing subtraction rule computes a concrete range type.

Value-producing ordinary calls are expressions, including nested calls and operator operands.
NoValue ordinary calls are standalone named call statements; they cannot occur
in any expression position. Value-producing calls cannot be silently discarded
as statements. Both call forms use exact argument typing and canonical FunctionIds.
Complete signatures are collected before bodies, so forward and direct/indirect
recursive calls resolve. No termination or recursion policy is introduced. Calls
require exact arity and exact concrete argument types; their result has the declared
return type. Returns likewise require exact concrete types. Distinct ranges or
records cannot be passed or returned implicitly even with identical bounds or
field definitions. Direct integer literal leaves may form a signature-required range under KD-037;
Bool literals already type as Bool. General conversions remain open under OQ-021.

Ordinary functions are **frontend semantic work only**: parsed, resolved,
type-checked, ownership/loan-checked, and lowered to ordinary MIR, with no execution, code generation, range
proof, or runtime checks. For example, `return current - used;` can type as Percent without establishing
that every result meets Percent's bounds. The verifier ignores ordinary functions
as proof targets. Plain `fn` does not yet implement the full `safe` assurance model,
with only core moves and whole-value, straight-line/acyclic, non-escaping borrowing
implemented under KD-004, KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, KD-028, KD-029, KD-030, KD-031, KD-032, KD-033, and KD-034. Ordinary calls may target only ordinary functions; verified
contracts/body expressions cannot call functions, and top-level harness calls may
target only verified functions. These rejections leave OQ-019 unresolved.

## Core owned-value moves (KED-005)

```klyxr
type Percent = range 0..100;
record Ticket { value: Percent }

fn identity(ticket: Ticket) -> Ticket {
    return ticket;
}

fn forward(ticket: Ticket) -> Ticket {
    let next = identity(ticket);
    return next;
}
```

Bool and named ranges are **Copy**. Records are **Move**, even when every field
is Copy. This fixed compiler classification introduces no Copy trait, derivation,
clone operation, or user customization. No move silently copies or clones a record.

Owned ordinary parameters begin available. A value reference to a record consumes
its ParameterId/LocalId place through an initializer, by-value argument, or return.
An initializer is analyzed first, then its destination local becomes a fresh owner.
Returning a record transfers it; call results are fresh values that may be bound,
passed onward, or returned directly. Nested calls need no artificial locals.
Forward and recursive calls remain structurally valid; no termination is established.
Copy parameters and locals remain reusable through all these operations.

```klyxr
fn bad(ticket: Ticket) -> Ticket {
    let next = ticket;
    return ticket; // error: use of moved value `ticket`
}
```

The diagnostic points at the illegal use and gives the previous move's line/column
and destination. The checker stops at the first ownership error, avoiding cascades.
State is per function and keyed by typed place IDs, never names. Expression children
are checked in source order for deterministic diagnostics; this is not a runtime
evaluation-order specification. Unused owned parameters/locals are accepted.

This is whole-record ownership with acyclic branch joins and the documented stable-loop boundary. Ordinary records enter through
by-value parameters, ordinary call results, or KD-035 construction. Direct named-owner
Copy-field reads preserve whole-record ownership. Field mutation, destructuring, partial moves, Copy customization, Clone, destruction,
and runtime resource release remain unsupported. KED-006 adds mutable owned locals
and references within the restricted borrowing model below.
In particular, ordinary record equality and arithmetic remain type errors.
The existing verified mutable state and top-level record harness are excluded
from this ownership state machine and retain their existing behavior.

`examples/move_values.klx` demonstrates accepted Copy reuse and record transfers.
`examples/move_fail.klx` intentionally fails ownership analysis. Passing this phase
does not execute ordinary functions, verify their contracts/ranges, or implement
the complete future memory-safety model. OQ-023 retains Copy customization, cloning,
partial moves, and destruction. Advanced borrowing remains open under OQ-024.

## Core borrowing and reference semantics (KED-006)

```klyxr
fn inspect(ticket: &Ticket) -> bool { return true; }
fn exclusive(ticket: &mut Ticket) -> bool { return true; }

fn example(ticket: Ticket) -> Ticket {
    let view = &ticket;
    let copy = view; // Shared reference values are Copy.
    let first = inspect(view);
    let second = inspect(copy); // Last use of all derived shared handles.
    let mut owned = ticket; // Explicit owner transition; exclusive borrowing eligible.
    let direct = exclusive(&mut owned); // Call-held loan ends with this call.
    let access = &mut owned;
    let next = access; // Mutable reference value moves; access cannot be reused.
    let result = exclusive(next); // Loan ends after the receiving call.
    return owned;
}
```

`&T` is a shared, non-owning, Copy reference value. `&mut T` is an exclusive,
non-owning, Move reference value. Referents are only Bool, a canonical named range,
or a canonical record; the HIR `ReferentType` is nonrecursive. Exact capability and
nominal referent identities are required at calls: no implicit owned/reference
conversion, dereference, reborrowing, or `&mut T` → `&T` coercion occurs.
Reference return declarations and nested reference types fail before ownership.
No call result can therefore carry an escaping reference.

Borrow expressions are exactly `&name`, `&mut name`, or `&mutable name`, where
name resolves to an existing owned ordinary ParameterId or LocalId. Calls,
parenthesized targets, literals, fields, and existing reference values cannot be
borrowed. `let mut name = expression;` and `let mutable name = expression;` mark
an owned local as eligible for exclusive borrowing. KED-006 itself introduced no
reassignment; KED-011 now permits the narrow Copy-safe form described below.
Reference-valued locals remain immutable bindings even for `&mut T`; marking
such a binding `let mut` is a type error. Ordinary owned parameters remain
immutable places; mutable owned parameter syntax is not implemented.

Availability and active loans are separate private state in `compiler/ownership`.
A borrow never consumes or clones its owner. Shared loans may overlap; exclusive
loans conflict with every active loan and with direct owner access. Non-Copy owners
cannot move while any loan remains active. Copy owners may still be read under
shared loans, but not under an exclusive loan. Borrowing a moved owner is rejected.

Stored references carry private loan provenance. Shared copies carry the same
loan; mutable transfers move their handle while retaining that loan. The checker
counts continuation-aware future place uses by ID, and keeps a loan while any
derived handle may remain available and future-used on that path. A local with no future use does not keep its loan
alive past the binding statement. Moved reference handles remain moved even after
their underlying loan ends. No lexical-block lifetime or runtime destruction is
implied by loan expiry.

Reference arguments, including stored handles at their final use, are held through
the complete receiving call. Later arguments cannot move or exclusively borrow
an owner already call-held. Nested calls release only their own holds; an outer
call's earlier reference arguments remain held. A completed inner call can release
its loan before the next outer argument. Sequential direct `&mut` calls are valid;
reusing a stored mutable reference after passing it by value is use-after-move.
Reference parameters have external provenance; the callee checks their Copy/Move
behavior without reconstructing caller owners or executing/inlining callees.

Loan legality failures use `FrontendError::Ownership`, with a useful conflict
span and earlier borrow/move location. Type incompatibilities remain Type errors.
The first substantive ownership error stops analysis. Loan IDs/state remain private
and are not attached to public HIR or printed in diagnostics.

`examples/borrow_values.klx` demonstrates valid borrowing and owner recovery;
`examples/borrow_fail.klx` intentionally moves an owner before a later shared use.
Ordinary borrowing does not execute or prove functions. The verified battery's
existing special state-reference semantics and harness remain unchanged and are
excluded from ordinary loan analysis. KD-021 settles this core; OQ-024 retains
reference returns, lifetimes, reborrowing, auto-dereference, fields, partial borrowing,
advanced reference mutation, temporaries, general control flow, and advanced borrowing.
KED-007 adds only the explicit Copy-safe access described below.

## Explicit dereference and Copy-safe mutation (KED-007)

```klyxr
fn read(value: &Percent) -> Percent {
    return *value;
}

fn update(value: &mut Percent, replacement: Percent) -> Percent {
    let before = *value;
    *value = replacement;
    *value = *value;
    return *value;
}
```

`*reference` is explicit dereference of a directly named ordinary reference
parameter or local. The referent type preserves the exact canonical Bool, range,
or record identity. Arbitrary-expression operands (`*(reference)`, `*(&value)`,
`*make(...)`), nested dereference, multiplication, and field projection remain
unsupported. Parentheses may group a completed dereference expression, as with
other existing expressions; they do not generalize its operand.

A Copy referent (Bool or named range) can be read through either shared or mutable
references. The referent is copied; the handle is used, not moved, and no loan or
reborrow is created. Repeated mutable-reference reads are therefore valid.
Transferring that handle through a binding or call still moves it under KED-006;
subsequent reads/writes report the existing use-after-move diagnostic.
A record dereference types as that record, but ownership rejects every by-value
materialization: local initialization, argument, or return. No implicit clone or
move out of borrowed non-Copy storage occurs.

`*reference = expression;` is a dedicated ordinary statement with no value.
Typing requires an exclusive mutable reference and an exactly matching RHS
referent type. Shared writes, nominal mismatches, owned/reference mismatches,
are Type errors. KD-037 permits in-bounds direct literals for range writes. Bool literals already
materialize as Bool. Ownership rejects non-Copy replacement even with the correct
reference capability and RHS type, because displacement/destruction semantics
are unresolved. No old value is implicitly dropped, leaked, or relocated.

Reads and writes count as non-consuming handle uses in existing last-use analysis.
Copied shared handles and moved mutable handles retain their original provenance.
A final read permits loan expiry immediately after that access. A write keeps its
exclusive loan held throughout the RHS, including nested dereferences and calls,
and releases that hold only after the statement. Thus `*access = owned;` cannot
bypass exclusive-owner conflicts, even when the write is the final handle use.
The target handle must remain available throughout the statement; moving it in
an RHS call cannot leave a valid subsequent write. Repeated writes and
`*access = *access;` use one continuous loan without consuming the handle.
Normal last-use expiry then permits direct owner recovery.

Passing `*access` to a Copy parameter passes a copied referent, so it does not
call-hold the reference loan. Passing `access` to a reference parameter retains
KED-006 transfer and complete-call hold behavior. Deterministic analysis does not
settle the full future runtime assignment evaluation-order specification.

See `examples/deref_mutation.klx` and intentional failure
`examples/deref_mutation_fail.klx`. Ordinary access/mutation remains frontend-only:
not executed, interpreted, code-generated, contract-proven, or range-enforced at
runtime. The verified battery state/field `-=` machinery is unchanged and separate.
No direct local/reference-binding reassignment, assignment expressions, compound
assignment, field mutation, auto-deref, reborrowing, or destruction
is introduced by KED-007. KED-008 allows these existing Copy-safe writes inside branches. KD-022 settles this narrow core; OQ-025 retains general mutation
and replacement, OQ-023 destruction, and OQ-024 advanced reference behavior.

## Statement conditionals and ordinary MIR (KED-008)

```klyxr
fn update(flag: bool, value: Percent, replacement: Percent) -> Percent {
    let mut owned = value;
    if flag {
        let access = &mut owned;
        *access = replacement;
    } else {
        let observed = owned;
    }
    return owned;
}
```

`if condition { ... }` is a statement; `else { ... }` is optional. Braces are
required, parenthesized conditions use existing expression syntax, and nesting
is supported. Conditions must type exactly as Bool, including ordinary calls
returning Bool. No truthiness conversion is permitted. Conditional values are initializer-only under KED-010, described below.
Branches contain lets, existing dereference writes, direct Copy assignment, return, and nested if/while statements; within while they may also end with unlabeled continue or break.
Each branch is a lexical child scope: outer bindings remain visible, locals enter
only after their initializer, visible-name shadowing is rejected, and branch
locals cannot escape or cross into siblings. Siblings may reuse a spelling with
distinct canonical LocalIds. Returns may terminate branch paths under KD-033.
`else if` shorthand is rejected, as are
match and expression statements. Unlabeled continue/break are supported only inside while. Copy-safe local reassignment
is permitted under KED-011.

Ownership evaluates the condition once before splitting state. Both branches
start independently from that post-condition state. A non-Copy place visible at
the join is available only if every incoming branch retains it, including the
empty false path when else is absent. Otherwise later use or borrowing reports
that the value may have moved on a previous control-flow path and identifies a
move location. Mutable-reference handles follow this Move rule; shared handles
remain Copy. Both branches may separately consume the same incoming owner if
there is no subsequent joined use.

Branch-local reference handles/loans cannot escape and end no later than branch
exit. KED-009 refines borrowing with path-sensitive liveness, described below.
Call-held references and write-held loans retain their existing protection inside
branches. No generalized CFG borrow solver, reference-valued branch result, or
destruction is implied.

`mir::lower(&program)` lowers accepted ordinary HIR to `MirProgram`/`MirFunction`.
Canonical FunctionId, ParameterId, LocalId, and nominal type identities survive;
names remain metadata. Public function/block inspection is read-only, with
compilation-local BasicBlockIds. Blocks contain Let/DerefAssign statements using
existing typed expression trees, and exactly one Branch/Goto/Return terminator.
Simple if has an explicit false edge to the join; if/else joins only actual fallthrough
paths; nesting recursively builds real CFG edges. Empty else is normalized to
the false join edge. Every fallthrough is explicit, and every explicit source return
is a terminator. All-return conditionals have no synthetic join or final return. Allocation order is deterministic, without making block numbering
source semantics. `MirFunction::validate()` checks targets, entry, reachability,
Bool conditions, and hidden conditional values. Indexed tables give unique IDs; the block type
requires one terminator. KED-012 adds cycle-safe visited tracking; validation accepts
cycles and does not claim that Return is reached.

The CLI exercises MIR lowering before success. There is no MIR source-error phase,
execution, interpretation, code generation, or proof of ordinary logic. The verified
battery path remains HIR-only and excludes ordinary conditionals. MIR has no phi,
block parameters/results, SSA, temporary flattening, cleanup, backend, or
verification consumer. KD-023 settles this boundary; OQ-026 retains broader
conditional expressions, control flow, and dataflow. See `examples/if_control_flow.klx`
and intentional Ownership failure `examples/if_move_fail.klx`.

## Copy-safe direct local reassignment (KED-011)

```klyxr
fn next(value: Percent) -> Percent { return value; }
fn update(start: Percent) -> Percent {
    let mut current = start;
    current = next(current);
    return current;
}
```

`let mut` / `let mutable` owned Bool and named-range locals may be reassigned with
`local = expression;`, including inside existing statement branches. The target
must be a previously declared in-scope mutable local; it retains the same canonical
LocalId. Parameters and immutable locals are Type errors. The RHS requires exact
concrete nominal identity; equal range bounds do not permit assignment between
distinct declarations. KD-037 permits direct literals at established range targets.

The RHS evaluates first against the old value. Complete RHS calls retain their
reference holds, then existing last-use expiry runs before the write. The actual
write conflicts with any still-active shared or exclusive loan of the target.
For example, `current = *view;` may be accepted when that Copy dereference is the
handle's final possible use, while a later `*view` keeps the loan active and blocks
the write. Sibling-only uses do not pin a loan on the opposite assignment edge;
post-join uses propagate back into each relevant branch. Direct assignment creates
no loan, consumes nothing out of the Copy target, and leaves it available for
self-assignment, repeated writes, borrowing, and returning.

Reference-local reassignment is a Type error; provenance replacement remains
unresolved. Exact-typed record replacement is an Ownership error before consuming
either owner, because displacement/destruction semantics remain undefined. KED-011
adds no drop, clone, leak, swap, take, or replace semantics.

Assignment remains statement-only and directly named. No assignment expressions,
chained/compound forms, parameters, projected/field/index targets, auto-deref,
reference replacement, or non-Copy replacement are added. Its RHS uses the existing
ordinary expression grammar, so `current = if ...` remains rejected. KED-010
conditional initialization is separate: a mutable Copy destination may subsequently
be assigned, but its mutually exclusive MIR Let leaves remain initialization.
Existing explicit `*reference = expression;` retains KED-007 write holds and rules.

AST/HIR `ValueStatement` and MIR `Statement` publicly add `Assign`; exhaustive
library matches must handle the new variant. HIR/MIR retain a canonical LocalId,
typed RHS, and complete statement span. MIR emits a distinct Assign in the current
block, adds no blocks for straight-line writes, and remains deterministic. KED-012 adds cyclic control flow.
Validation rejects hidden IfValue nodes in assignment RHSs. `compile_source` and
`mir::lower` signatures remain unchanged. No generalized place/dataflow model,
general cyclic ownership/lifetime inference, SSA, phi/block parameters, result slots, definite assignment,
execution, or ordinary verification is introduced.

See `examples/local_reassignment.klx` and the intentional future-use loan conflict
`examples/local_reassignment_borrow_fail.klx`. KD-026 settles this boundary;
OQ-022/OQ-024/OQ-025/OQ-026 retain broader mutation and control-flow design.

## Conditional value initialization (KED-010)

```klyxr
fn choose(flag: bool, first: Percent, second: Percent) -> Percent {
    let chosen = if flag { first } else { second };
    return chosen;
}
```

This is immutable one-time initialization; `let mut` / `let mutable` also works
and retains the existing exclusive-borrow capability. `else` and braces are
mandatory. Each branch contains one expression, without statements or a trailing
semicolon. Nested conditionals may be complete branch results. The binding enters
scope only after its initializer resolves, so self-reference remains forbidden.

Conditions require Bool; branches require exactly the same concrete Bool, range,
or record identity. Identical bounds or structure do not make distinct nominal
types compatible. Integer literals gain no contextual range type. Reference-valued
results are rejected, while Copy dereference values retain KD-022 rules.

Copy sources remain available. Record sources move only on their selected path;
a source moved on any possible path is unavailable after the join. Selecting the
same record on both branches is valid, but later source use is not. The destination
becomes available after every path initializes it. Conditions run before branch
splitting for ownership analysis. KD-024 path-specific future uses, post-join loan
requirements, and existing call/write holds remain intact.

AST and typed HIR publicly add `ExprKind::IfValue`; exhaustive library matches
must handle the new variant. `compile_source` still returns canonical HIR and
`mir::lower` retains its signature. Typed HIR preserves typed children and canonical IDs.
MIR lowers it to Branch/Goto blocks: each reachable leaf initializes the same
source LocalId and reaches a common join. Nested value branches share that join.
No conditional value remains hidden in a MIR Let initializer. This is not direct
reassignment, SSA, a phi, a block parameter, or a general temporary/result slot.

The syntax is accepted only as a complete local initializer or complete nested
branch result. Conditional values in returns, call arguments, unary/binary operands,
parenthesized expressions, and dereference-write RHSs remain rejected. General
value blocks, reference joins, and general definite assignment
remain unresolved. Statement `if` retains its existing grammar and optional else.
Ordinary functions remain non-executable and unverified; the battery proof path
is unchanged. See `examples/conditional_value.klx` and the joined-move rejection
`examples/conditional_value_move_fail.klx`. KD-025 settles this narrow boundary;
OQ-022/OQ-024/OQ-026 retain broader conditional value and lifetime design.

## Path-sensitive acyclic loan liveness (KED-009)

```klyxr
fn update(flag: bool, value: Percent, replacement: Percent) -> Percent {
    let mut owned = value;
    let view = &owned;
    if flag {
        let observed = *view;
    } else {
        let access = &mut owned;
        *access = replacement;
    }
    return owned;
}
```

This was rejected by KED-008's whole-conditional loan hold and is now accepted.
**Lexical coexistence of reference bindings does not imply overlapping loans;
only control-flow paths with future uses keep a loan active.** The then path
needs the shared loan through `*view`. On the else edge, no use of `view` remains,
so that loan expires before the exclusive borrow. Last use within a branch can
likewise permit a later conflicting borrow on that same path.

The ownership checker decomposes the remaining statement suffix into condition,
then, else, and post-join future uses. A loan required on either successor remains
active through condition evaluation. Each outgoing path then receives its own
uses plus the common continuation, excluding sibling-only uses. No constant
condition is pruned: both edges are considered possible. Nested conditionals use
the same finite recursive analysis, without a generalized dataflow/fixed-point engine.

If the example instead ends with `return *view;`, the shared loan remains needed
on both incoming paths, and the exclusive borrow in else is an Ownership error.
Loans never expire and then reactivate along one path. Shared aliases extend
original provenance through their own last possible uses; mutable transfers
remain moves, including transfers to branch-local handles. Call arguments remain
held throughout their receiving call, and writes hold the exclusive loan through
the entire RHS.

Move joins are unchanged: availability requires every incoming path. At a join,
pre-existing loans active on any incoming path remain potentially active; loans
dead on all paths disappear. A conditionally moved handle remains unusable, but
its loan may still block owner access if a future handle use needs that loan on
a path where the handle was not moved. With no future reference use and loans
ended on every path, owner recovery is allowed. Branch-local handles/loans cannot
escape, and unrelated sibling-created provenance is never merged.

These rules belong to the existing ownership checker over typed HIR, before MIR.
`compile_source`, HIR, and MIR APIs are unchanged; newly accepted programs lower
to the same validated acyclic CFG forms. KD-024 settles only whole-value loans
across the structured acyclic subset. Cyclic loan analysis, arbitrary CFG lifetime inference,
reborrowing, reference returns, lifetime syntax, field/partial borrowing, reference-valued conditional
results, destruction, execution, and code generation remain unsupported. KD-033 adds structured early returns.
See `examples/path_sensitive_borrowing.klx` and the intentional post-join-loan
failure `examples/path_sensitive_borrowing_fail.klx`. OQ-024/OQ-026 retain the
broader lifetime and control-flow questions.

## Core expressions and types (KED-003)

Primary expressions are `true`, `false`, signed i64 literals, parameters,
`state.field`, `old(state.field)`, and parenthesized expressions. Ordinary functions additionally support locals and
ordinary calls, direct whole-place borrow expressions, and named dereference. A record parameter
by itself is not a numeric expression. `old(...)` is allowed only in `ensures`
and only for the current mutable state field. Parentheses around that exact field
are permitted; arbitrary snapshots, nested objects, and aliases are not.

Only these operators are implemented, in order from highest to lowest precedence:

| Form | Associativity |
|---|---|
| Primary / calls / `old(...)` / named `*reference` | Grouping |
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

Every HIR expression has an `ExprType`: `Bool`, `Range(RangeTypeId)`,
`Record(RecordId)`, `SharedRef(ReferentType)`, `MutableRef(ReferentType)`, or `IntegerLiteral`. The last is an internal constant category, not a public numeric
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
`types::check(resolved)`, then `ownership::check(&program)`, returning accepted
canonical `hir::Program` or `FrontendError` (`Lex`, `Parse`, `Resolve`, `Type`,
or `Ownership`). Compilation does not prove numerical
obligations. `resolve` now returns an opaque `ResolvedProgram`, consumed by
`types::check`; it no longer publishes typed HIR by itself. The resolver identifies
names, fields, callees, borrow targets, and source-ordered local scope/mutability. It resolves type names to
canonical type references. `compiler/types` validates concrete declared value types,
operator legality, nominal compatibility, dereference referent types, mutable write
capability and exact RHS types, result types, Boolean contracts,
subtract-assignment, initializer propagation, call arguments, and explicit returns.
The type layer performs no textual source-name lookup. `types::check` remains a
separate type-only API and may return typed HIR that fails ownership checking.
Use `compile_source` for the complete frontend, or call `ownership::check(&program)`
after typing. Ownership inspects that HIR without duplicating or rebuilding it;
its classification, availability, provenance, future-use counts, call holds, and
loan/move sites and borrowed-access legality remain private implementation data. No separate borrowing pass is introduced.

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
range/record IDs. AST `FunctionDecl` and HIR `Function` each distinguish Ordinary and
Verified forms in one source-ordered function table. `functions()` and `function(id)`
now return the common HIR `Function`, with `id()`, `name()`, `as_ordinary()`, and
`as_verified()` read-only views. This is an intentional prototype API change.
Verified functions retain one `requires` and one `ensures` expression; ordinary
functions carry parameter IDs, a `ValueType` return, typed lets and a typed explicit
return. `ValueType` permits Bool/Range/Record and SharedRef/MutableRef, excluding IntegerLiteral.
Borrow HIR carries BorrowKind and a canonical `Place::Parameter` or `Place::Local`;
reference types retain their canonical non-reference referent IDs.
`ExprKind::Deref { reference: Place }` identifies the handle and carries the
referent ExprType. `ValueStatement::DerefAssign { reference, value, span }`
is a dedicated typed write statement, not a generalized assignment/place system. Ordinary AST
signature types now use `ast::ValueType` with a textual name and flat reference
prefixes, rather than a single String. Illegal nested source prefixes can therefore
receive a Type-phase diagnostic without introducing recursive semantic reference types. Ordinary
parameters use `ParameterType::Value(ValueType)`; verified parameter types are
unchanged. Locals carry their owning FunctionId, concrete type, and owned mutability marker; BindingId
continues to denote only top-level record bindings. Parentheses
affect grouping and spans without introducing an extra HIR node.
Construction and calls carry record/field/function/binding IDs. Names and source
spans remain metadata for diagnostics. These IDs have no cross-compilation,
serialization, or ABI stability guarantee.

`verify::verify_report(&hir)` (and the diagnostics-only `verify`) consume canonical
HIR accepted by the frontend. Ownership is a separate prerequisite; the verifier
does not discover moves or reinterpret ordinary functions. Verification uses direct ID-based
table access and tracks current state by `BindingId`; names are used only for diagnostics and the
human-readable `final_values` report. It retains numerical initialization/argument
checks, record-ID compatibility and the existing mutable-binding check, universal
body proof, and ordered modular call-state reasoning.

`hir::Program` owns the canonical tables for its compilation-local IDs. Public
inspection is read-only: `ranges()`, `records()`, `fields()`, `functions()`,
`parameters()`, `locals()`, `bindings()`, and `statements()` return immutable slices, while
`range(id)`, `record(id)`, `field(id)`, `function(id)`, `parameter(id)`,
`local(id)`, and `binding(id)` provide immutable ID-based lookup. External consumers
cannot mutate or reorder the underlying tables. Storage is crate-private so the resolver can
construct HIR directly; compiler-internal transformations are responsible for
preserving its identity and reference invariants.

The CLI follows this exact path:

```text
source → lexer → parser → AST expressions/functions → resolver → expression and value-flow type checking
       → typed HIR → core ownership, loan, and borrowed-access checking
       ├─ ordinary functions → MIR CFG lowering → diagnostics
       └─ verified functions → specialized prototype verifier → diagnostics
```

Resolution, type, and ownership errors prevent proof. Diagnostics distinguish malformed syntax,
unknown references, incompatible expression types, unsupported well-typed proof
shapes, and numerical counterexamples. Missing names are resolution errors;
non-Boolean contracts and incompatible operators are type errors; well-typed
use-after-move, loan conflicts, non-Copy borrowed materialization, and non-Copy
replacement are ownership errors. Non-reference dereference, shared writes, and
RHS mismatches are Type errors. Names and spans
remain available for source diagnostics; internal IDs are not printed.

This implements KD-012, KD-018, KD-019, KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, KD-028, KD-029, KD-030, KD-031, KD-032, KD-033, and KD-034 only for the documented subset.
General type inference, advanced borrowing/ownership, effects, MIR backend/verification lowering, VIR,
and broader HIR features remain future work.

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

The prototype does not implement ownership beyond core ordinary moves and
whole-value acyclic loans, reference returns, explicit lifetimes, reborrowing,
auto-dereference, non-Copy replacement, field/reference mutation beyond Copy-safe
whole-value writes, partial borrowing, general replacement, destruction,
effects, runtime contracts, expressions beyond this subset, general cyclic ownership/loans, quantifiers,
multi-field verified/harness state, record invariants, SMT/VIR or MIR backend/verification lowering, or machine-code generation. In particular,
`examples/effects.klx` is illustrative and is rejected rather than analyzed.
A successful prototype result must not be described as establishing those
unimplemented properties or unspecified program correctness.

KD-036 implements ordinary records with one or more named-range fields and complete
named construction; specialized verified and harness state still requires exactly one field.

This work implements portions of KD-005, KD-006, KD-012, KD-013, KD-014, KD-015, KD-018, KD-019, KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, KD-028, KD-029, KD-030, KD-031, KD-032, KD-033, and KD-034,
subject to KD-017 and DP-008/DP-009. It does not reopen accepted language decisions
or freeze the broader language's syntax (OQ-018). Mixed assurance boundaries,
general mutation framing, and snapshot semantics beyond this one-field subset
remain design work under OQ-019/OQ-020. OQ-022 records remaining ordinary-function
and local-value questions; KD-020 now settles core moves, while OQ-023 retains
Copy customization, partial moves, cloning, and destruction. KD-021 settles only
core borrowing, and KD-022 explicit Copy-safe access; OQ-024/OQ-025 retain advanced
reference/lifetime and general mutation/replacement questions.

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
Ownership regressions cover record signatures/locals/results, nominal record
mismatches, binding/call/return transfers, Copy reuse, nested and repeated consuming
arguments, forward/recursive calls, unused owners, function isolation, first-error
reporting, phase ordering, retained restrictions, and privileged metadata renaming.
CLI tests distinguish successful core ownership checking from execution/proof and
check the later-use and prior-move locations on failure.
Borrowing regressions cover exact reference capability/referent typing, direct
owned-place resolution, reference Copy/Move, immutable/exclusive eligibility,
shared/exclusive conflicts, Copy owner reads, unused handles, last use of shared
aliases and mutable transfers, final-use call holds, nested-call release boundaries,
external reference parameters, aliases, phase ordering, and retained non-goals.
Privileged tests rename all diagnostic metadata while preserving borrow targets,
nominal referents, provenance, conflicts, and last-use behavior. CLI tests cover
accepted borrowing with zero proof counts and spanned intentional borrow conflicts.
Dereference/mutation tests cover exact canonical target/referent types, named
operand restrictions, repeated non-consuming reads/writes, record move-out and
replacement rejection, phase boundaries, alias/transfer provenance, future
read/write liveness, final-use recovery, whole-write RHS holds, nested calls,
Copy-argument versus reference-argument holds, moved RHS target handles, and
metadata-independent typing/ownership. CLI tests distinguish frontend acceptance
from execution/proof and explain the intentional non-Copy replacement boundary.
CI runs the tests in debug and optimized builds and Clippy with warnings
treated as errors.

The failing examples are intentional:

- `battery_fail.klx`: invalid first-call precondition.
- `battery_sequence_fail.klx`: invalid second-call precondition after mutation.
- `battery_body_fail.klx`: uncalled body fails its postcondition.
- `move_fail.klx`: returning a record parameter after moving it into a local.
- `borrow_fail.klx`: moving an owner while a later reference use keeps its shared loan live.
- `deref_mutation_fail.klx`: replacing a non-Copy record before displacement/destruction semantics exist.


## Statement while and cyclic MIR (KED-012)

```klyxr
fn update(flag: bool, value: Percent, replacement: Percent) -> Percent {
    let mut current = value;
    let mut running = flag;
    while running {
        current = replacement;
        current = current;
        running = false;
    }
    return current;
}
```

This is a pre-test, zero-or-more-iteration statement with an exact Bool condition.
Braces are mandatory, without a trailing semicolon. Nested while and statement
if/else compose; bodies are lexical child scopes with ordered declarations, no
visible-name shadowing, and static canonical LocalIds. Body locals cannot escape.
KD-033 permits loop-local return, with structural completeness still requiring
the while condition-false path to reach an explicit return. While expressions,
for/unconditional loop and labels remain unsupported. KD-031/KD-032 add unlabeled statement continue/break.

KED-012 initially allowed only owned Copy Bool/named-range activity in conditions and bodies:
Copy calls/reads/locals, direct mutable-local reassignment, and KED-010 Copy
conditional initialization. All reference activity (including shared Copy handles,
borrow creation, arguments, aliases, dereference and write-through) and non-Copy
record activity (including fresh call results) was rejected by ownership with an
explanation that cyclic ownership/lifetime analysis is future work. Existing
Type restrictions can reject malformed operations earlier. KED-013 permits
iteration-local Move activity in bodies as described below; conditions and
pre-existing-handle and condition restrictions remain. KED-014 permits new body
borrowing as described below. Constant false does not bypass checks.

The HIR ownership checker analyzes one permitted iteration and requires equal
outer Move availability, handle provenance, active loans and loan allocation state
at the header/backedge after body-local state is scoped away. Nested loops use the
same invariant. Untouched outside records and references remain valid. Outside
loans with post-loop handle uses remain live and constrain Copy owner reads/writes
inside the loop; last pre-loop use can end a loan before entry. Dead loans never
resurrect. Finite future-use counts are only finite traversal/continuation bookkeeping;
KD-030 retains recurrent loans separately, never by counting syntax once.

MIR lowers through existing terminators: preheader Goto header; header Branch Bool
to body or exit; completed body Goto header; exit continues to the final Return.
Nested statement/value joins compose with this real backedge. Let initializes,
Assign mutates the same source LocalId; no temporary, phi, SSA or block parameter
is synthesized. Lowering is deterministic. Validation accepts reachable cycles
with valid targets and Bool conditions; it still rejects hidden IfValue nodes.
A self-cycle without Return is structurally valid. Both edges remain for true/false
constants, and no termination, execution or ordinary verification is claimed.

Ownership stays above MIR with no fixed-point solver or second engine. Reference
activity/non-Copy transfer across backedges, full MIR dataflow, destruction,
verified loops/invariants/variants, and termination checking remain unresolved
under OQ-024/OQ-026. The battery verifier and 1,800-case affine cross-check are
unchanged. See `examples/while_copy.klx`, `examples/while_nested.klx`, and intentional
failures `examples/while_reference_fail.klx` / `examples/while_move_fail.klx`.


## Iteration-local Move ownership (KED-013)

A fresh non-Copy call result may now be bound and transferred inside one while
iteration using existing Move rules:

```klyxr
while running {
    let first = make_ticket();
    let second = identity(first);
    let done = consume(second);
    running = false;
}
```

Calls remain expressions, so Copy results use existing let/assignment statements;
no general expression-statement syntax is added. The example helpers have ordinary
owned signatures. KED-013 originally had no ordinary record construction; its historical
frontend fixture `examples/while_iteration_move.klx` uses a recursive record-returning
factory. This demonstrates typing/ownership/MIR structure, not executable value
creation or termination.

Every loop records the canonical Move places present at its own header. Transfers
of those pre-existing owners reject before changing state and explain that they
require future generalized cyclic ownership analysis. Fresh body-local owners are
outside that set; their Available/Moved/ConditionalMove states evolve normally.
Use-after-move and all-path branch availability still apply. Existing owned
conditional initialization may select iteration-local records. Unused local owners
retain existing scope-exit behavior; lexical state removal implies no runtime Drop.

Nested loops establish independent boundaries: a record made in an outer iteration
before an inner loop cannot move within the inner loop, but may remain untouched
there and be consumed afterward in the outer iteration. Body-local state is removed
before comparing header/backedge state. Static LocalIds remain unchanged across
iterations; names/spans are diagnostic metadata. Outside ownership and loans stay
protected. KED-014 subsequently permits iteration-local body borrowing;
KD-030 subsequently permits stable carried access; non-Copy replacement remains
forbidden.
Conditions retain KED-012's Copy-only restriction, including nested call arguments.

AST/parser/resolver/type/HIR/MIR representations and public APIs are unchanged.
Ownership remains the existing HIR-stage, one-iteration stability check without
fixed points, general loop-carried Move joins or MIR ownership. MIR uses unchanged
Let/call trees and Branch/Goto backedges, with no new temporaries or terminators.
No execution, verification, destruction or termination claim is made.

See `examples/while_iteration_move.klx` and intentional failures
`examples/while_carried_move_fail.klx` / `examples/while_nested_carried_move_fail.klx`.
KD-028 settles this refinement; OQ-022/OQ-023/OQ-024/OQ-025/OQ-026 retain broader
control flow, destruction/replacement, and cyclic ownership/lifetime questions.


## Iteration-local borrowing (KED-014)

```klyxr
fn update(flag: bool, value: Percent, replacement: Percent) -> Percent {
    let mut owned = value;
    let mut running = flag;
    while running {
        let view = &owned;
        let observed = *view;
        let access = &mut owned;
        *access = replacement;
        running = false;
    }
    return owned;
}
```

New body references may borrow both pre-existing owners and iteration-local Move
owners. Existing shared copies, mutable-handle moves, call/write holds, dereference,
Copy-safe mutation and path-sensitive acyclic last-use behavior apply. Local records
may move after their final borrow use. Conditions remain reference-free. Handles
already present at the current loop header could persist untouched under KED-014
but could not be used there; KED-015 permits stable body access as described below. Their active loans still constrain new borrows
and owner access; dead loans never resurrect. An outer-created handle is pre-existing
at an inner header; KED-015 permits its stable inner use, while outer-local
provenance must still die before the outer backedge. No constant-condition pruning bypasses these rules.

Lexical cleanup removes body-local handles/availability and expires loans. The
checker proves every new loan/provenance is gone before restoring the header loan
allocator, then compares remaining semantic state to the header. Safe transient
ID reuse cannot conceal surviving loans. Names/spans and finite continuation counts
are excluded from semantic equality. This remains one cloned-iteration check above
MIR, with unchanged deterministic Branch/Goto backedges, no second engine or cyclic
fixed-point inference. Ordinary functions remain non-executable and unverified.

See `examples/while_iteration_borrow.klx`, `examples/while_iteration_mut_borrow.klx`,
and `examples/while_iteration_move_borrow.klx`. Intentional failures
`examples/while_carried_reference_fail.klx` and
`examples/while_nested_reference_fail.klx` demonstrate relative header boundaries.
KD-029 settles this subset; KD-030 adds stable carried access. OQ-024/OQ-026 retain
generalized cyclic lifetimes. Reference returns/results/reassignment,
reborrowing, implicit coercions, partial loans, non-Copy dereference/replacement,
destruction and execution remain unsupported. The specialized battery verifier
and independent 1,800-case mathematical cross-check are unchanged.


## Stable loop-carried references (KED-015)

```klyxr
fn update(flag: bool, replacement: bool) -> bool {
    let mut running = flag;
    let mut owned = false;
    let access = &mut owned;
    while running {
        *access = replacement;
        running = false;
    }
    return owned;
}
```

There are three categories: straight-line references follow ordinary last-use
liveness; fresh iteration-local references must be discharged before that loop's
backedge; stable header-carried references may recur only with unchanged handle
availability and original loan identity/owner/kind. Shared handles remain Copy,
including local aliases and ordinary reference call arguments. Mutable handles
support existing dereference/Copy-safe writes but cannot be moved to locals or
calls. No implicit reborrow is inserted.

If any checked body path uses a carried handle, its loan remains continuously
active through every accepted backedge, including paths that skip that use.
Thus reading a shared handle and then assigning its owner inside the loop rejects,
even after the body's final syntactic use or after setting the loop flag false.
Finite FutureUses cannot represent recurrence: separate scoped recurrent-loan
obligations retain provenance alongside ordinary liveness and call/write holds.
Branches inherit the frames; inner-loop exit cannot release an outer obligation.
An outer-local reference may cross inner backedges but must die before the outer
backedge. No loan expires and is reconstructed to repair carried state.

Backedge cleanup/validation precede obligation discharge. False exit removes only
the current frame, then uses ordinary post-loop liveness. If nothing outside or
in an enclosing loop needs a loan, it may end and its owner may be used again.
If a post-loop handle use remains, the loan continues protecting its owner.
Reference names remaining in scope do not by themselves imply active loans.
Failed operations restore counts, holds, availability and provenance atomically.

See `examples/while_carried_shared.klx`, `examples/while_carried_mutable.klx`,
`examples/while_carried_nested.klx`, and intentional conflict
`examples/while_recurrent_borrow_fail.klx`. Older carried-reference failure examples
now isolate mutable-handle transfer, which remains forbidden.
KD-030 adds no syntax, generalized cyclic NLL/fixed-point solver, second checker,
MIR lifetime analysis, reference conditions/reassignment/returns/results, reborrowing,
coercions, non-Copy replacement, destruction, execution or termination proof.
The existing cyclic MIR and specialized battery proof path are unchanged.


## Structured continue (KED-016 / KD-031)

An unlabeled `continue;` is an ordinary statement inside while or its nested
statement branches. It targets the innermost while header. The semicolon is
mandatory; values, labels, expressions, and verified loops remain unsupported. KD-032 separately adds break below.
Direct continue must end its lexical block. A conditional whose arms both continue
also terminates that block; any following statement is rejected structurally.
A loop itself still has its real condition-false exit and may have a suffix.

```klyxr
fn update(flag: bool, skip: bool) -> bool {
    let mut value = false;
    while flag {
        let view = &value;
        if skip {
            value = true;
            continue;
        }
        let observed = *view;
    }
    return value;
}
```

Here the iteration-local shared loan ends on the skip path **before** the
assignment: the later `*view` is not reachable on that path. Move `view`'s
creation before while and its recurrent obligation instead protects `value`
across every continue, even paths skipping the read. A post-loop reference use
independently protects its owner through the loop and continue paths. Lexical
existence alone does not keep a loan alive. Finite future uses, recurrent LoanId
obligations, and explicit call/write holds are independent retention reasons.

Every continue is validated against its own loop header, just like the normal
bottom backedge: carried Move availability and exact reference/loan provenance
must be unchanged, holds must balance, and fresh iteration-local provenance must
be gone before allocator reset. Current and enclosing recurrent frames stay active
through validation and are never discharged by continue. Failure restores complete
checker state. With one conditional arm continuing, only the actual fallthrough
arm contributes its effects to the suffix; with both continuing there is no join.
Iteration-local Move values may be scoped away without runtime destruction.

When all body paths continue, the separately checked header false edge still
provides the loop exit. False-exit processing removes the current recurrent
frame (KD-032 also discharges it on independently checked break exits); enclosing frames and post-loop finite uses remain authoritative. Constant
conditions and apparently one-shot loops receive the same checks. Nested continue
targets the inner header; after inner false exit, outer continue targets the outer
header. MIR uses the existing Goto and optional live tails; no unreachable join or
suffix is emitted and structural validation is unchanged.

See `examples/while_continue.klx`, `examples/while_continue_nested.klx`, and
`examples/while_continue_recurrent_fail.klx`. Ordinary code is neither executed
nor proven. This introduces no reborrowing, generalized fixed-point/CFG ownership,
second checker, MIR loan pass, labels, loop values, general unreachable-code
policy, destruction, or termination proof. The verified battery path and its
1,800-case affine numerical cross-check are unchanged.


## Structured break (KED-017 / KD-032)

An unlabeled `break;` exits the innermost ordinary while. It has no value and must
end its lexical statement block. It may appear inside nested statement if/else.
Both-break and mixed break/continue arms have no lexical fallthrough; a suffix in
that block is rejected. The while itself always retains its real condition-false
exit, including constant conditions and all-breaking bodies.

```klyxr
fn update(flag: bool, stop: bool) -> bool {
    let mut value = false;
    while flag {
        let view = &value;
        if stop {
            value = true;
            break;
        }
        let observed = *view;
    }
    return value;
}
```

Here the iteration-local loan's only future use is skipped on the break path,
so it can end before the assignment. Moving `view`'s creation before while makes
its checked body use recurrent: the assignment must then reject even though that
path will break. Removing all body uses is a positive control; a post-loop use
still independently keeps the loan active through the assignment. Recurrence is
never discharged early merely because a later break is known.

Ordinary bottom backedges and explicit continue backedges preserve exact header
state after local cleanup, with recurrent frames active. Ordinary condition-false
exit instead derives the canonical exit before any break candidates are checked.
Each explicit break exit must normalize its actual candidate to that same saved
state, without resetting carried availability/provenance or merging arbitrary exits.
The integrity gate precedes reductions that could hide corruption: target/outer
frame membership and nesting, original carried handle/LoanId/owner/kind/activity,
and balanced call/write holds are checked first. Captured outside finite uses are
installed before expiry; target-local state is scoped away; exactly one target
frame is discharged, even if empty. Outer obligations persist. Fresh provenance
must be absent before allocator reuse; normalized candidates then compare exactly
to canonical exit. Failure restores the actual entering candidate and full target
context rather than repairing a fault.

Post-loop-needed carried references retain original continuously active provenance
through break. Recurrence-only loans may end at exit, with no resurrection. Inner
break cannot remove an outer frame or retarget outer continue/break. Pre-existing
non-Copy Move restrictions remain unchanged, including terminal break paths.
Lexical cleanup and loan expiry introduce no destruction or resource release.

MIR reuses Goto to the innermost exit, while continue targets its header. Optional
live tails avoid synthetic suffix joins for all-terminal arms. Canonical identities,
structural validator and specialized battery proof path are unchanged. See
`examples/while_break.klx`, `examples/while_break_nested.klx`, and
`examples/while_break_recurrent_fail.klx`. Ordinary code remains non-executable
and unverified. Labels, break/loop values, generalized fixed-point ownership,
MIR borrowing, reborrowing, arbitrary unreachable-code analysis, destruction,
and termination proof remain open under OQ-022/OQ-024/OQ-025/OQ-026.


## Structured early return (KED-018 / KD-033)

`return expression;` may terminate any ordinary statement path: the function
block, a branch, or a while body. Returns end their lexical block; a direct
return or recursively all-terminal conditional cannot have a statement suffix.
An ordinary function must have no structural fallthrough to its closing brace
once targeted break/continue edges are consumed. Both-return branches are
complete. Every while retains its condition-false path, including `while true`;
this is structural completeness, not a termination or reachability proof.
Every return operand is checked for the exact declared type in the existing
Type phase before ownership. Public manually constructed ASTs receive the same
structural checks and diagnostics as parsed source.

```klyxr
fn choose(flag: bool, value: Percent) -> Percent {
    if flag {
        return value;
    } else {
        return value;
    }
}
```

Before `return E;`, finite liveness is exactly `finite_uses(E)`. Bypassed lexical,
loop-outside, enclosing-loop and function suffixes do not keep finite loans live
on that path. Operand uses still do. All current and enclosing recurrent LoanId
obligations remain active throughout operand evaluation, including nested calls
and arguments; return does not discharge them early. A returned path never
contributes ownership state to a surviving branch, loop false exit, or suffix.
Non-returning paths retain their actual effects and finite requirements.

Inside the actual return-expression tree only, terminal permission bypasses the
pre-existing owned non-reference Move restriction, allowing a carried record to
be returned directly or consumed by a nested by-value call. It does not authorize
preceding lets/calls/conditional initializers retroactively. Availability, active
loans, shared copies, mutable-reference transfers, provenance, dereference rules,
non-Copy replacement restrictions, and call/write holds are unchanged. A
`Transfer::Return` tag used to analyze a condition does not grant this permission.

Ownership installs the operand-only finite summary and runs ordinary expiry,
then snapshots the return-entry state before enabling terminal permission.
Complete operand evaluation and all fallible final validation, including balanced
holds, occur in that transaction. Failure restores that exact entry, including
finite counts, provenance, recurrence, allocator, targets and permission; it does
not restore the superseded suffix summary. Permission is restored on success
as well. Success has no canonical local successor: no header/false-exit
normalization, allocator reset, recurrence discharge or function-exit join occurs.

MIR lowers the whole ordinary body structurally, using existing Return/Goto/Branch
terminators. Return/break/continue have distinct targets, and only actual
fallthrough paths receive joins. No synthetic final return is emitted. MIR
validator semantics are unchanged; this adds no bottom type, general unreachable
analysis, generalized CFG/fixed-point ownership, second checker, SSA/phi nodes,
reference returns, reborrowing, destruction, execution, or verification of ordinary
logic. The specialized verified battery path and its 1,800-case affine cross-check
are unchanged. See `examples/early_return.klx`, `examples/early_return_move.klx`,
and the intentional failure `examples/early_return_recurrent_fail.klx`.


## No-value ordinary functions and call statements (KED-019 / KD-034)

```klyxr
fn reset(flag: &mut bool) {
    *flag = false;
}

fn reset_if_needed(flag: &mut bool, skip: bool) {
    if skip {
        return;
    }
    reset(flag);
}

fn prepare(flag: &mut bool) -> bool {
    reset_if_needed(flag, false);
    return true;
}
```

See `examples/no_value_functions.klx`, `examples/no_value_call_fail.klx`, and
`examples/no_value_recurrent_fail.klx`. NoValue is function-result metadata,
never an expression type. AST/resolution/HIR use `Option<ValueType>`:
`None` explicitly means NoValue and `Some(T)` means Value(T). No inference,
unit, void, dummy expression, implicit value return, or general expression
statement is introduced. Signatures are collected before bodies, including
forward and recursive calls. A NoValue call structurally falls through even
when recursive; this makes no claim about runtime termination.

Typing rejects wrong return forms and wrong call positions before ownership,
including manually constructed public ASTs. Bare return is legal only in
NoValue functions; value return only in Value(T) functions, with exact T.
Only the actual top-level NoValue boundary permits normal completion. Different
terminal ownership states do not join at a synthetic function exit. Surviving
branch suffixes retain the all-path Move rule. Loop false exits, break/continue
targets, recurrence, and KD-033 value-return semantics remain unchanged.

K19-01 gives each statement call a complete prepared-entry ownership transaction:
all ordered arguments, receiving/nested call holds, hold release, expiry, and
fallible completion validation. Failure restores availability, reference mappings,
loans/provenance/holds, finite uses, allocator, recurrence, loop context, and
terminal-permission context. Nested expression calls release only their own holds.
Statement-call arguments participate in both finite-use and recurrent-reference
traversal, including nested calls and loops. Statement calls never inherit
KD-033 terminal-expression Move permission, even immediately before `return;`.

Bare return installs an empty finite continuation and performs ordinary expiry
before taking its transaction snapshot. All current/enclosing recurrent obligations
and operation holds remain authoritative. Fallible completion validation is inside
that transaction; failure restores that prepared snapshot, not a broader skipped
suffix. Bare return has no operand or terminal-expression Move permission.
Successful bare return and normal NoValue completion require balanced holds and
perform no ownership repair, frame discharge, allocator reset, or cleanup.

Typed HIR has distinct `CallNoValue` and `ReturnNoValue` statement forms. MIR
retains canonical function IDs, typed arguments and spans in `CallNoValue`, and
uses an honest `ReturnNoValue` terminator for bare return or a live top-level
tail. Already-terminal branches create no unreachable completion block; nested
braces create no function completion. MIR validation recursively inspects every
statement-call argument for unlowered conditional values. No-value completion
creates no result local or synthetic value. Ownership remains above MIR in the
existing checker, with no second checker or generalized CFG/fixed-point engine.

Ordinary functions remain checked and lowered, not executed, code-generated or
proven. Consuming an argument establishes static transfer, not destruction, Drop,
RAII, runtime cleanup or resource-release ordering. The specialized verified
function and top-level battery harness paths are unchanged.


## Ordinary record construction and Copy-field reads (KED-020 / KD-035)

```klyxr
type Percent = range 0..100;
record Battery { charge: Percent }

fn make_battery(charge: Percent) -> Battery {
    return Battery { charge: charge };
}

fn inspect_and_forward(battery: Battery) -> Battery {
    let observed = battery.charge;
    return battery;
}
```

See `examples/ordinary_records.klx`, `examples/ordinary_record_move_fail.klx`
and `examples/ordinary_record_condition_fail.klx`. KED-020 originally permitted exactly one named-range field; KD-036 now permits
one or more fields as described below. An initializer must have that exact nominal
type; equal numeric bounds confer no conversion. KD-037 now permits
`Battery { charge: 80 }` when 80 is inside Percent's inclusive bounds.
Already range-typed arithmetic may initialize the field, without proving bounds.
Construction may be a local initializer, return operand, ordinary call argument
or complete branch result of an existing conditional initializer; it does not
make hidden nested conditional values legal. Syntax recognition preserves named
identifier conditions and their blocks without parser type-name lookup.

Construction retains canonical RecordId/FieldId plus its typed initializer. It
produces a fresh Move record without a harness binding, anonymous public local,
new type, reference provenance, hidden loan, allocation or runtime constructor.
The existing complete-expression transaction handles initializer effects; KD-033
return and KD-034 whole-call rollback boundaries remain authoritative. A later
well-typed ownership failure commits none of a successful initializer prefix.
Static nominal type failure precedes ownership and is not rollback evidence.
Both finite-use and recurrent-reference traversal descend construction children.

Copy-field reads use only canonical named owned ParameterId/LocalId roots, resolved
through the owner's actual RecordId. They require availability and no active
exclusive whole-record loan, but permit active shared loans. They account for the
written owner use without consuming or partially moving it, changing transfer
history, creating field availability/loans/provenance, or establishing a hold.
A subsequent whole-record Move remains subject to ordinary availability and loans.
Moved/conditionally moved owners reject; a returning sibling does not participate
in a later suffix join. Existing lawful loan expiry is unchanged.

Repeated reads preserve stable loop state. `while battery.charge <= threshold`
is legal when its owner is available and loans permit the read. Construction is
rejected anywhere within a recurring condition, even as an argument to a Bool
call. Loop-local construction is an ordinary iteration-local Move value; it
cannot sanitize a prohibited pre-existing Move initializer or grant KD-033
terminal permission outside an actual return operand.

Typed-HIR publication performs a table-aware record-expression invariant check,
including owner eligibility, record/field membership and exact range results.
MIR retains typed trees/IDs and recursively validates all construction children,
hidden conditional values and residual unsupported projection representations.
No new MIR terminator, second checker or generalized ownership solver is added.
Private evidence distinguishes malformed HIR metadata from ownership-state proof
that field reads create no partial availability. Public declaration-boundary
rejections do not demonstrate partial-move checking.

Reference roots (`&T`/`&mut T`), temporary/nested projection, field borrowing,
field mutation, broader field types, partial moves, structural compatibility,
implicit nominal/literal conversion, invariant enforcement and replacement remain
unsupported. Ordinary functions remain checked/lowered, not executed or proven.
Verified battery contracts, diagnostics, counts and literal harness behavior are
unchanged. Construction implies no destruction, cleanup, allocation or runtime
general ordering guarantee outside record initializers; KD-036 specifies written
initializer order for multi-field construction.


## Multiple-field records and complete named construction (KED-021 / KD-036)

```klyxr
type Percent = range 0..100;
type Millivolts = range 0..5000;
record Battery { charge: Percent, voltage: Millivolts, reserve: Percent }
fn forward(charge: Percent, voltage: Millivolts) -> Battery {
    let battery = Battery { voltage: voltage, reserve: charge, charge: charge };
    let observed_voltage = battery.voltage;
    let observed_charge = battery.charge;
    return battery;
}
```

See `examples/multiple_field_records.klx` and
`examples/multiple_field_record_move_fail.klx`. A record has at least one unique
named-range field. Commas are required between entries; trailing commas are
optional. Complete construction supplies each declared field exactly once.
Unknown/duplicate entries diagnose their written occurrence before completeness;
missing fields are listed in declaration order; exact type errors follow written
initializer order. Equal range bounds confer no nominal conversion, and integer
literals may now form at exact field boundaries under KD-037. Values copied from another record may initialize
a field of the exact same named-range type; record identities remain distinct.

Declarations preserve canonical FieldIds in declaration order. AST/resolved/HIR
construction vectors preserve written order separately, including per-entry spans.
Ownership evaluates every initializer in that written order in the existing
expression transaction. A late failure restores the complete prepared entry,
including successful prefix effects, and publishes no destination. Completion
creates no balancing check, hold or permission and preserves inherited call/write
holds and recurrence. KD-033 return and KD-034 whole-call transactions still apply.
Successful construction is one fresh whole-record Move result with no field
owners, partial state, field provenance/loans or anonymous public locals.

Every field is readable through a direct named owned parameter/local. The exact
range value is Copy; the whole record remains available. Whole-record shared
loans are compatible and exclusive loans block reads. Whole-record Move blocks
all later field reads. Reads can repeat through stable loop backedges and appear
in permitted Copy-valued while conditions. Construction remains forbidden at any
depth of a recurring condition; loop-body construction remains iteration-local.

Typed-HIR publication validates every declaration, including unused records,
canonical table IDs, nonempty unique fields, unique spellings, valid range types,
reciprocal membership and orphan/multiple coverage, without panic on malformed
IDs. It then checks complete construction vectors and every initializer's exact
metadata and position. MIR retains ordered typed trees and recursively validates
all children in every container; it does not reconstruct HIR tables. The verified
battery state and specialized literal harness are explicitly gated to one field.
Unrelated ordinary multi-field declarations coexist with supported verified code
without broadening proof claims or changing proof counts.

Construction and reads are transparent data primitives. No factory, builder,
getter/setter or executable property is generated or required. Readability,
constructibility and mutability are separate future authorities. Empty/default/
positional/spread construction, broader field types, invariants, visibility,
field borrowing/mutation, partial moves, reference/temporary/nested projection,
general contextual propagation, layout, destruction, generalized cyclic inference, execution
and code generation are not implemented. Ordinary records are checked and lowered,
not executed or proven. This decision specifies record initializer ordering only;
it does not settle general argument/operator sequencing or runtime cleanup.


## Contextual named-range literal formation (KED-022 / KD-037)

A direct signed integer literal may form a Copy value at an existing ordinary
boundary independently requiring one exact canonical named range: ordinary return,
value/NoValue call argument, assignment to an established mutable range local,
write-through to an established range referent, or named record-field initializer.
Both declared endpoints are inclusive. Out-of-range values are Type errors at the
literal (including preserved grouped span), identifying the range, bounds and value.
Malformed/unrepresentable numbers retain earlier lexer/parser errors.

```klyxr
type Percent = range 0..100;
record Battery { charge: Percent, health: Percent }
fn identity(p: Percent) -> Percent { return p; }
fn full() -> Battery { return Battery { health: 100, charge: 80 }; }
fn revise(access: &mut Percent) -> Percent {
    let mut current = identity(0);
    current = 80;
    *access = 75;
    return 100;
}
```

See `examples/contextual_range_literals.klx` and
`examples/contextual_range_literal_fail.klx`. Typed HIR publishes a distinct
FormedRangeLiteral leaf with its sole canonical identity in ExprType::Range;
raw IntegerLiteral remains an expression-only operator constant. HIR validation
checks canonical tables and bounds; MIR retains and validates structural type tags.
The formed leaf creates no owner, loan, provenance, hold, allocation, finite use,
recurrent obligation or terminal permission. Existing call/write completion,
target protection, recurrence, written construction order and rollback are unchanged.
A bounds failure precedes ownership and is not rollback evidence.

`let x = 80;` still lacks an independently established range type and rejects;
later uses cannot infer it. `return 81 - 1;` cannot use the return expectation
through arithmetic. Conditional branches do not infer from siblings. Names,
field reads, dereferences and call results never change nominal identity, even
for equal bounds. Parentheses preserving a literal leaf permit formation; general
numeric negation, casts/conversions, annotations and constant folding are absent.
Inner `identity(100)` forms its argument independently, so `identity(100) - 1`
uses the existing arithmetic rule. Raw `p <= 101` and `p - 101` retain their current
behavior; this feature does not prove range-typed arithmetic bounds or safety.
Verified-body constants and top-level harness literals retain their separate
representation, eligibility gates, diagnostics, proofs and call-state behavior.
