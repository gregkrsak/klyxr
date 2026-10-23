# Bowfin Name Migration Plan — M0 Draft 1

**Status:** Active migration plan; M0 planning and staging setup authorized on 2026-10-23. No production rename or merge is authorized by this document.  
**Prepared:** 2026-10-23  
**Repository baseline inspected:** `gregkrsak/klyxr` at `prod` commit `3c0a540df7d5ad75510db4ecad80e27fb4d0903a`  
**Working premise:** Bowfin is the selected migration candidate. Klyxr remains the canonical production identity until the adoption gate in Phase 0 is passed.\n**Domain reservation:** `bowfin-lang.com`, `.org`, `.net`, `.info`, and `.online` were acquired by Greg on 2026-10-23. Registrar hardening and DNS configuration remain pending.

## 1. Objective

Rename the project from **Klyxr** to **Bowfin** without changing language behavior, weakening assurance, corrupting historical evidence, or silently breaking users and tools.

The migration is an identity change, not a feature release. Each implementation pull request should be reviewable as a bounded transformation with explicit predecessor correspondence.

## 2. Branch and integration policy\n\nThe migration uses `bowfin-migration-staging` as its sole integration target. Phase branches never target `prod` directly. Each phase PR must target staging, pass its phase gate, and be accepted before the next dependent branch is updated from staging. The final staging-to-`prod` PR is a separate human merge decision after full migration review.\n\nReserved branch set:\n\n- `bowfin-m0-migration-contract`\n- `bowfin-m1-machine-compatibility`\n- `bowfin-m2-toolchain-identity`\n- `bowfin-m3-source-extension`\n- `bowfin-m4-docs-governance`\n- `bowfin-m5-visual-identity`\n- `bowfin-m6-public-cutover`\n- `bowfin-m7-stabilization`\n\nAll branches were cut from production commit `3c0a540df7d5ad75510db4ecad80e27fb4d0903a`, tree `928accaa6c71068a6bce273f73f8c152b4c4d348`. The later phase branches are parking branches only until refreshed from the accepted staging head immediately before their work begins.\n\n## 3. Proposed identity map

| Surface | Current | Proposed | Treatment |
| --- | --- | --- | --- |
| Display name | `Klyxr` | `Bowfin` | Replace on live/normative surfaces |
| Lowercase name | `klyxr` | `bowfin` | Replace on live/normative surfaces |
| Project initial | `K` | `B` | Replace only in project-scoped identifiers; never globally replace the letter K |
| Engineering decisions | `KED` | `BED` | Expand as **Bowfin Engineering Decision** |
| Decision example | `KED-035` | `BED-035` | One-to-one mapped identity |
| Control example | `K35-P01` / `K35-I01` | `B35-P01` / `B35-I01` | Enumerated one-to-one migration, never inferred by a blind replacement |
| CLI executable | `klyxr` | `bowfin` | New canonical command; temporary legacy shim considered |
| Rust package | `klyxr-compiler` | `bowfin-compiler` | Cargo metadata and lockfile migration |
| Rust crate path | `klyxr_compiler` | `bowfin_compiler` | Source, tests, doctests, and test identities |
| Source extension | `.klx` | `.bow` (provisional) | Finalize at Phase 0 gate; preserve legacy reading |
| Markdown fence | `klyxr` | `bowfin` | Update examples and future syntax grammar registration |
| Tool config | `klyxr.toml` (provisional today) | `bowfin.toml` | Finalize before the old name becomes a public contract |
| Machine namespace | `klyxr.*` | `bowfin.*` | Versioned compatibility migration; never blind replacement |
| Repository | `gregkrsak/klyxr` | `gregkrsak/bowfin` | Rename only at public cutover |
| Primary domain | `klyxr.com` | `bowfin-lang.com` (provisional) | Register first; cut over later; retain redirects |
| Secondary domain | `klyxr.online` | To be decided | Retain during transition; do not let lapse accidentally |
| AI context files | `KLYXR_CONTEXT.md`, `KLYXR_AI_CONTEXT_BUNDLE.md` | `BOWFIN_CONTEXT.md`, `BOWFIN_AI_CONTEXT_BUNDLE.md` | Rename generator inputs and generated output together |
| Historical advertisement | `klyxr-1994-ad.jpeg` | Add `bowfin-1994-ad.jpeg` | Regenerate; preserve the original as historical ephemera |

