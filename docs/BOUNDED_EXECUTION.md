# Bounded ordinary MIR execution (KED-026 Draft 3)

`cargo run --locked --bin klyxr -- run examples/ordinary_run.klx --function demo`
prints `Pair { first: Percent(89), second: Percent(90) }`. Initializers run in
written order; rendering uses canonical declaration order. `loop_demo` returns
`Percent(0)`; `trap_demo` stops at the inner subtraction with computed `-101`.

This command executes the complete accepted ordinary subset after the existing
parse, resolve, type, and ownership passes. It never calls proof evaluation,
`verify_report`, the affine kernel, specialized functions, or harness calls.
Frontend errors anywhere still block execution. `check` and `verify` retain their
existing behavior and output. Running an ordinary function proves no contract.

## Public authority and runtime

`execution::prepare_execution(&str, AdmissionLimits)` and
`prepare_execution_from_reader(Read, AdmissionLimits)` are the only artifact
constructors. Caller-owned `&str` is checked before source-dependent work; the
reader accumulates at most the admitted source size and reads one lookahead byte.
`CheckedExecutable` has no public fields or mutators. It retains an immutable
successful-ownership witness, canonical typed HIR, the same compilation's MIR,
and origins. Private preparation identities and exact deterministic correspondence
reject component replacement, including numerically aligned foreign IDs.
`compile_source`, public HIR inspection, and `mir::lower` do not grant authority.
`execute(&CheckedExecutable, FunctionId, ExecutionLimits)` validates the artifact
and zero-parameter ordinary entry. MIR functions are found by stored FunctionId,
including files with verified declarations in the same ID namespace.

An explicit continuation stack executes MIR statements and terminators. HIR
statement trees are used only in admission/lowering. Frame IDs increase with
checked arithmetic; references contain a frame ID and canonical local/parameter
place, permission, and referent type. Slots are uninitialized, available, or
vacant. Shared handles copy; exclusive handles and records transfer. Reads of
fields, dereferences, borrows, and assignment targets do not consume their roots.
Each loop `Let` initializes afresh and publishes only on completed evaluation.
No runtime loans, borrow checking, snapshots, or transactions execute.
KED-027 adds private synthetic cleanup described below; public source data remains
resource-free.
A callee's reference slot and caller referent are distinct. Retired IDs are never
reused; references cannot be public return values.

Arguments and record fields evaluate written left to right; unary operands run
first; binary operands run left then right. `&&`/`||` skip the entire unused RHS.
Assignment completes the RHS before publication; reference targets resolve
before the RHS. Selected-field commit preserves lawful RHS mutation to other
fields. Successful prefix effects survive traps and configured stops. A pending
record or assignment is never partly published. Every run starts with fresh state.

Every named-range subtraction widens both signed i64 operands to i128 and checks
the mathematical difference against the canonical inclusive range before forming
a value. Invalid intermediates trap immediately, including under comparisons or
later arithmetic. Raw integers remain distinct from formed nominal values.

## Version-one limits

Every override is positive and at most its supported maximum. Defaults and
accounting are versioned tool contracts, not source semantics. Configuration
arithmetic and counters use checked operations. CLI options are listed below.

| Limit / CLI override | Default | Supported maximum |
| --- | ---: | ---: |
| source bytes / `--max-source-bytes` | 65,536 | 1,048,576 |
| tokens including EOF / `--max-tokens` | 1,024 | 4,096 |
| identifier bytes / `--max-identifier-bytes` | 128 | 256 |
| frontend work / `--max-frontend-work` | 100,000,000,000,000 | 1,000,000,000,000,000,000 |
| traversal depth / `--max-traversal-depth` | 24 | 32 |
| alias-lineage depth / `--max-alias-depth` | 48 | 64 |
| alias-lineage work / `--max-alias-work` | 1,000,000 | 10,000,000 |
| admission storage / `--max-admission-storage` | 134,217,728 | 1,073,741,824 |
| diagnostic storage / `--max-diagnostic-storage` | 1,048,576 | 4,194,304 |
| output bytes / `--max-output-bytes` | 65,536 | 1,048,576 |
| execution steps / `--max-steps` | 1,000,000 | 100,000,000 |
| call depth / `--max-call-depth` | 64 | 256 |
| expression depth / `--max-expression-depth` | 64 | 128 |
| execution storage / `--max-working-storage` | 16,777,216 | 268,435,456 |
| trace entries / `--max-trace-depth` | 16 | 64 |

Admission reserves conservative envelopes **before** unchanged semantic passes,
not after compilation. With actual token count n, source size s, configured
traversal depth d and identifier cap i, frontend work reserves `128*n^4+s`,
working storage reserves `(16*d+32)*n*(i+512)+8*s`, and diagnostic storage reserves
`4*n*(i+128)+512`. Lexer storage is separately reserved as `128*s` before lexing.
These conservative bounds cover repeated table/suffix walks, expression and
record comparisons, simultaneous bounded scopes/checker snapshots, representations,
clones, lowering, and the immutable authority witness. Reservations can stop a
small input despite unused actual capacity; a supported maximum is a ceiling,
not a promise every combination of maxima fits. They bound successful and rejected
paths, including diagnostic construction and disposal. Parser call depth is
checked before recursion; expression depth is checked at construction (including
flat left-associated trees) before later visitors or recursive disposal. MIR
reachability is iterative. Canonical bounded shape checks cover ordinary bodies,
verified contracts, and retained verified-body operands before recursive record
validation, authority comparison, or correspondence validation. This structural
guard neither evaluates proofs nor executes verified bodies or harnesses. Other
retained tables and harness metadata contain scalar identities and values rather
than additional expression subtrees. Alias traversal has separate actual depth and
cumulative work counters shared across checker snapshots; analysis rollback cannot refund
these counters. Token, identifier, input, explicit runtime buffers, trace, and
rendering growth use fallible reservation. Existing bounded semantic collections
remain behind the pre-reserved envelope.

