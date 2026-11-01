# Klyxr Repository Structure

This repository contains the initial Klyxr scaffold plus an executable Rust-based
compiler prototype. Current GitHub home: `gregkrsak/klyxr`. The repository may
later move to a dedicated Klyxr organization.

```text
klyxr/
├── AGENTS.md
├── CLAUDE.md
├── GEMINI.md
├── README.md
├── REPO_STRUCTURE.md
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .gitignore
├── .github/
│   ├── copilot-instructions.md
│   └── workflows/
│       ├── build.yml
│       ├── docs.yml
│       ├── tests.yml
│       └── verify.yml
├── assets/
│   ├── diagrams/
│   │   ├── assurance-ladder.svg
│   │   └── knowledge-boundaries.svg
│   └── readme-banner.png
├── compiler/
│   ├── Cargo.toml
│   ├── README.md
│   ├── lib.rs
│   ├── main.rs
│   ├── ast/
│   ├── codegen/
│   ├── diagnostics/
│   ├── effects/
│   ├── hir/
│   ├── lexer/
│   ├── mir/
│   ├── ownership/
│   ├── parser/
│   ├── resolve/
│   ├── types/
│   ├── tests/
│   ├── verify/
│   └── vir/
├── docs/
│   ├── README.md
│   ├── ai/
│   │   ├── ARCHITECTURE.md
│   │   ├── DECISION_TEMPLATE.md
│   │   ├── DESIGN_PRINCIPLES.md
│   │   ├── HANDOFF_PROMPT.md
│   │   ├── KLYXR_AI_CONTEXT_BUNDLE.md
│   │   ├── KLYXR_CONTEXT.md
│   │   ├── LANGUAGE_DECISIONS.md
│   │   ├── OPEN_QUESTIONS.md
│   │   └── README.md
│   ├── design/
│   └── language/
├── examples/
│   ├── battery.klx
│   ├── battery_ok.klx
│   ├── battery_fail.klx
│   ├── battery_sequence_fail.klx
│   ├── battery_body_fail.klx
│   └── effects.klx
├── rfcs/
├── runtime/
├── scripts/
│   ├── build-ai-context.py
│   ├── verify-repo-scaffold.py
│   └── verify-readme.py
├── std/
├── tests/
└── tools/
```

Empty implementation directories contain `.gitkeep` so Git will preserve the initial structure.

## Deliberately omitted for now

The scaffold does **not** choose these yet because the project has not locked them down:

- a software license;
- a final package/manifest format;
- a stable ABI policy.

Those should be added through explicit project decisions rather than guessed into the initial repository.

The prototype compiler uses a Cargo workspace with a pinned Rust toolchain.
That implementation choice does not settle the future Klyxr package manifest.
