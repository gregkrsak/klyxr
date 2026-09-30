# Klyxr Agent Instructions

Before making language-design, compiler-architecture, verification, or major tooling changes, read these files in order:

1. `docs/ai/KLYXR_CONTEXT.md`
2. `docs/ai/DESIGN_PRINCIPLES.md`
3. `docs/ai/LANGUAGE_DECISIONS.md`
4. `docs/ai/ARCHITECTURE.md`
5. `docs/ai/OPEN_QUESTIONS.md`

`docs/ai/LANGUAGE_DECISIONS.md` is authoritative for settled decisions.

## Rules for agents

- Do **not** reinterpret an **Accepted** decision unless the user explicitly asks to reopen it.
- Distinguish **Accepted**, **Direction Accepted**, **Provisional**, **Proposed**, and **Open**.
- Do not silently turn illustrative syntax into a language guarantee.
- Do not overclaim formal verification. Klyxr proves stated properties under stated assumptions.
- When proposing a change, identify the affected KD/DP/OQ entries.
- Prefer coherent semantics, explicit trust boundaries, precise diagnostics, and auditable behavior.
- After an accepted design change, update the canonical files in `docs/ai/`.
- Rebuild the single-file context bundle with:

```bash
python scripts/build-ai-context.py
```

The repository—not conversational memory—is Klyxr's canonical project context.
