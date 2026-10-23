# Bowfin Name Migration Plan — M0 Draft 2

**Status:** Active migration contract. M0 planning and staging setup are authorized. No implementation phase, production merge, repository rename, domain cutover, package publication, or external release is authorized by this document.  
**Prepared:** 2026-10-23  
**Draft 2 owner decisions recorded:** 2026-10-23  
**Repository baseline:** `gregkrsak/klyxr` at `prod` commit `3c0a540df7d5ad75510db4ecad80e27fb4d0903a`, tree `928accaa6c71068a6bce273f73f8c152b4c4d348`  
**Migration integration branch:** `bowfin-migration-staging`

## 1. Objective and controlling principle

Rename the live project from **Klyxr** to **Bowfin** without changing language behavior, weakening assurance, manufacturing authority, corrupting historical evidence, or silently breaking promised consumers.

The migration is an identity change, not a language-feature release. Every changed identity must have a typed, injective correspondence; every compatibility path must name its consumer and authority boundary; and every phase must be independently reviewable and reversible within its stated boundary.

The controlling question remains:

> What new power does this create, where is that power bounded, and can we explain the answer plainly?

For this migration, the required answer is: **no new execution, approval, replay, custody, or resource authority is created by changing or translating a name.**

## 2. Greg’s resolved owner decisions

The following choices bind Draft 2 and later migration work:

1. **Bowfin** and `bowfin` are the final adopted live project names.
2. `KED-###` becomes `BED-###` for live engineering-decision identities. `KD-###` is a distinct namespace and becomes `BD-###` for live language-decision identities. The two families must never be conflated by their numeric suffixes.
3. Existing decision numbers, status, scope, issue association, acceptance history, and original freeze evidence are preserved one-to-one. The next new engineering decision is `BED-037`; the next new language decision is `BD-045` unless a pre-cutover decision legitimately advances either sequence.
4. Project control identities such as `K35-P01` and `K35-I01` become enumerated `B35-P01` and `B35-I01` identities. No generic `K` replacement is permitted.
5. `DP-###`, `OQ-###`, commit hashes, tree hashes, blob hashes, historical payloads, recorded commands, verbatim reports, and unrelated third-party identifiers remain unchanged.
6. `.bow` is the selected canonical Bowfin source extension **conditional on passing the bounded extension gate in this contract**. Until that gate and M3 are accepted, `.klx` remains canonical in production.
7. `bowfin-lang.com` is the primary Bowfin domain. The acquired `.org`, `.net`, `.info`, and `.online` names are secondary redirect candidates. `klyxr.com` and `klyxr.online` remain controlled compatibility entry points.
8. The planned repository destination is `gregkrsak/bowfin`. A future organization transfer requires a separate owner decision.
9. `.klx` input and the `klyxr` command alias remain supported until a future accepted BED explicitly removes them. New documentation and generated output use Bowfin identities after their phase gates.
10. Existing canonical KED and KD files may move to BED and BD filenames and live headings through the manifest. Historical evidence fields retain their original tokens.
11. The compiler repository and the separate AI-governance repository are distinct authority surfaces. Governance migration requires its own explicitly authorized branch and pull request; compiler-repository work cannot silently change governance authority.
12. The current repository tree will contain only the Bowfin Gateway 2000-style advertisement after M5. The Klyxr advertisement is removed from the live tree but remains faithfully recoverable from Git history. It is not retained beside the Bowfin advertisement.

These decisions select the destination. They do not authorize M1 or later implementation.

## 3. Branch and integration policy

`bowfin-migration-staging` is the sole integration target for M0–M7. No phase branch may target `prod`.

| Phase branch | Identity boundary |
| --- | --- |
| `bowfin-m0-migration-contract` | Contract, owner decisions, mapping and gate definitions |
| `bowfin-m1-machine-compatibility` | Machine identifiers and historical compatibility |
| `bowfin-m2-toolchain-identity` | Rust package, crate, binary, and control identities |
| `bowfin-m3-source-extension` | Source extension, examples, and tooling hooks |
| `bowfin-m4-docs-governance` | Decisions, documentation, AI context, and coordinated governance work |
| `bowfin-m5-visual-identity` | Logo, banner, Gateway advertisement, and ephemera metadata |
| `bowfin-m6-public-cutover` | External-cutover preparation only; no live switch before production acceptance |
| `bowfin-m7-stabilization` | Exact-candidate stabilization, evidence, and release recommendation |