Runtime storage accounts for vector capacities, frame slots, record payloads,
pending arguments, and continuation buffers before growth. Detached record
payloads and receiving value-buffer or destination-slot growth are reserved
together with checked arithmetic before allocation or publication. Parameter and
frame growth are likewise reserved together while argument payloads remain
counted. Record construction and return publication reuse vacated value entries.
Error traces reserve separate bounded capacity of at most 64 entries so exhaustion
can still be reported.
Rendering is bounded before each write, including deterministic truncation. The
CLI has borrowed, fixed emergency diagnostics when even the truncation text
cannot fit or the renderer detects allocation failure; reporting those failures
does not allocate another String or retry ordinary rendering. Host/tool
classification takes precedence if a host/tool failure is detected either by the
original operation or while rendering or writing its diagnostic. Configured
rendering exhaustion remains class 1 only when no host/tool failure has already
occurred. An original host failure followed by rendering allowance exhaustion
uses a static host-identifying truncation message and retains class 2.
Host allocator overhead and fixed stack/error bookkeeping are outside byte-buffer
accounting. Arbitrary allocator abort, external corruption, process kill, and
hardware failure are outside structured reporting; detected reader, allocation,
and writer failures are classified as host/tool failures, not limit exhaustion.

Fuel charges one unit before each statement, terminator (including synthetic
Goto), evaluated expression node, and function entry. Continuation actions do not
add fuel. A skipped RHS has no node, call, mutation, or fuel charge. Boundary
precedence is fuel, then call/expression depth, then storage, then detected host
allocation failure. Wall-clock timeout is not semantic fuel. Limit exhaustion is
not a proof of nontermination. Invalid/zero/overflowing overrides reject before
source ingestion.

## Origins and diagnostics

Each block terminator has an origin produced by lowering. Explicit returns,
breaks, and continues use their statement span. Conditional initialization uses
the enclosing initializer statement; structural branches and synthetic preheaders,
backedges and joins use their enclosing if/while. Implicit completion uses the
function boundary. Statements retain source spans; traps additionally retain the
failing expression. Nested frames retain canonical callees and call-site spans.
Origin correspondence is checked before execution, with missing, malformed and
foreign origins rejected. Traces have deterministic bounded truncation.

Results are `Bool(...)`, named-range `Name(value)`, ordinary records with nominal
field values, or `NoValue`. Errors distinguish frontend/admission/integrity,
named-range traps, configured limits, and detected host/tool failures. They are
tool outcomes, not source-level exceptions, catchable results, effects, or I/O.
Exit status is 0 on success, 1 on frontend/integrity/trap/limit stops, and 2 on
usage/configuration or detected host/tool failures. No host addresses or Rust
runtime diagnostics are emitted by the structured error renderer.

## KED-027 private cleanup extension

The frozen KD-042 extension separates canonical resource identity, owned/reference
identity and Copy/Move classification. Trusted acquisition/release descriptors and
shared acyclic cleanup plans are private. Source record fields still require named
ranges, and the CLI/public data boundary never admits a live resource result.

Resource functions carry explicit lexical MIR Flow and synthesized origins.
Activations retain successful owner-initialization history, exact inner-to-outer
exits and reverse owned-parameter order. Runtime payloads own responsibility;
explicit transfer markers justify skipping former owners, while mere vacancy
does not. A protected return is retained outside callee cleanup, and delivered only
after cleanup and frame retirement complete. Borrowed handles and inert Move
records have no cleanup plan. Unused private catalogs preserve resource-free fuel
and storage behavior.

Pending aggregates evaluate fields in written order and publish only after exact
canonical membership is complete. Complete cleanup and synthetic partial abort
visit reverse canonical declaration order, with abort filtered by initialized
membership. Independent nested pending containers abort inside-out. Moving into a
pending field never restores the old source on failure. This private synthetic
failure facility demonstrates KD-042 and does not add source recoverable propagation
or general unwinding. Unnamed resource arguments without a defined lifetime reject.

Type classification shares child plans and caps its acyclic depth at 32 (also
subject to configured admission traversal depth), record tables at 4,096 and field
tables at 8,192. Execution admission retains the stricter configured token-based
table/node bounds and identifier cap and reserves its expanded frontend envelope.
Cleanup State, activation/history buffers, events, pending fields, protected/current/
detached payloads and work-list capacities participate in checked prospective
working-storage accounting. State is reserved before lazy activation. Detachment
and receiving-container growth are counted together. Error traces and reporting
retain the existing separate bounded diagnostic contract.

In addition to KD-041 charges, fuel is charged before scope entry/exit and owner
inspection, successful initialization recording, pending field progress, aggregate
traversal and fixed trusted operations. Resource-free continuations retain their
previous costs. Release attempts reserve event storage before any effect and record
completion exactly once. Configured exhaustion or controlled host release failure
retains prior effects and all uncompleted payloads, cannot retry, and cannot publish
ordinary success. Disposal of host interpreter state is not a release effect.

The 76 new KD-042 tests cover canonical/malformed authority, structured exits,
transfers/returns, aggregates/pending abort, exact fuel/storage thresholds, terminal
failure/reporting and unique-leaf conservation. GitHub debug and release profiles
also reconcile the 1,581 predecessor identities and each run the 1,800-case affine
cross-check. See `docs/engineering/KED-027-IMPLEMENTATION-LEDGER.md` for executed
evidence; no local executable conformance is claimed during K27-I01.
