# First Klyxr compiler vertical slice

This is intentionally **not** a general-purpose Klyxr compiler yet.

It is the first executable path through the planned architecture:

```text
.klx source
   ↓
lexer
   ↓
parser
   ↓
AST
   ↓
tiny semantic / contract verifier
   ↓
Klyxr-style diagnostic
```

The slice understands just enough Klyxr to demonstrate the flagship battery example:

- `type ... = range ...`
- one-field `record`
- `verified fn`
- `requires amount <= state.field`
- mutable record construction
- one call with an integer literal
- compact `mut` / `fn` and explicit `mutable` / `function` keyword normalization

This narrow scope is deliberate. It proves the compiler architecture and diagnostic loop before expanding the language surface.