All eight branches were initially parked at the baseline above. Before work begins on M1–M7, the phase branch must be refreshed from the current accepted staging head. Its PR must prove:

- its base is the exact accepted staging commit and tree;
- its head descends from that base;
- its target is `bowfin-migration-staging`, never `prod`;
- the resulting integration tree is recorded and independently checked;
- no accepted staging work was replaced by a similar-looking cherry-pick; and
- any intervening `prod` change was incorporated into staging and re-reviewed before dependent work continued.

A migration-specific CI gate must reject an M0–M7 PR whose base is `prod`. The only permitted staging-to-`prod` PR is the separately identified final production-candidate PR, and only a human may merge it.

## 4. Namespace-qualified identity contract

### 4.1 Required mapping families

| Kind | Legacy identity | Canonical identity | Policy |
| --- | --- | --- | --- |
| Display brand | `Klyxr` | `Bowfin` | Replace on live surfaces |
| Lowercase brand | `klyxr` | `bowfin` | Replace on live surfaces |
| Uppercase brand | `KLYXR` | `BOWFIN` | Replace on live surfaces |
| Engineering decision | `KED-###` | `BED-###` | Typed one-to-one map |
| Language decision | `KD-###` | `BD-###` | Separate typed one-to-one map |
| Project control | `K##-*` | `B##-*` | Enumerated one-to-one map; never inferred globally |
| Test identity | Project-specific legacy test name | Enumerated Bowfin test name | One predecessor maps to exactly one successor unless an accepted report explains otherwise |
| CLI executable | `klyxr` | `bowfin` | Canonical command plus retained compatibility alias |
| Rust package | `klyxr-compiler` | `bowfin-compiler` | Intentional Cargo package-name break; repository is not registry-published |
| Rust crate path | `klyxr_compiler` | `bowfin_compiler` | Update source, tests, doctests, and harnesses together |
| Source suffix | `.klx` | `.bow` after gate | Legacy input retained; canonical output becomes `.bow` |
| Markdown fence | `klyxr` | `bowfin` | Documentation/tooling identity only |
| Configuration name | `klyxr.toml` | `bowfin.toml` | Reserved names only; no configuration loader currently exists |
| Machine namespace | `klyxr.*` | `bowfin.*` | Versioned, field-specific mapping; never treated as authority |
| Compiler identity | `klyxr-compiler/<version>` | `bowfin-compiler/<version>` | Permitted provenance change with explicit correspondence |
| Environment variable | Project-prefixed `K...` names, including `K34_EVIDENCE_DIR` | Individually selected `B...` names | Enumerate by exact spelling and consumer |
| Filename/script stem | `KLYXR`, `klyxr`, `KED`, `ked`, `KD`, or `kd` forms | Typed Bowfin/BED/BD form | Rename only after occurrence classification |
| AI context | `KLYXR_CONTEXT.md`, `KLYXR_AI_CONTEXT_BUNDLE.md` | `BOWFIN_CONTEXT.md`, `BOWFIN_AI_CONTEXT_BUNDLE.md` | Rename canonical inputs, generator, and generated output atomically |
| Repository | `gregkrsak/klyxr` | `gregkrsak/bowfin` | External cutover only |
| Primary domain | `klyxr.com` | `bowfin-lang.com` | External cutover only; old domain retained |
| Historical advertisement | `docs/ephemera/klyxr-1994-ad.jpeg` | `docs/ephemera/bowfin-1994-ad.jpeg` | Replace in live tree; original remains in Git history only |

The `KLX` monogram and every spelling or case variant found in the repository or external inventory must receive an explicit row before implementation. `DP`, `OQ`, unrelated variables, units, prose letters, third-party identifiers, and externally defined names remain unchanged unless separately enumerated and accepted.

### 4.2 Manifest schema and validity

Before M1, M0 must produce or require a machine-readable manifest whose every entry records:

