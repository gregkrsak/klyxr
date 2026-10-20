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
value functions with statement conditionals, ownership-stable while loops,
iteration-local Move values, and ordinary multiple-field named-range records.
The specialized signed-integer verified-state/harness subset remains single-field.
Explicit resolution assigns compilation-local function, parameter, local, type,
field, and top-level binding IDs. Expression and value-flow type checking produce
canonical typed HIR. Public HIR inspection remains read-only; names/spans are
diagnostic metadata.

KED-004 introduced ordinary Bool/range value functions, immutable locals, calls,
and explicit `return expression;` (KD-019). KED-005 now permits existing records
by value in ordinary signatures, call results, locals, and returns. Ordinary and
verified functions retain one function namespace and ID table. Bare integer locals remain unsupported; KD-037 now permits direct literal
formation at independently established named-range value boundaries.

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
auto-dereference, generalized reference projection beyond KD-039/KD-040, and partial
borrowing/moves remain unsupported (KD-021 / OQ-024). KED-027 implements KD-042's
deterministic ownership-attached cleanup through private synthetic resource leaves,
explicit lexical MIR metadata and bounded runtime release. Public source records
still have named-range fields; no resource registration or production resource API
is exposed.

KED-007 adds explicit `*reference` for directly named ordinary reference parameters
and locals. Copy referents (Bool/ranges) may be read through shared or mutable
references without consuming the handle. `*reference = expression;` requires an
exclusive mutable reference and an exact Copy referent/RHS type. Non-Copy move-out
and replacement are Ownership errors; no destruction or displaced-value semantics
are implied. Reads/writes count for last use; writes hold the loan throughout the
RHS. Copied dereference arguments do not call-hold a reference, while reference
arguments retain KED-006 holds. General owner replacement, field mutation beyond KD-038/KD-040, auto-deref,
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
solver, cleanup, or MIR-based ownership solver. KD-033 later adds structured early return. KED-012 later adds narrow statement loops. Conditional
values in returns/call arguments/general expression positions remain open under
OQ-022/OQ-024/OQ-026.

KED-011 adds direct `local = expression;` statements for mutable owned Copy Bool
and named-range locals (KD-026). Parameters, immutable locals, and reference locals
are Type errors; exact-typed non-Copy record replacement is an Ownership error.
RHS typing is exact and nominal; KD-037 forms direct literals at established
named-range targets only. The RHS evaluates
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
KED-013 subsequently permits iteration-local Move activity; KED-014 also permits
iteration-local borrowing in bodies, as described below.
One permitted iteration must preserve the header's Move availability, reference
provenance and active loans after body locals are scoped away. Outside owners and
references may remain untouched; outside live loans still constrain Copy owner
reads/writes, while dead pre-loop loans stay dead. There is no fixed-point solver.
MIR uses Goto → header Branch → body/exit, with body Goto back to header. Its
validator now accepts cycles with visited tracking; structural validity does not
prove termination. Both constant-condition edges remain. For and labeled loop transfers (KD-031/KD-032 settle unlabeled continue/break),
loop values, verified loops, and general cyclic ownership/loans
remain unresolved. Ordinary loops are not executed by `check`/`verify` or proven; KD-041 `run` executes their MIR.

KED-013 permits iteration-local Move ownership in ordinary while bodies (KD-028):
fresh record call results, transfer chains, by-value consumption, and existing
acyclic branch joins. Every loop snapshots its own pre-existing Move place set;
outer-loop locals are pre-existing at an inner header and cannot move there.
Body-local availability evolves normally and is scoped away before comparing
remaining header/backedge state. Pre-existing owners cannot transfer in the loop,
non-Copy conditions remain forbidden, KED-014 refines the body-reference boundary, and non-Copy
replacement remains unresolved. Lexical cleanup is not runtime destruction.
No syntax, HIR/MIR representation, second ownership engine, or fixed-point solver
is added by KED-013. KD-035 later adds ordinary record construction; KD-041 later adds bounded MIR execution.

