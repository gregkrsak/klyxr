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
The current straight-line and acyclic-branch prototype checks these together in `compiler/ownership`.
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
and value-flow type checking → typed HIR → core ownership, loan, and borrowed-access checking → specialized
ordinary MIR CFG lowering (ordinary functions) or specialized
integer contract proof (verified functions) → diagnostics. This implements KD-012, KD-018, KD-019,
KD-020, KD-021, KD-022, and KD-023 only for the documented
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
KED-004 through KED-008 do not execute, prove, or dynamically enforce ordinary functions.
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
with syntactic future-use counts for straight-line and branch-scoped places and operation-hold counts for nested
calls and (under KED-007) whole write statements.
Shared copies and mutable moves retain loan identity across LocalIds. Loans expire
when no available derived handle has future uses and no call/write holds the loan. Local
transfers attach destination provenance before expiry; final-use arguments attach
call holds before expiry. Reference parameters have external provenance without
interprocedural owner reconstruction. Loan state is not published in HIR.
Exclusive borrowing requires a mutable owned local; `let mut` enables no assignment.
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
No destruction, field mutation, auto-deref, or direct reassignment is implemented.

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

KED-008 conservatively pins incoming live loans through each whole conditional,
including nested branches. Branches analyze independently from the same state;
branch-local handles and loans are discarded at exit. At the join, syntactic use
counts from both branches are consumed and normal straight-line expiry resumes.
No branch result or escaping handle exists. Existing call/write operation holds
remain active; general path-sensitive loan precision remains open.

`compiler/mir` lowers only ordinary typed HIR after frontend ownership checks.
`mir::lower(&program)` leaves `compile_source`'s canonical HIR result unchanged.
MIR preserves FunctionId/ParameterId/LocalId and exact expression types. Public
inspection is read-only; BasicBlockId and canonical block storage are compilation-local.
Each block owns Let/DerefAssign statements and exactly one Goto, typed Bool Branch,
or value Return terminator. Expressions retain their typed HIR trees. Structured
recursive lowering allocates blocks deterministically, gives every fallthrough
an explicit Goto, and emits the final source return as a terminator. Empty else
branches use a direct false edge to the join. A structural validator checks entry,
targets, reachability, acyclicity, and Bool conditions; unique IDs and single
terminators follow from indexed storage and the block type. Every finite path ends
in Return. MIR discovers no new source semantic errors. The CLI exercises lowering.

There are no phi nodes, block parameters/results, SSA, loops, general temporaries,
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
KD-013, KD-014, KD-015, KD-017, KD-018, KD-019, KD-020, KD-021, KD-022, KD-023, DP-008, DP-009, and OQ-018 through OQ-026.