- namespace kind;
- original identity and source location;
- canonical identity or intentional retention;
- occurrence role: live, compatibility, or historical;
- owning migration phase;
- consumer and producer, when machine-readable;
- support/removal policy;
- validation or test identity; and
- historical evidence relationship, when applicable.

Mappings must be injective within each namespace. Duplicate keys, duplicate canonical values that would conflate distinct identities, untyped numeric-only correspondence, unknown kinds, and ambiguous occurrence roles are errors.

A rename preserves the decision or control’s status, scope, issue association, and acceptance history. It does not create a new decision, repeat a freeze, or rewrite an old evidentiary claim.

### 4.3 Field-level occurrence roles

Classification applies to individual structured fields and occurrences, not merely to whole files or directories:

1. **Live** fields become Bowfin identities.
2. **Compatibility** fields retain or accept a legacy representation according to the per-surface matrix.
3. **Historical** fields remain byte-truthful when they record prior hashes, commands, outputs, control identities, reports, or measurements.

A JSON document may legitimately contain all three roles. Verifiers must not become green by rewriting both a historical expectation and the observation it was meant to check.

Original historical payloads and digests remain untouched. A new live adapter or manifest may record a correspondence to them, but may not recompute an old claim under new spelling and present it as the original evidence.

## 5. Inventory and completeness gate

The baseline inventory found:

- 276 tracked files;
- 87 text files and 883 occurrences containing `Klyxr`, `klyxr`, or `KLYXR`;
- 47 files and 713 occurrences containing KED forms;
- 17 files and 122 occurrences containing known K-control forms;
- 75 `.klx` files;
- 49 `klyxr` Markdown fences;
- 14 verification/build scripts containing known legacy literals;
- 8 old-domain references; and
- known `klyxr.*` and `klyxr-compiler/<version>` machine identities.

These counts are a lower bound, not completion evidence. Before M1, the exhaustive inventory must additionally cover:

- all `KD`/`kd`, `KLX`, environment-variable, filename, script, test, package, crate, binary, configuration, and case-variant families;
- structured-field roles inside evidence and predecessor records;
- generated context inputs and outputs;
- repository settings, workflows, templates, releases, tags, badges, raw links, submodules, Pages/custom domains, and hosted-Action consumers that actually exist;
- the separate governance repository and its canonical charter/instructions;
- domains, package registries, editor/tooling identifiers, app-store or social identities that actually exist; and
- raster/vector text by manual inspection or OCR.

Nonexistent integrations are marked **not applicable**. The migration does not create a configuration system, package release, editor architecture, website, or public replay format merely to satisfy an inventory checklist.

The inventory gate passes only when an independently reviewed occurrence manifest accounts for every discovered legacy occurrence or records a bounded scanner limitation. Historical allowlists may contain exact paths, fields, and reasons; broad directory or wildcard exemptions are prohibited.

## 6. Per-surface compatibility and authority matrix