## 4. Current repository exposure

The inspected baseline contains 276 tracked files. The initial inventory found:

- 87 text files containing `Klyxr`, `klyxr`, or `KLYXR`;
- 883 textual occurrences of those spellings;
- 47 files containing `KED-###` or `ked-###`, with 713 occurrences;
- 17 files containing project control identities such as `K35-P01` or `K35-I01`, with 122 occurrences;
- 75 `.klx` source/example files;
- 49 Markdown fences tagged `klyxr`;
- 14 verification/build scripts containing Klyxr or KED identity literals;
- 8 textual references to `klyxr.com` or `klyxr.online`;
- machine-visible identifiers including `klyxr.sensor-demo.catalog.v1`, `klyxr.sensor-demo.profile.v1`, `klyxr.ordinary-mir.v1`, `klyxr.sensor-demo.assumptions.v1`, and `klyxr-compiler/<version>`.

This inventory is a lower bound. Phase 1 must also inspect issue/PR templates, release artifacts, branch rules, repository metadata, local documentation, package registries, editor integrations, cached examples, image text, and any external site or social profile.

## 5. Governing migration rules

### 5.1 Three classes of material

Every occurrence must be classified before it changes:

1. **Live normative identity** — current code, commands, documentation, filenames, headings, examples, metadata, generated context, and public branding. These become Bowfin.
2. **Compatibility identity** — old source extensions, CLI names, serialized identifiers, configuration names, and machine-readable formats that existing tools or evidence may still consume. These receive an explicit legacy-read/new-write policy.
3. **Historical evidence** — Git history, commits, merged PRs, issue comments, published reports, recorded command output, old URLs, hashes, and verbatim evidence. These remain truthful and are not cosmetically rewritten. A mapping document explains their relationship to Bowfin.

### 5.2 No blind global replacement

`K` becomes `B` only when `K` is a project identity prefix. Generic letters, variable names, units, prose words, third-party names, and externally defined identifiers are out of scope.

### 5.3 Preserve semantic behavior

Rename pull requests may add compatibility code and migration tests, but may not add unrelated language features or alter ownership, authority, boundedness, verification, execution, or diagnostic semantics.

### 5.4 Preserve evidence and replay

Machine identifiers may participate in hashes, evidence correspondence, replay, or custody. Old `klyxr.*` values must not be silently rewritten into `bowfin.*` values. The migration must define whether each value is:

- accepted forever as legacy input;
- translated through an explicit, tested one-to-one map;
- emitted only under the new namespace; or
- intentionally incompatible, with a documented error.

The preferred default is **legacy read, Bowfin write**.

### 5.5 One identity boundary per pull request

Do not combine the repository rename, crate rename, extension rename, documentation rewrite, and image regeneration into one giant change. Each phase has its own verification gate and rollback point.

## 6. Phase plan

### Phase 0 — Final decision and reservation gate

**Purpose:** Do not begin an expensive migration until the durable public identity is settled.

Actions:

1. Adopt the display name `Bowfin` and lowercase form `bowfin`, or stop.
2. Complete a second-stage name search covering USPTO, WIPO/EUIPO as practical, companies, software products, GitHub, package registries, app stores, and defense/aerospace usage.
3. Register `bowfin-lang.com` before publicly announcing the rename. Consider defensive registration of one affordable adjacent name only if useful.
4. Decide whether the GitHub destination remains `gregkrsak/bowfin` or later moves to a `bowfin-lang` organization.
5. Finalize the source extension.
6. Record package-namespace choices. The unscoped npm name `bowfin` is already occupied; use a scoped namespace if JavaScript tooling is ever published.
7. Freeze the identity map in a migration decision document.

#### `.bow` extension gate

`.bow` is memorable and semantically excellent, but it is not collision-free. It has older associations with BO2K data and bag-of-words formats. GitHub Linguist does not currently list `.bow`, `.bfn`, or `.bwfn` as a recognized language extension.

Before choosing `.bow`, test:

- Windows, macOS, and Linux file association behavior;
- common archive, antivirus, DLP, and email-gateway treatment;
- GitHub, GitLab, VS Code, Sublime Text, and tree-sitter integration behavior;
- MIME-type behavior and web-server defaults;
- collisions in package fixtures and existing developer tools.