Ordinary functions are parsed, resolved, type-checked, and ownership-checked,
including loan and borrowed-access legality, and lowered to ordinary MIR. `check`/`verify` do not execute or prove ordinary functions; KD-041 `run` executes them through bounded preparation. A range result type does not establish that the
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
General cyclic ownership/lifetimes beyond iteration-local moves/borrowing and stable carried access, advanced effects beyond frozen KED-030, runtime
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

Frozen KED-030 Draft 2's first phase uses ordinary explicit upper bounds:

```klyxr
fn observe() -> bool effects clock, io { return true; }
fn demo() -> bool effects io, clock { return observe(); }
```

The complete declaration is caller-visible even when its body does not perform an
effect. Absence means empty. All eight canonical identities are independent;
`unsafe` adds no safety permission. Derived cleanup contracts enter only where the
existing successful ownership checker produces eligible custody evidence. A move
transfers that obligation; ordinary terminal stops add no unwind. This branch's
implementation awaits independent exact-head review and Greg's acceptance.

The following remains illustrative future policy syntax, outside KED-030:

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


### KED-014: iteration-local borrowing

KD-029 permits fresh body references to pre-existing owned Copy/record values and
iteration-local Move owners. Existing shared aliases, mutable-handle transfers,
call holds, named dereference, Copy-safe writes and acyclic path-sensitive last-use
rules apply. A borrowed local record can move after its loan ends. Each loop
captures canonical header handle identities and loan provenance: these handles
were unusable under KED-014 but could persist untouched. KD-030 subsequently
permits stable carried access without loan resurrection.
Live header loans continue constraining compatible owner access/new borrows.
While conditions remain reference-free; non-Copy condition/transfer and replacement
boundaries are unchanged. Nested loops classify outer-created handles as
pre-existing and restore the outer boundary afterward.

After one cloned iteration, lexical cleanup removes body handles and availability,
then expires loans. The checker proves no iteration-created loan or surviving
handle provenance remains before restoring the header allocator; semantic header
comparison still checks availability, handles, active loans and allocation state.
Names/spans and finite FutureUses counts remain diagnostic/continuation metadata.
This permits safe reuse of transient private LoanIds without hiding provenance.
No generalized cyclic NLL, fixed-point solver, second checker, syntax, HIR/MIR
surface change, runtime destruction, execution or verification is introduced.
KED-015 subsequently permits stable carried access through scoped recurrent
obligations; generalized cyclic lifetimes remain open under OQ-024/OQ-026.


### KED-015: stable loop-carried reference access

KD-030 distinguishes straight-line references, iteration-local loop references,
and stable loop-carried references. Existing header shared handles may be read,
passed to calls and copied into body-local aliases; mutable handles may be read
and used for Copy-safe writes, but cannot move into bindings or by-value calls.
Reference conditions, reference replacement, reborrowing and carried owned Move
changes remain unsupported. Constant and apparently one-shot loops do not weaken
recurrent protection.

The same typed-HIR checker discovers canonical header reference uses in both
branches and nested bodies. Private scoped sets of original LoanIds provide
recurrent retention independently of finite FutureUses and operation holds.
Branches inherit all frames: a skipped use cannot expire the recurrent loan.
Loans retain their identity/owner/kind continuously, without refreshing, sentinel
counts or repeated body analysis. Nested loops add frames and discharge only their
own obligations. Outer-created provenance may be carried by an inner loop but
must still die before the outer backedge under KD-029.

Lexical cleanup removes body-local aliases and new provenance; allocator
normalization cannot conceal surviving new loans. Exact backedge validation runs
with obligations still active. Only then is the current frame discharged and
false-exit suffix liveness applied, preserving enclosing obligations and post-loop
uses. With neither, the loan can end and owner access resume. Compound expressions
and statements restore their pre-operation snapshot on error, including partially
consumed counts and operation holds. Canonical HIR/MIR and public APIs are unchanged.
No generalized cyclic lifetime/fixed-point solver, second checker, MIR borrowing,
execution, destruction or termination proof is implemented.


### KED-016: structured continue and explicit backedges

