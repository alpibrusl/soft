# Issue #186 — Wire `spec-checker` as a runtime action gate

> **Status:** Closed by [PR #191](https://github.com/alpibrusl/lex-lang/pull/191) in lex-lang v0.2.0 (2026-05-06).
>
> **Shipped:**
>
> ```
> evaluate_gate(specs, bindings, lex_source)         -> GateVerdict
> evaluate_gate_compiled(compiled_program, bindings) -> GateVerdict
>
> GateVerdict = Allow
>             | Deny         { spec_name, reason }
>             | Inconclusive { spec_name, reason }
> ```
>
> Bindings are `IndexMap<String, lex_bytecode::Value>`. Marshal Rust state via `Value::from_json(serde_json::to_value(state)?)`. Compiled-program lifetime is caller-owned; cache per agent type. Phase 1 specs (`grid_budget`, `soc_reserve`) verified at ~5 ms mean for 1k iterations.

---

## Original summary

Reuse `spec-checker` as a runtime invariant gate that runs between an agent's proposed action and its execution.

## Soft-side policy decisions

- **Inconclusive → Deny by default**, fail-safe. The verdict variant + spec name + reason are recorded in the trace alongside `Allow`/`Deny` so audit replay can distinguish "spec rejected" from "couldn't tell."
- Specs are authored as a library; no grammar change. Grammar stays ≤500 tokens.

## Acceptance — met

- Two safety specs gate proposed effects in the demo:
  - `grid_budget` — site grid load (active + scheduled + delta from proposed action) does not exceed budget.
  - `soc_reserve` — vehicle projected SoC after proposed action does not drop below reserve.
- Gate verdicts (variant + spec_name + reason) recorded to `lex-trace`.

## Consumer

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — calls `evaluate_gate_compiled` from the action-gate hook on every proposed effect.