Decision candidates:

| Extension | Strength | Risk |
| --- | --- | --- |
| `.bow` | Memorable, short, on-brand | Existing obscure formats; old BO2K association |
| `.bfn` | Short and likely lower collision | Less immediately legible |
| `.bwfn` | Highly distinctive | Awkward to type and say |
| `.bowfin` | Unambiguous | Long for routine source files |

**Exit gate:** name, domain, repository destination, extension, and namespace policy are explicitly accepted. No unresolved high-severity collision remains.

### Phase 1 — Migration contract and exhaustive inventory

**Purpose:** Define exactly what will change before changing it.

Deliverables:

- A canonical rename manifest containing every old/new token, filename, test identity, machine identifier, and exception.
- A historical-identity map: `KED-###` ↔ `BED-###`, `K##-*` ↔ `B##-*`, and Klyxr/Bowfin terminology.
- A generated inventory report grouped by code, tests, docs, CI, artifacts, images, repository settings, and external services.
- An allowlist for historical Klyxr literals that must remain.
- A denylist verifier that fails CI when unapproved live Klyxr identifiers are introduced.
- A rollback checklist for every later phase.

Important policy decision:

- Existing canonical decision files should move to `BED-###` names and headings.
- Historical evidence payloads, old commands, hashes, old PR/issue URLs, and verbatim reports should retain their original Klyxr/KED strings.
- Every intentional predecessor test-identity rename must be enumerated one-to-one; it is not permitted to disappear as unexplained test churn.

**Exit gate:** independent review agrees the manifest is complete enough to implement and that historical exceptions are bounded.

### Phase 2 — Compatibility boundary and machine identities

**Purpose:** Make the toolchain capable of crossing the name boundary safely before moving user-facing files.

Actions:

1. Define old/new handling for trusted catalog, profile, target, assumptions, compiler, audit, replay, and custody identifiers.
2. Add compatibility fixtures containing the old `klyxr.*` namespace.
3. Prove Bowfin can read required historical artifacts without silently changing their meaning.
4. Make new outputs use the accepted `bowfin.*` namespace.
5. Add explicit diagnostics for unsupported or ambiguous legacy inputs.
6. Decide whether `klyxr.toml` is accepted as a legacy config filename and whether simultaneous old/new configs are an error.

**Exit gate:** old evidence remains readable as required; new evidence is unambiguously Bowfin; digest/replay/custody consequences are documented and tested.

### Phase 3 — Toolchain and code identity

**Purpose:** Rename executable and Rust identities without yet performing the public repository cutover.

Actions:

1. Rename Cargo package `klyxr-compiler` to `bowfin-compiler`.
2. Rename Rust crate path `klyxr_compiler` to `bowfin_compiler`.
3. Rename the canonical binary from `klyxr` to `bowfin`.
4. Update Cargo.lock, doctests, imports, test harness environment variables, CLI help/version output, temp paths, and diagnostics.
5. Provide a temporary `klyxr` command shim only if the compatibility policy requires it. The shim must identify itself as legacy and delegate without behavioral divergence.
6. Replace project-scoped K control identities with B identities through the frozen map.
7. Add tests proving K/B identity correspondence and detecting accidental semantic changes.

**Exit gate:** debug and release suites pass; every predecessor test is accounted for; only explicitly mapped identity migrations occur.

### Phase 4 — Source extension, examples, and editor/tooling surface

**Purpose:** Move the language’s source-file identity.

Actions:

1. Rename all 75 canonical example files from `.klx` to the accepted Bowfin extension.
2. Update `include_str!` paths, CLI tests, diagnostic golden strings, workflows, scripts, documentation commands, and Sentinel examples.
3. Continue reading `.klx` as a legacy input unless Phase 0 explicitly chooses a hard break.
4. Emit the new extension in generated examples and documentation.
5. Decide conflict behavior when equivalent old/new files coexist.
6. Create or update syntax highlighting, file icons, editor snippets, language-server identifiers, and GitHub Linguist planning material.
7. Add a corpus equivalence gate proving renamed sources parse, verify, and execute identically.

**Exit gate:** all canonical examples use the new extension; legacy behavior matches policy; diagnostics and tooling use Bowfin consistently.

### Phase 5 — Governance, documentation, and generated context