AST, resolved statements and typed HIR preserve Continue/span. Parser and resolver lexical
loop depth reject use outside an ordinary while; recursive loop/branch structure
identifies the innermost target. Structural block checks reject any suffix after
a direct continue or an all-terminating conditional. Loops still have a false
exit, including constant conditions and all-continuing bodies. No general
unreachable-code analysis is implemented.

The existing ownership checker carries immutable header snapshots and finite
outside continuations on a private nested target stack. Statement analysis returns
fallthrough or a previously validated continue; conditionals analyze each arm in
source order and join only surviving fallthrough states. A single surviving arm
keeps its actual effects while branch-local state is scoped away. Each explicit
continue runs the existing lexical provenance cleanup, safe allocator normalization,
and exact header comparison transactionally with recurrent frames still active.
Normal bottom-of-body backedges use that same invariant. No continue discharges
an obligation frame; failed analysis restores its complete entering snapshot.

Finite future-use summaries are computed backwards over structured syntax before
forward ownership operations. A continue substitutes its loop's outside finite
continuation for the skipped same-iteration suffix. Statement branches receive
separate summaries; a pre-dispatch summary retains uses possible on either edge.
Expression-level counts and existing call/write holds remain unchanged. Recurrent
LoanId sets remain separate from FutureUses and retain exact provenance even with
zero finite counts and zero holds. The independently checked header-condition false
state, not a fabricated body fallthrough, supplies the loop exit. Only its current
frame is discharged; enclosing frames and post-loop finite uses still protect loans.

MIR maintains a nested header stack and returns an optional live tail. Continue
finishes the current block with the existing Goto to that header. No new terminator
or ownership annotation is introduced. Only real fallthrough arms create joins or
lower a suffix, so all emitted blocks remain reachable under the unchanged
validator. The header false exit still exists when all body paths continue.
Canonical source IDs, deterministic lowering, and the verified proof path remain
unchanged. No generalized CFG/fixed-point machinery or competing checker is added.


### KED-017: structured break and canonical loop exits

AST, resolution, and typed ordinary HIR contain Break/span. Parser and resolver
reject placement outside while, labels/values, and suffixes after a direct break
or recursively all-terminal conditional. Actual edge class and lexical
fallthrough are distinct: mixed Break/Continue arms have no surviving suffix
state. The single HIR ownership checker retains immutable header, canonical false
exit, and outside-continuation snapshots on a finite nested target stack.

The four structured loop edges have distinct contracts:

| Edge | Ownership/reference boundary | Recurrent frames | Existing MIR |
| --- | --- | --- | --- |
| Ordinary bottom backedge | Exact header state after local cleanup | Retain current and enclosing | Goto header |
| Explicit continue backedge | Independently validate exact header state | Retain current and enclosing | Goto header |
| Ordinary condition-false exit | Independently derive canonical exit before body candidates | Discharge exactly current; retain enclosing | Header Branch false target |
| Explicit break exit | Validate actual normalized candidate against saved canonical exit | Discharge exactly target; retain enclosing | Goto innermost exit |

Canonical false exit comes from the checked condition state, outside finite uses,
and outer obligations. A candidate never supplies or replaces it. Break first
checks invariants whose evidence could disappear: exact frame membership/position,
outer context, carried handle mapping and original active LoanId/owner/kind, and
balanced operation holds. This is bounded integrity checking, not raw header
state equality. Then the actual candidate installs captured outside uses before
expiry, scopes target-iteration locals, pops exactly the target frame/context,
expires with outer obligations and finite uses, proves fresh provenance absent,
and safely normalizes its allocator. Semantic comparison does not repair carried
availability or provenance. All steps are transactional; failure restores the
actual entering snapshot, including faults and canonical-exit target context.

Finite backward summaries cut skipped same-iteration suffixes before preceding
operations. Recurrent sets remain separate and active until break itself; post-loop
finite uses independently protect loans. Nested exits preserve outer frames and
continuous LoanIds, including when the removed inner frame is empty. A recurrent-only
loan may end when no outer or outside retention reason remains. No resurrection,
pre-existing Move relaxation, generalized fixed point, or second checker is added.