| Surface | Existing producer/consumer | Accepted input | Canonical output | Conflict or rejection behavior | Authority boundary and support |
| --- | --- | --- | --- | --- | --- |
| Source path | CLI opens the explicitly supplied path; no sibling discovery exists | `.klx` and, after M3, `.bow` | Docs/generated examples use `.bow` | No same-stem conflict exists because no discovery occurs; future discovery requires a separate BED | File suffix grants no authority; `.klx` remains until a future BED removes it |
| CLI command | Installed/build binary invoked by user or tests | `bowfin`; retained `klyxr` alias after M2 | Help/version identify Bowfin; legacy alias help/version identifies itself as compatibility | Normal execution adds no unsolicited stdout or stderr banner; arguments, exit status, and result streams remain equivalent | Alias delegates to the same implementation and remains until a future BED removes it |
| Cargo package | Workspace Cargo manifests; currently `publish = false` | Repository manifests are updated to `bowfin-compiler` | `bowfin-compiler` | Direct consumers requesting package `klyxr-compiler` must update; a CLI shim does not claim Cargo compatibility | No registry publication is implied or authorized |
| Rust crate | Repository source, doctests, and tests | `bowfin_compiler` after M2 | `bowfin_compiler` | Legacy crate-path references fail and receive migration documentation | Internal source identity only; no authority change |
| Configuration filename | No configuration loader currently exists | None | None | `klyxr.toml` and `bowfin.toml` are reserved spellings, not accepted runtime inputs | Migration must not invent configuration loading or discovery |
| Machine/provenance identifiers | Trusted catalogs, profiles, targets, compiler IDs, assumptions, audit and evidence records | Exact legacy values only in declared archival/compatibility readers; exact Bowfin values in current producers | New records use accepted versioned `bowfin.*` identities | Unknown values reject; mixed values reject unless a field-level manifest entry and existing validation prove one coherent record | Translation is descriptive only and cannot establish approval, grants, custody, replay identity, schema validity, or execution permission |
| Approval-bearing correspondence | Existing `Contract::same_approval` and associated validators | The same original opaque approval required by the governing implementation | No synthetic approval output | Independently compiled equality or spelling equality is insufficient and rejects | The migration must not weaken same-original-approval semantics |
| Private addressed replay | Existing bounded conformance mechanism | Only its already-authorized private inputs | Existing private results with mapped presentation where accepted | No public serialized replay import is created | Reading or translating a name cannot import resource authority or recover executable state |
| Historical evidence | Existing evidence/verifier consumers | Original historical bytes plus explicit mapping metadata where needed | New evidence records Bowfin identity and its predecessor relationship | Digest mismatch, unknown mapping, duplicate mapping, or role ambiguity rejects | Historical inspection is not executable admission |
| Decisions and controls | Human and script consumers | Historical legacy identities and live mapped identities in their declared roles | Live references use BED/BD/B identities | Numeric suffix alone is never sufficient; cross-namespace ambiguity rejects | Mapping preserves status/history and grants no new authority |
| Repository/domain URLs | Git clients, badges, raw links, Pages, hosted Actions, web users where applicable | Tested old redirects and new canonical URLs after cutover | New URLs | Each facility is tested separately; successful Git redirect does not prove Pages, raw links, or hosted Actions | Routing does not authorize code or release state |

Global prohibition: **name translation cannot manufacture, recover, merge, renew, or substitute executable approval, grants, operation association, replay identity, custody, schema validity, or resource authority.** No persisted executable authority, public replay facility, source-discovery system, or configuration facility enters through this migration.

## 7. Permitted observable-difference relation

“Rename-only” does not mean byte-identical presentation. The migration compares old and new behavior through the following reviewed relation.

### 7.1 Must remain unchanged

- language acceptance except for the separately gated source filename surface;
- nominal typing, ownership, borrowing, movement, and resource-bearingness;
- grants, actual resource responsibility, and approval requirements;
- effect and cleanup order;
- execution results and terminal classification;
- non-resumption behavior;
- logical execution charges and bounded-execution semantics;
- diagnostic class, causal meaning, and source span except for explicitly mapped path or source-text displacement; and
- predecessor coverage, except for enumerated one-to-one identity changes.

### 7.2 Permitted only when enumerated

- displayed project, package, crate, command, decision, control, test, path, URL, provenance, and machine-namespace spelling;
- byte length of rendered identity-bearing text;
- source-origin offsets caused solely by reviewed edits to comments or branding text;
- help/version output for the canonical command and retained alias; and
- actual-byte boundary results that differ solely because the honestly rendered Bowfin spelling has a different byte length.

Actual-byte limits remain honestly enforced. The implementation may neither undercharge Bowfin bytes nor silently raise a production limit to force old and new outcomes to match. For every affected boundary, evidence records the legacy sufficient/insufficient neighbors and the mapped Bowfin neighbors separately, preserving original measurements.

### 7.3 Forbidden normalization

No global normalization may erase or ignore authority fields, custody events, failures, byte-limit behavior, unknown differences, or unmapped identifiers. Migration differential evidence uses an explicit migration relation and independent expected results; it must not weaken same-original-approval by comparing independently compiled old and new programs as though spelling equality established approval identity.

## 8. Phase plan and gates

### M0 — Migration contract

Deliver this contract, the namespace-qualified manifest requirements, the occurrence-inventory gate, the compatibility matrix, the permitted-difference relation, and the release state machine. Sarah or another independent reviewer recommends disposition; Greg alone authorizes freeze.

