# Parser: allow underscore-leading identifiers for unused-binding ergonomics

> **Status:** **Resolved upstream in lex-lang v0.2.2** (closes [#200](https://github.com/alpibrusl/lex-lang/issues/200), shipped in [PR #205](https://github.com/alpibrusl/lex-lang/pull/205)). Draft preserved for historical context.
>
> **Shipped:**
>
> - Underscore-prefixed identifiers (`_name`) are accepted as function names, parameters, and let-bindings. The lexer's `Ident` regex grew a second alternative `_[a-zA-Z0-9_]+`.
> - `let _ := expr` is accepted as a discard pattern; the parser synthesises a unique name `__lex_discard_<N>` per occurrence so multiple discards don't collide.
> - Bare `_` alone is still invalid as a function name (only `_<chars>` forms work). Match patterns (`match { _ => ... }`) keep their existing meaning.

---

## Soft-side implications

soft-agent's `lex_dsl::DSL_PREAMBLE` and the agent `.lex` files used `state` and `msg` parameter names even when the handler didn't reference state — verbose but unavoidable. With v0.2.2 those become `_state` and `_msg`, signalling intent and silencing future unused-warning rules cleanly.

The verbose record-update workaround in our DSL preamble (each builder copies all 7 fields by hand) is unaffected by this change — that's a separate ergonomics gap (record-update spread syntax `{ ..base, peers: x }`). Worth a follow-up draft if it becomes painful, but the current preamble is well-tolerated.

## Original draft (for context)

The current parser rejects:

1. **Function names with a leading underscore.** `fn _host()` failed.
2. **`let _` as a binding pattern.** `let _ := state.ok` failed.

Both are conventional in Rust / ML-family languages. We hit them while writing soft-agent v3 (`crates/soft-agent/tests/lex_handler.rs`) and v2 (`crates/soft-agent/examples/depot.rs`).

## Related

- soft-agent's [`DSL_PREAMBLE`](../../../crates/soft-agent/src/lex_dsl.rs) — could now use `_state`/`_msg` for unused params; nice but not load-bearing.