**Purpose:** Make the written project identity coherent without falsifying its past.

Actions:

1. Rename current decision documents and data files from KED/ked to BED/bed as authorized by the migration manifest.
2. Update headings and live cross-references to Bowfin/BED/B control identities.
3. Preserve historical evidence strings and annotate them through the identity map rather than rewriting them.
4. Rename AI context source and bundle files; update `AGENTS.md`, Claude/Gemini/Copilot instructions, bundle generation, and readback verification.
5. Update README, repository structure, documentation policy, Sentinel guide, language decisions, open questions, templates, code fences, and examples.
6. Update every verification script and workflow label from KED/Klyxr to BED/Bowfin where the string is live rather than historical.
7. Add `docs/migration/KLYXR-TO-BOWFIN.md` describing compatibility, old identifiers, new identifiers, and user action.
8. Define the next decision number. If the sequence is preserved, the first new decision after KED-036 becomes BED-037.

**Exit gate:** generated context rebuilds byte-for-byte; no unapproved live Klyxr/KED/K identifiers remain; historical exceptions match the allowlist.

### Phase 6 — Visual identity and ephemera

**Purpose:** Replace public visual identity while preserving project history.

Actions:

1. Finalize the Bowfin mascot/logo direction and production variants.
2. Replace the README banner and any embedded Klyxr wordmarks.
3. Regenerate the Gateway 2000-style advertisement as `docs/ephemera/bowfin-1994-ad.jpeg`.
4. Update the advertisement’s wordmark, commands, source extension, and test count using the actual cutover head—not today’s remembered count.
5. Preserve `docs/ephemera/klyxr-1994-ad.jpeg` as a labeled pre-rename historical artifact unless Greg explicitly authorizes its removal.
6. Add an ephemera index recording provenance, dimensions, size, generation/edit history, and non-normative status.
7. Inspect image text manually or by OCR; grep cannot detect stale Klyxr text inside raster images.

**Exit gate:** all live visual surfaces say Bowfin; historical artwork is clearly contextualized; no normative claim is embedded only in an image.

### Phase 7 — External namespace and repository cutover

**Purpose:** Change the public address only after the repository itself is internally ready.

Recommended order:

1. Configure `bowfin-lang.com` and prove the destination works.
2. Configure redirects from `klyxr.com` and `klyxr.online`; retain the old domains for a defined minimum period.
3. Merge all internal rename phases while the repository is still `gregkrsak/klyxr`.
4. Quiesce merges briefly and record the exact cutover commit/tree.
5. Rename the repository to `gregkrsak/bowfin`.
6. Immediately update badge URLs, clone commands, repository metadata, homepage, topics, workflow references, local remotes, and external links.
7. Verify issues, PRs, releases, tags, Actions, raw links, submodules, Pages/custom domains, and any consumers of repository-hosted Actions.
8. Never create a new repository at the old `gregkrsak/klyxr` path; doing so would defeat GitHub’s redirect.
9. Publish a concise rename notice: “Klyxr is now Bowfin; language intent and assurance commitments are unchanged.”

**Exit gate:** clone/fetch/push, badges, CI, documentation, domain routing, and issue/PR links work from both expected old and new entry points.

### Phase 8 — Stabilization and final freeze

**Purpose:** Observe the renamed project before removing temporary scaffolding.

Actions:

1. Run the full debug/release/format/docs/static-verification matrix on exact head.
2. Compare the complete test-identity inventory with the pre-migration baseline and rename manifest.
3. Verify both new canonical input and every promised legacy input.
4. Search case-insensitively for `Klyxr`, `klyxr`, `KLYXR`, `KED`, `.klx`, project-scoped `K##-*`, old domains, old crate names, and old binary names.
5. Classify every remaining occurrence as an approved historical/compatibility exception.
6. Exercise a clean install, a fresh clone, Sentinel 0, documentation generation, and the AI handoff bundle.
7. Keep the legacy readers required for historical evidence. Remove only temporary shims whose removal criteria were defined in Phase 1.
8. Publish a final migration ledger with head, tree, test counts, correspondence, exceptions, and external checks.

**Exit gate:** independent review authorizes freeze; Greg accepts; Bowfin becomes the sole live project identity.

## 7. Suggested pull-request sequence

