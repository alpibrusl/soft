# Draft — Parser: allow underscore-leading identifiers for unused-binding ergonomics

> **Status:** Draft, not yet filed. Lower priority than the spec-checker tracer hook.
> **Repository:** `alpibrusl/lex-lang`
> **Title:** `` Parser: allow `_name` and `let _` for unused-binding ergonomics ``
> **Suggested labels:** `lex-syntax`, `ergonomics`
> **Source:** `alpibrusl/soft:crates/soft-agent/{tests/lex_handler.rs,examples/depot.rs}`

---

## Summary

The current parser rejects:

1. **Function names with a leading underscore.**
   ```
   fn _host() -> Int { 0 }
   //  ^ expected identifier for function name, got Some(Underscore)
   ```
2. **`let _` as a binding pattern.**
   ```
   let _ := state.ok
   //  ^ expected identifier after `let`, got Some(Underscore)
   ```

`_name` and `let _` are the conventional way (Rust, ML, Haskell, OCaml) to declare an intentionally-unused binding without triggering unused-variable warnings or refactoring around the unused thing.

## Where this bit us

`soft-agent` uses Lex source for spec host helpers (compiled by `Gate::from_sources`) and for handler functions (run by `LexHost::call`). Two recurring patterns surface:

1. **Stub host program** — sometimes a gate's specs are self-contained (no helper calls) but `evaluate_gate*` still requires a Lex source. The natural stub is `fn _host() -> Int { 0 }`, currently rewritten as `fn host() -> Int { 0 }`.
2. **Ignored handler params** — Lex handlers receive `(state, msg)` whether they use both or not. When a handler ignores `state`, the natural way to silence the unused-arg warning is `let _ := state.ok` or to name the param `_state`. Currently we have to either reference both or name the param without underscore prefix.

Neither is a hard blocker — the workarounds are minor — but they're papercuts for everyone authoring Lex code that interfaces with embedded Rust contexts.

## Proposed change

Either:

- **(a) Allow leading-underscore identifiers everywhere.** They're treated as ordinary identifiers, and the type-checker's unused-warning logic suppresses warnings when the name starts with `_`.
- **(b) Allow `_` as a sentinel pattern in `let` and as a function-name marker.** More surgical; matches Rust's convention for `let _ = expr;`.

(a) is a smaller change to grammar (probably one tokenizer rule), broader ergonomics. (b) is more typical of ML-family languages.

## Acceptance

```
fn _host() -> Int { 0 }                         # parses + type-checks
fn handler(_state :: Int, _msg :: Str) -> Int   # parses + type-checks; no unused warnings
{ 0 }
fn other(state :: Int) -> Int {
  let _ := state                                # parses; no unused warnings
  state + 1
}
```

Existing programs that don't use leading underscores keep behaving the same. Underscore-only identifiers (just `_`) become a discard pattern in `let`.

## Out of scope

- Pattern matching on `_` (already works in `match` arms via `match { _ => ... }`).
- Treating leading-underscore names as private/visibility markers — that's a separate discussion.

## Related

- Hit while writing soft-agent v3 ([`crates/soft-agent/tests/lex_handler.rs`](../../../crates/soft-agent/tests/lex_handler.rs)) and v2 ([`crates/soft-agent/examples/depot.rs`](../../../crates/soft-agent/examples/depot.rs)).