**Exit:** all major contract findings resolved; owner decisions recorded; complete-manifest work is either included in M0 or is a mandatory independently reviewed precondition that blocks M1.

### M1 — Machine compatibility

Implement exact old/new handling for the machine and evidence surfaces that actually exist. Add legacy fixtures, field-role validation, mapping injectivity checks, conflict rejection, and tests proving that translation cannot establish authority.

**Must not include:** CLI, source-file, documentation, branding, domain, or repository renames.

**Exit:** required historical inputs remain readable without changed meaning; new machine output is unambiguously Bowfin; replay, digest, approval, and custody consequences are documented and tested.

### M2 — Toolchain identity

Rename the Rust package, crate, and canonical binary; add the retained command alias; update Cargo metadata, lockfile, imports, doctests, harness variables, help/version text, and project-control identities through the manifest. Supporting verifier changes required to keep the phase testable travel with this phase.

**Exit:** debug and release suites account for every predecessor; alias result streams do not diverge; only mapped identities change.

### M3 — Source extension and tooling surface

Complete the bounded `.bow` collision matrix, record its limitations, and—only if it passes—rename canonical examples and all exact consumers. Continue explicit `.klx` input. Update fences and existing editor/tooling hooks; do not invent absent infrastructure.

The extension matrix covers Windows, macOS, Linux, common archive/antivirus/DLP/email behavior available for testing, GitHub/GitLab, VS Code, Sublime Text, tree-sitter, MIME/web-server defaults, and known format/tool collisions. It does not claim universal clearance.

**Exit:** canonical sources use `.bow`; promised legacy input works; the full corpus parses, verifies, and executes under independently expected results.

### M4 — Decisions, documentation, and governance

Rename live KED→BED and KD→BD files and references through the typed map. Update AI context sources, generator, generated bundle, README, repository structure, documentation, Sentinel 0 guide, templates, code fences, scripts, and workflows. Preserve exact historical fields. The new checked-in context bundle must reproduce byte-for-byte from the new canonical inputs; it is not expected to equal the old bundle.

Coordinate a separately authorized governance-repository branch and PR. Record its commit/tree and accepted relationship to the compiler staging candidate. Neither repository silently confers authority on the other.

**Exit:** no unapproved live Klyxr/KED/KD/project-K identities remain in covered text surfaces; the governance dependency is explicitly reconciled.

### M5 — Visual identity

Replace live wordmarks and banners. Create `docs/ephemera/bowfin-1994-ad.jpeg`; remove `docs/ephemera/klyxr-1994-ad.jpeg` from the current tree. Git history remains the sole repository preservation of the original Klyxr raster.

The Bowfin advertisement must contain only Bowfin-era branding and commands. Any quantitative test claim is tied in the ephemera index to an exact tested commit/tree and workflow evidence. M7 revalidates the claim against the production candidate; if it is no longer true, the advertisement is regenerated before that candidate is fixed. OCR plus manual inspection verifies raster text.

**Exit:** the live tree contains one coherent Bowfin advertisement and no live Klyxr visual branding.

### M6 — External-cutover preparation

Prepare—but do not execute—the domain, repository, redirect, badge, raw-link, Pages/custom-domain, hosted-Action, clone, release, and announcement runbook. Inventory only facilities that exist. Prove new domain destinations in a noncanonical or otherwise non-disruptive state where practical.

**Exit:** every external action has a target, prerequisite, verification, and recovery step. Public routing and repository identity remain unchanged.

### M7 — Stabilization and exact candidate

Run the full debug/release/format/docs/static-verification matrix, predecessor reconciliation, denylist/allowlist scan, legacy-input checks, clean clone/install, Sentinel 0, context generation, image inspection, and external runbook rehearsal on an exact staging head/tree. Publish the final migration ledger and independent recommendation.

**Exit:** Greg may accept or reject the exact production candidate. Review recommends; it does not authorize.

## 9. Release and cutover state machine