MIR maintains header/exit pairs and optional live tails. Break finishes its block
with existing Goto to the innermost exit; continue still targets the header. No
all-terminal suffix join is emitted; nested target restoration is deterministic.
The validator is unchanged. Constant and all-breaking bodies retain false edges.
Private tests inspect normalized candidates before loop consumption and inject
pre-reduction and post-cleanup faults. The specialized verifier and its 1,800-case
affine cross-check remain unchanged; ordinary code is not executed or proven.


### KED-018 / KD-033 — Structured early return

Explicit ordinary return may end any statement path, including branches and
while bodies. Completeness means no structural closing-brace fallthrough after
loop transfers are consumed; while always retains its condition-false edge,
without constant pruning or termination claims. Returns terminate lexical blocks;
parsed and public ASTs obey the same structural checks. Exact operand typing
remains before ownership.

The finite continuation before return is operand uses only, excluding every
bypassed suffix. Current/enclosing recurrent obligations remain active throughout
actual operand evaluation. A scoped private permission bypasses only pre-existing
owned non-reference Move restrictions within the return-expression tree; it never
relaxes preceding operations, availability, loans, provenance, references or holds.
The return transaction snapshots after finite-summary installation/expiry and
before permission; all operand/final-validation failures restore that exact entry.
Permission restores on both outcomes. Successful return state has no local
successor normalization or merge. Mixed return/break/continue retains actual
surviving-path state and canonical loop false exits.

MIR lowers whole bodies with existing Return terminators; no synthetic join/final
return or MIR-validator semantic changes. No function-exit ownership join, second
checker, generalized CFG/fixed points, SSA/phi, reference returns, reborrowing or
runtime behavior is added. The verified proof path remains unchanged.


### KED-019 / KD-034 — No-value ordinary functions and call statements

Implemented from frozen Issue #39 baseline aaca52a63a40b252287935b3c7416e26f4ea50d9.
Omitting `-> Type` explicitly declares NoValue, not inference or a unit/void
expression type. NoValue functions permit `return;` or validated top-level
completion. Value functions preserve KD-033 exact returns/structural completeness.
Only named NoValue calls may be standalone call statements; value discard and
NoValue expressions are rejected. AST/resolution/HIR retain canonical IDs and
separate statement forms; signature metadata uses an optional result payload.

Statement calls have whole-operation prepared-entry rollback, including all
arguments, holds, release/expiry and final validation (K19-01). Both finite and
recurrent-reference traversal include arguments. Bare return has empty finite
continuation, pre-snapshot expiry, no terminal Move permission, balanced holds
and exact prepared-state rollback. Recurrence and loop rules remain authoritative;
completion performs no repair, cleanup or synthetic function-exit join.
MIR has honest CallNoValue/ReturnNoValue forms and exhaustive argument validation.
The single ownership checker remains above MIR. Verified functions/harness/proofs
remain unchanged; no destruction, execution, unit/void, generalized dataflow or
new lifetime semantics are implemented.


### KED-020 / KD-035 — Ordinary record construction and Copy-field reads

Frozen Issue #41 Draft 4 is implemented from baseline
02eb5364927ee9d4325fc9d4fdbe05623e3ce0d3, including K20-S01–S03,
K20-A01–A07 and K20-F01–F04. The existing one named-range-field record schema
is unchanged. `Battery { charge: expression }` produces a fresh owned Move
record from an exactly typed initializer. Bare integers do not implicitly become
named ranges, and construction proves no bounds/invariants or runtime behavior.
Canonical RecordId/FieldId selection and typing precede ownership. Construction
is an expression child in all applicable visitors and inherits complete-expression,
KD-033 return and KD-034 whole-call transactions; it creates no harness binding,
anonymous public local, hidden loan/provenance or allocator effect.

`owned.charge` reads an exact Copy range from a direct named owned parameter/local,
identified by ParameterId/LocalId. Availability and exclusive whole-record loans
are checked without consuming the owner. Shared loans remain compatible and
protected; no field loan, projected place, provenance or partial availability is
created. Reads may occur in while conditions and stable loop bodies; construction
anywhere beneath a recurring condition is rejected. Reference/temporary/nested
roots, field borrowing, mutation beyond KD-038 and broader field declaration types remain excluded.