| PR | Scope | Must not include |
| --- | --- | --- |
| M0 | Migration decision, identity map, inventory, allowlist/denylist design | Production renames |
| M1 | Machine-identifier compatibility and historical fixtures | CLI, files, branding |
| M2 | Rust package, crate, binary, and B-control identities | Repository/domain rename |
| M3 | Source extension, examples, tests, editor/tooling hooks | Governance rewrite |
| M4 | BED documents, docs, AI context, scripts, generated bundle | Visual assets, repo rename |
| M5 | Logo, banner, Gateway advertisement, ephemera index | External cutover |
| M6 | Domain and GitHub repository cutover | Language feature work |
| M7 | Stabilization ledger and authorized cleanup | Unmapped compatibility removal |

Every PR should state:

- authoritative base commit and tree;
- exact identity boundary being migrated;
- old/new mappings;
- historical exceptions;
- test and workflow results;
- rollback procedure;
- confirmation that no unrelated feature work occurred.

## 8. Verification requirements

The migration is complete only if all of the following are true:

- The complete predecessor suite is preserved, except for enumerated one-to-one identity renames.
- Debug and release suites pass on exact head.
- Formatting, documentation, scaffold, context-bundle, and all existing repository workflows pass.
- Sentinel 0 produces equivalent behavior under Bowfin.
- New Bowfin source files, CLI commands, crate names, and machine identifiers work.
- Promised Klyxr compatibility inputs work and produce explicit legacy handling.
- Historical evidence remains truthful and retrievable.
- The denylist finds zero unapproved live Klyxr identifiers.
- The historical allowlist contains no broad wildcard that could conceal a regression.
- Domain and repository redirects are tested from a clean environment.
- Raster and vector assets are manually inspected for stale branding.

## 9. Rollback strategy

Before public cutover, each PR can be reverted independently in reverse order because repository and domain identity have not yet changed.

After repository/domain cutover:

- do not reuse the old GitHub repository name;
- keep old domains and redirects intact;
- prefer a forward repair over renaming back and forth;
- retain the cutover commit/tree and full migration ledger;
- if a critical behavioral regression appears, disable the affected Bowfin release while preserving both names’ routing and historical access.

## 10. Decisions still requiring Greg’s explicit acceptance

1. Is **Bowfin** final, or still tentative?
2. Is `.bow` accepted despite its older format associations, or should `.bfn`, `.bwfn`, or `.bowfin` win?
3. Is `bowfin-lang.com` the primary domain?
4. Should the repository remain under `gregkrsak`, or is a future `bowfin-lang` organization desired?
5. How long should the `klyxr` CLI shim and `.klx` legacy input remain supported?
6. Should all existing canonical KED filenames/headings become BED immediately, while historical evidence payloads retain legacy tokens?
7. Should the first post-migration decision be BED-037, preserving the existing number line?
8. Should the original Klyxr Gateway advertisement remain beside the regenerated Bowfin version? The recommended answer is yes.

## 11. Preliminary external findings

- `bowfin-lang.com` returned a registry “not found” response during this planning pass, consistent with availability but not a reservation or guarantee.
- The GitHub destination `gregkrsak/bowfin` returned “not found” during this pass; this is not a reservation.
- GitHub search found other repositories named Bowfin, chiefly biological projects and one small inactive TypeScript repository, but no identified programming language/compiler project using Bowfin.
- The npm package `bowfin` exists at version `0.0.0`, published in 2020, with no description. Future JavaScript packages should use a controlled scope.
- PyPI returned “not found” for an exact `bowfin` project during this pass.
- The current GitHub Linguist language registry lists none of `.bow`, `.bfn`, or `.bwfn`.

These are point-in-time technical observations, not legal clearance. Recheck them immediately before reservation and cutover.

## 12. Reference links

- GitHub repository rename behavior: https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository
- GitHub Linguist language registry: https://github.com/github-linguist/linguist/blob/master/lib/linguist/languages.yml
- npm package naming: https://docs.npmjs.com/package-name-guidelines/
- npm scopes: https://docs.npmjs.com/about-scopes

## 13. Recommended immediate next move

Do **Phase 0 only**:

1. Greg accepts or rejects Bowfin as the final name.
2. Register `bowfin-lang.com` if accepted.
3. Run the focused `.bow` extension gate.
4. Freeze the mapping and compatibility policy in M0.

Only then authorize implementation.