| State | Required transition | Prohibited before transition completes |
| --- | --- | --- |
| R0 — M0 frozen | Independent recommendation plus Greg’s explicit freeze | M1 implementation |
| R1 — Phases integrated | M1–M6 individually reviewed and accepted into staging from exact predecessors | Direct phase merge to `prod`; live external cutover |
| R2 — Candidate fixed | M7 records exact staging commit/tree, mapping version, tests, exceptions, artwork evidence, and runbook | Candidate drift without renewed evidence |
| R3 — Recommended | Independent review recommends the exact R2 candidate | Treating review as owner authority |
| R4 — Production accepted | Greg explicitly accepts; a human merges the sole staging-to-`prod` PR | Repository rename, canonical DNS switch, public Bowfin release claim |
| R5 — Production verified | Exact merged `prod` commit/tree passes required post-merge checks | External cutover |
| R6 — Cutover authorized | Greg separately authorizes the external runbook | Any external write not named in that authorization |
| R7 — Public cutover | Canonical Bowfin routing, repository rename, redirects, metadata, and announcement are executed in recorded order | Declaring completion before clean-environment verification |
| R8 — Post-cutover verified | Old and new entry points, CI, issues/PRs, releases/tags, raw links, badges, domains, and applicable consumers pass | Removal of compatibility surfaces without a future BED |

An alias-first public preview is allowed only by a separate owner decision that plainly states production is still Klyxr and makes no premature Bowfin-tool availability claim.

## 10. Verification and evidence requirements

Every phase report records:

- accepted staging base commit/tree;
- reviewed phase head/tree;
- mapping-manifest version and digest;
- exact identity boundary and field roles changed;
- historical exceptions;
- tests, workflows, and independent expected results;
- resulting staging integration tree;
- content rollback and external-recovery procedures; and
- confirmation that no unrelated feature or authority work occurred.

Completion additionally requires:

- all predecessor identities preserved or mapped one-to-one with no unexplained migration;
- zero unapproved live legacy identities under case-insensitive and typed scans;
- no broad historical allowlist wildcard;
- all promised legacy inputs verified;
- actual-byte limits tested honestly at mapped neighbors;
- context generation reproducible from canonical inputs;
- Sentinel 0 behavior preserved under the permitted-difference relation;
- clean clone/install and all repository workflows green on exact head;
- raster and vector branding manually inspected; and
- external facilities tested individually rather than inferred from Git redirect behavior.

## 11. Rollback and recovery

Repository-content rollback and external-state recovery are different operations.

Before R4, an accepted phase may be reverted from staging using its recorded integration boundary, followed by dependent-phase invalidation and re-review. A revert cannot be described as restoring an external or emitted state that never changed.

After R4 but before R7, production content may be forward-repaired or reverted under a separately reviewed plan. No public cutover has yet occurred.

After R7:

- do not recreate or reuse the old GitHub repository name;
- retain old domains and required redirects;
- prefer a forward repair over repeated repository renames;
- preserve both names’ routing and historical access during a critical-release disablement;
- separately recover DNS, repository metadata, badges, hosted services, cached pages, announcements, and emitted artifacts where applicable; and
- never claim that reverting Git content retracts an installed binary, downloaded artifact, cached page, published package, or previously issued runtime authority.

No package publication is part of this plan without separate authorization.

## 12. Point-in-time external findings

- Greg acquired `bowfin-lang.com`, `.org`, `.net`, `.info`, and `.online` on 2026-10-23. Registrar hardening and DNS work remain operational tasks, not evidence of cutover.
- `gregkrsak/bowfin` returned not found during the preliminary check; that is not a reservation guarantee.
- Other Bowfin-named repositories exist, but no programming language/compiler collision was identified in the preliminary search.
- The unscoped npm package `bowfin` exists at version `0.0.0`; future JavaScript tooling uses a controlled scope if separately authorized.
- PyPI returned no exact `bowfin` project during the preliminary check.
- GitHub Linguist did not list `.bow`, `.bfn`, or `.bwfn` during the preliminary check.

These findings are not legal clearance and must be rechecked at the phase that relies upon them.

## 13. Immediate next move

Submit M0 Draft 2 for focused independent confirmation against findings `BOW-M0-01` through `BOW-M0-04`. If the reviewer recommends freeze, Greg decides whether to freeze M0. M1 remains unauthorized until that decision.