Typed-HIR publication explicitly validates table relationships and existing
conditional positions before ownership. MIR recursively validates construction
children and excludes hidden conditionals/residual verified projections without
reconstructing canonical tables. Verified battery/harness/proof behavior is unchanged.
No generalized ownership solver, execution, allocation, destruction or broader
nominal/literal semantics is introduced.


### KED-021 / KD-036 — Complete named multiple-field construction

Ordinary records now declare one or more unique named-range fields. Canonical
FieldIds follow declaration order; complete named construction retains written
initializer order independently, including reversed order. Initializers require
exact nominal types and are evaluated in written order within the existing
complete-expression transaction. Later failure restores the entire prepared entry
and publishes no destination. Completion preserves inherited holds, recurrence and
permissions; it introduces no separate balancing stage or ownership engine.

Every declared field supports non-consuming direct named-owner Copy reads under
whole-record availability and loans. Records remain Move; there are no field
owners, partial availability, field borrowing, mutation beyond KD-038 or reference projections.
The typed-HIR boundary checks all declaration tables, including unused malformed
records, before ordered construction children. MIR preserves and recursively
checks all children. Explicit gates keep verified state/harness support single-field.
Ordinary construction and reads are transparent data primitives, not generated
factory/accessor code. Visibility, constructibility, invariants, broader field
categories, layout, destruction, execution and generalized cyclic analysis remain
unresolved. See KD-036 and compiler/README.md for the implemented boundary.


### KED-022 / KD-037 — Bounded contextual literal formation

Direct signed integer literal leaves can form exactly the canonical range required
independently by ordinary returns, either ordinary call form, established-local
assignment, established-referent write-through and record fields. Inclusive bounds
are checked during typing. Typed HIR distinguishes FormedRangeLiteral from raw
IntegerLiteral; checked table-aware validation precedes ownership. MIR preserves
and checks leaf/type structure only. The formed Copy leaf has no ownership effects.
No expectation enters recursive synthesis or conditional sibling inference; raw
operator constants, unannotated literal-local rejection, nominal identity and
verified/harness behavior are unchanged. Arithmetic bounds proofs, general
conversions/inference, annotations and broader numeric semantics remain open.


### KED-023: direct Copy-field assignment

KD-038 permits `local.field = expression;` only through a directly named mutable
owned ordinary record local and the field's exact named-range type. Contextual
literals use KD-037. The RHS completes without target reservation, then ordinary
expiry and whole-record availability/shared-or-exclusive-loan checks precede the
write. A dedicated prepared field-operation transaction restores all checker state
on failure while committing lawful RHS effects on success. The write preserves
one whole owner; no field owner/loan/provenance, partial move, replacement or
terminal permission is added. Branch/loop/return/call boundaries remain unchanged.
AST/HIR/MIR have genuine field-write representations and recursive validation.
Verified/harness mutation remains specialized and single-field; ordinary field
writes are checked/lowered, not executed or proven. Reference/temporary/nested
projection, field borrowing, invariants, visibility, destruction and generalized
mutation/cyclic analysis remain open.


### KED-024: direct Copy-field reads through named references

KD-039 implements `view.field` for directly named ordinary `&Record` / `&mut Record`
parameters and inferred locals. It copies the exact declared named-range field
through the existing whole-record loan without moving handle/record, reborrowing,
converting exclusive permission, or creating field-level state. Typed HIR/MIR
explicitly distinguish referenced reads from owned reads. Canonical table checks
and distinct field-token spans precede ownership; MIR validates structural facts.

Reads count for finite use and scoped recurrence. Final use may expire a loan;
later alias/handle uses, receiving holds and recurrent obligations retain it.
Existing expression/return/call transactions restore even prefix-expired loans
on later failure. Canonical origin/initializer-lineage integrity validates external
parameters, shared aliases and exclusive transfers using the single handle→loan
mapping. KD-040 subsequently permits writes through named exclusive record
references. Reference-backed while conditions, shared-reference writes, field borrowing, temporary/nested/dereference projection and whole-record move-out
remain unsupported. Ordinary functions remain unexecuted/unproven; verified battery
and single-field harness eligibility/proofs are unchanged.

