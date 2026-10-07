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
The current prototype checks moves, acyclic loans, and narrow Copy loop stability and iteration-local Move ownership together in `compiler/ownership`.
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
KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, KD-028, KD-029, and KD-030 only for the documented
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
and no call/write hold or KD-030 recurrent obligation retains the loan. ConditionalMove blocks handle use but does not
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
generalized cyclic lifetime analysis beyond KD-030, reborrowing, escaping references, and broader MIR ownership dataflow.

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
ownership/path-sensitive acyclic loans and Copy loop stability/iteration-local Move → MIR Let/Assign/DerefAssign statements
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
KD-013, KD-014, KD-015, KD-017, KD-018, KD-019, KD-020, KD-021, KD-022, KD-023, KD-024, KD-025, KD-026, KD-027, KD-028, DP-008, DP-009, and OQ-018 through OQ-026.


### KED-012: while and cyclic MIR

AST, resolved statements and typed HIR preserve explicit While condition/body/span.
Resolution gives the body a child scope using the existing canonical LocalId
allocator; type checking establishes exact Bool before ownership. Ordinary branch
and loop bodies cannot contain returns. Verified syntax remains separate.

The existing HIR ownership checker enters a private loop mode and analyzes one
permitted iteration from a header snapshot. KED-012 restricted nested expressions/statements to
owned Copy values; KD-028 subsequently permits iteration-local Move in bodies,
as described below. KD-029 subsequently permits iteration-local body references;
pre-existing-handle use, reference conditions and non-Copy conditions reject with
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


### KED-013: iteration-local Move ownership

The existing checker replaces the KED-012 blanket non-Copy loop-body rejection
with a private per-loop scope containing canonical pre-existing Move places and
condition/body mode. Condition analysis retains the strict Copy boundary. Body
expressions may return fresh records; existing consumption checks permit transfer
only when the source is not in the current loop's header set. A forbidden transfer
rejects before changing availability or consuming future-use counts. Non-Copy
assignment still rejects before RHS analysis. KD-029 subsequently refines the
body-reference restriction without relaxing pre-existing-handle or condition checks.

Branch clones inherit the same loop boundary. A nested loop establishes a new
header from the current state, protecting outer-iteration locals; afterward the
outer boundary is restored. Each loop checks one cloned iteration, scopes away
body-local availability/provenance, and compares remaining state semantically with
its header. Source names, diagnostic move sites, spans and finite FutureUses counts
remain excluded from equality. Body-local Available/Moved/ConditionalMove states
may evolve normally and disappear at lexical exit without runtime destruction.
Outside handles/loans remain unchanged at accepted backedges and constrain owner
access as before; KD-030 distinguishes false-exit liveness.

No AST, resolver, type, HIR or MIR representation change is required. Existing
Let/call expressions and Branch/Goto backedges retain static LocalIds, exact types,
deterministic construction and cyclic validation. Ownership remains above MIR
without generalized fixed points or a second engine. KD-029 adds iteration-local
borrowing; OQ-024/OQ-026 retain loop-carried ownership/reference-use analysis; OQ-023/OQ-025 retain
replacement/destruction. Ordinary functions remain non-executable and unverified.


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
