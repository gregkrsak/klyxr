# Klyxr Repository Structure

This scaffold is the intended initial structure for the main `klyxr/klyxr` repository.

```text
klyxr/
├── AGENTS.md
├── CLAUDE.md
├── GEMINI.md
├── README.md
├── REPO_STRUCTURE.md
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
│   └── effects.klx
├── rfcs/
├── runtime/
├── scripts/
│   ├── build-ai-context.py
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
- concrete compiler source files;
- a final build system;
- a stable ABI policy.

Those should be added through explicit project decisions rather than guessed into the initial repository.