### KED-025: direct Copy-field assignment through exclusive named references

KD-040 implements `access.field = value;` for a directly named ordinary exact
`&mut Record` parameter or local, including an immutable handle binding. The
actual record selects its canonical field and exact nominal range; direct
contextual literals form at that boundary. Shared and non-record references reject.
Unlike KD-038 owned-local RHS-first writes, this operation retains the existing
exclusive loan before the complete RHS, then revalidates the available handle,
live lineage and original LoanId before authorizing the write. Same-handle reads
are legal; original-owner conflicts and RHS target transfers reject. Last use
expires only after completion. Lawful unrelated RHS effects commit on success;
failure restores the exact prepared operation entry with the initializer index
shared. Finite/recurrent summaries include target and RHS. Stable loops, continue,
break and existing convergence integrity remain authoritative. HIR/MIR preserve
CopyReferenceFieldAssign distinctly. General projection, reborrow/coercion, field
loans, partial ownership and whole-record replacement remain excluded. Ordinary
functions are checked/lowered only; verified/harness behavior and proofs are unchanged.

## KED-026 execution addition (KD-041)

Frozen Draft 3 adds bounded `run` of the complete accepted ordinary MIR subset.
Source ingestion and the existing frontend/ownership pipeline are protected before
growth; an opaque checked artifact binds canonical HIR, MIR, and lowering origins.
Explicit frames and continuations preserve written evaluation order, moves,
short-circuiting, prefix mutations, and immediate widened named-range traps.
Execution is independent of specialized proof and harness evaluation. `check` and
`verify` retain their existing behavior. This is interpretation, not native code
generation or ordinary contract proof. APIs, limits, origins, and tool reporting
are documented in `docs/BOUNDED_EXECUTION.md`; excluded language boundaries remain
unchanged. General execution mechanisms beyond this frozen subset remain open.


## KED-027 cleanup decision (KD-042)

Frozen Draft 1 accepts RAII-like deterministic scope cleanup for canonically
resource-bearing owned values. Resource leaves have one fixed trusted non-fallible
release operation; records become resource-bearing structurally and clean their
resource-bearing fields recursively in reverse declaration order without a
user-authored destructor body. Every resource-bearing value is Move, but ordinary
Move values remain semantically inert at destruction. A move transfers the unique
cleanup responsibility; borrowing and loan expiry do not affect it.

Still-live owners clean exactly once on covered structured exits: block fallthrough,
implicit NoValue completion, value or NoValue return, break, continue, and future
typed recoverable propagation. Exited inner scopes precede outer scopes; locals
clean in reverse successful-initialization order before parameters, parameters
clean in reverse declaration order, and return-result ownership is transferred
before the callee cleans its remainder. Pending record construction owns each
successfully initialized resource-bearing field and, on future recoverable failure,
cleans the actual initialized set in reverse canonical field-declaration order.

Automatic cleanup releases authority only. Fallible flush, commit, publication,
durability, synchronization, and protocol completion remain explicit. Cleanup must
be represented through auditable validated MIR control flow, never runtime HIR
reconstruction or name-based destructor lookup. Range traps, configured stops,
integrity failures, host/tool failures, process termination, panic/unwind, and task
cancellation receive no cleanup guarantee from KD-042. The decision adds no partial
moves, non-Copy replacement, generalized projection, arbitrary destructor code,
resource API, effects system, heap, FFI, exception model, or temporary-lifetime rule.
The authorized KED-027 implementation supplies private canonical acquisition and
release descriptors, non-cloneable resource payloads, bounded shared cleanup plans,
explicit MIR scope entries/initializations/exits and protected return ownership.
Runtime activations retain actual successful initialization order; transferred
slots have explicit provenance distinct from vacancy. Pending construction tracks
exact field membership and a private bounded failure facility demonstrates abort
order without admitting source-level recoverable propagation. Cleanup stops retain
earlier effects and remaining owned payloads and cannot retry or publish success.
Undefined resource argument temporaries reject at canonical admission. Every
private fixture repeats canonical validation, ownership, lowering, correspondence
and opaque approval. Source syntax and ordinary resource-free accounting remain
unchanged. Implementation evidence and the exact predecessor inventory are in
`docs/engineering/KED-027-IMPLEMENTATION-LEDGER.md`; during K27-I01 only GitHub
Actions supplies authoritative executable evidence. Acceptance remains Greg's
decision and merge remains a human action.

## KD-043 owned Move replacement

Accepted KED-028 Draft 1 adds deterministic whole-owner replacement through existing
assignment syntax for available mutable owned Move locals. Exact record identity,
prepared checker rollback, target continuity, final loan expiry and relative loop
boundaries are authoritative. RHS results are protected before displaced cleanup;
new generations register in the target's owning activation. Old generations never
clean a newer occupant. Public records demonstrate inert replacement; private
synthetic resources demonstrate fixed canonical cleanup and identity conservation.

Earlier sections retain their historical restrictions; this narrowly refines their
direct-local replacement exclusion. References, projections, self-consuming
transforms, unavailable slots, loop-carried Move mutation, arbitrary temporaries,
user destructors and production resource APIs remain excluded. See
LANGUAGE_DECISIONS.md and the KED-028 implementation ledger for scope and
GitHub-executed evidence. Greg accepted and human-merged PR #65 as commit
`72c34593e79e7763ce16f0e18ad99595959a2454`, tree
`2b9339fbe21f705cb8c5722ff681c34680870490`.

## KD-044 expression temporary ownership

Accepted KED-029 Draft 2 gives admitted resource-bearing expression intermediates
explicit bounded ownership. Caller expressions share their source evaluation
region; callee statements and returns have their own regions. Record fields use
child regions, transferring into pending construction before child retirement.
Owned arguments evaluate left to right and transfer once on whole-call entry.
Remaining siblings retire in reverse successful creation order; complete aggregates
retain reverse canonical field-declaration cleanup order.

Results are protected before other temporaries retire and before locals, existing
assignments, returns, replacements or condition selection continue. Pending-field
transfer is the specified immediate-transfer exception. Static regions/sites and
dynamic activations/owners are distinct; temporary handles are neither LocalIds nor
borrowable places. Resource-free evaluations omit empty temporary machinery.

This refines earlier temporary exclusions only within KD-044. Temporary
borrowing, value-returning call statements, new recurring-while Move/reference
eligibility, unwind, rollback, custom destructors and public resource APIs remain
excluded. See the KED-029 implementation ledger for GitHub-executed evidence.
Greg accepted and human-merged PR #67 as commit
`5ce5dc5895efab7ba29a7cbc8e980ce58a7b24ab`, tree
`e95fb9e69d7554719d86d9662cf22cbf5151b84b`.

## Frozen KED-031 implementation boundary

Issue #72 Draft 4 adds exact ordinary `Result<success: S, failure: F>` signatures,
contextual Ok/Error/Err construction and a fixed exhaustive statement match. Error
and Err normalize to one canonical Error tag. The full ASCII case-insensitive
Result/Ok/Error/Err/match families are reserved, while only exact syntax spellings
are accepted. Every Result is Move and carries an independent unresolved outcome
obligation through locals, calls, nested payloads, branches and structured exits.
Cleanup and conditional Move unavailability cannot acknowledge it.

Finite owned bool/range/record/nested payloads, ordered variant plans, structural
cleanup-effect unions and actual machine-owned cells preserve active-only custody.
Manual propagation protects the complete outgoing Result before callee cleanup;
cleanup failure prevents delivery. Resource-free Results do not enable the private
resource catalog. No conditional Result initializer, raw named-range constructor
formation, reference/public-record/verified/harness extension, general enum/pattern,
`?`, conversion, unwind, retry, rollback or production resource API is added.
The implementation and ledger await independent review and Greg's acceptance.

## Frozen KED-032 implementation boundary

Frozen Issue #75 Draft 2 adds exact lowercase contextual `or return` only at an
ordinary Result-returning Value function's local initializer. The independently
typed inner Result and declared outer Result must share exact failure identity;
their success types may differ. Success initializes the original local in its
actual lexical scope; failure canonically rewraps the selected payload and uses
protected ordinary return, actual-scope cleanup and delivery after completion.
There is no synthetic owning match arm. Existing replacement generations yield
direct A → C → B versus explicit match A → B → C.

The existing ownership checker retains outcome/effect witnesses and all loop/loan
restrictions. Tool stops remain distinct from source Error. Contextual `or` remains
an identifier elsewhere; existing keyword aliases/trivia are preserved. No `?`,
conversion, nested propagation expression, conditional Result initializer, Result
replacement, verified/harness extension, unwind or resource API is added. The
implementation is authorized on its dedicated branch; independent review and
Greg's acceptance remain pending. Its exact baseline is the human-merged KED-031
commit 74c1bedc8f847ffaeb38ed4795ee91730e35b949, tree
2d0cc78b67a8cb69ea8feaff183dbf1b91938e63.

## Frozen KED-033 implementation boundary

Issue #78 Draft 2 is frozen; Greg separately authorized implementation on baseline
62023217b3f93e53cae302731643cbcea1fb3d17 (tree
3998bcad9b34d66e960b863eb20b7b78680da07d). The branch implements one explicitly
selected immutable sensor-demo catalog: exact nominal Reading 0..4095 and
SensorError 1..3, opaque Move Resource Sensor, canonical sensor_open/sensor_read
trusted calls and cleanup-only fixed release, all effects io. Operation-specific
grants remain separate from effects. Open Error is only 1/3; read Error is only
2/3 and preserves the Sensor. No selected catalog means no names or authority.

The defining source uses existing exact local `or return`. Owned helpers, protected
internal relay, shared observation, nested Results and structured cleanup reuse
the existing ownership/outcome/effect/temporary systems. Retire needs io for
residual cleanup; relay transfer alone does not. Mutable Sensor refs, conditional
Sensor initialization, replacement, record fields, representation/forging and
public live-Sensor egress remain excluded. Existing whole-Result obligations and
sealed private primitive cleanup are unchanged.

Explicit MIR plans and opaque approval seal catalog/profile/compiler/configuration,
grants, binding/signature/effect identities, roots, origins and custody. Only a
persistent ExecutionSession may invoke: its pre-reserved machine-owned receipt
holds actual binding payload before any fallible boundary; stopped sessions retain
unfinished responsibilities, protected Results and completed prefix. Terminal
Host/Limit/Integrity/trap/cleanup failure never becomes source Error or permits
retry/resume/unwind. Teardown is not successful release. The fixed deterministic
fixture is not hardware/FFI. KED-034's frozen completion specification now has an
implementation candidate, described below; acceptance remains pending.

CLI `run` and `audit` require `--profile sensor-demo` and an explicit --function
root; the static bounded audit follows sealed structural edges through empty-effect
helpers and both alternatives, disclosing grants versus reachable use and exact
release pairing without invocation. See docs/engineering/KED-033-TRUSTED-SENSOR.md,
its ledger and identity manifest for contracts, budgets, commands and evidence.
Implementation is not independent confirmation or acceptance; Sarah's exact-head
review and Greg's acceptance remain separate, and only humans merge.

### KED-034 frozen completion implementation candidate

Draft PR #83 implements consuming `sensor_finish(Sensor)` with exact
`Result<success:FinishReceipt,failure:SensorError>` and `{io}`. FinishReceipt is
nominal `1..1`, Copy source-formable status data, never proof or authority.
Only finish Error(3) is admitted. Both ordinary outcomes remain protected until
the exact fixed release succeeds. Pending completion custody and protected Result
are separate; terminal stops retain actual prefix and remaining duty with no
retry/resume/rollback or caller delivery. The existing checker, temporary, Result,
cleanup, effect and opaque approval systems remain the only authorities.

For this task GitHub Actions is the sole authorized Rust execution environment.
M0 used a fresh same-tree baseline gate; M5 is separately preserved before M6-M9.
See `docs/engineering/KED-034-COMPLETION-CUSTODY.md` and the implementation ledger
and manifest for source examples, exact boundaries, malformed controls, workflow
evidence and limitations. This candidate changes no Accepted decision or proof
claim and still requires Sarah's independent exact-head review and Greg's decision.
