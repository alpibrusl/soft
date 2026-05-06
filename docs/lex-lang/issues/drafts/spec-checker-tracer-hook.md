# Draft — `spec-checker`: optional Tracer hook for `evaluate_gate*`

> **Status:** Draft, not yet filed. Will become the next open issue (#199 or thereabouts).
> **Repository:** `alpibrusl/lex-lang`
> **Title:** `` spec-checker: accept an optional Tracer in evaluate_gate*` ``
> **Suggested labels:** `spec-checker`, `lex-trace`, `enhancement`
> **Source:** `alpibrusl/soft:crates/soft-agent/src/{gate.rs,lex_host.rs}`

---

## Summary

`spec_checker::evaluate_gate_compiled` (and `evaluate_gate`) build their internal `Vm` with no `Tracer` attached:

```rust
// spec-checker/src/gate.rs, eval_body for SpecExpr::Call
let handler = DefaultHandler::new(policy.clone());
let mut vm = Vm::with_handler(bc, Box::new(handler));
vm.call(func, argv).map_err(|e| format!("call `{func}`: {e}"))
```

That means when a downstream gate (e.g. `soft-agent`) wants to record the spec body's host-function calls into its own [`lex_trace::Recorder`], the nested calls are invisible — only the gate verdict appears in the trace.

## Concrete pain we hit

`soft-agent`'s Phase 1 demo gates each proposed action against a `grid_budget` spec whose body calls a Lex helper:

```text
spec grid_budget {
  forall current_kw :: Float, delta_kw :: Float, grid_kw :: Float, pv_kw :: Float:
    under_budget(current_kw, delta_kw, grid_kw, pv_kw)
}
```

`under_budget` itself calls `projected_load` and `budget_total`. We want the trace to read:

```
gate.verdict
  └─ call: under_budget
       ├─ call: projected_load
       └─ call: budget_total
```

…so audit replay can show *why* the gate said Allow/Deny in terms of the helper functions, not just the predicate's final boolean. Today we get the leaf verdict only. We can demonstrate the full nested trace via `soft_agent::LexHost` when we drive a Lex function through our own `Vm + Recorder`, but `evaluate_gate_compiled` doesn't expose that path.

## Proposed change

Add an opt-in tracer parameter to the evaluator, keeping current callers unaffected. One reasonable shape:

```rust
pub fn evaluate_gate_compiled_traced(
    specs: &[Spec],
    bindings: &IndexMap<String, Value>,
    bc: &lex_bytecode::Program,
    tracer: Option<Box<dyn lex_bytecode::vm::Tracer>>,
) -> GateVerdict { ... }
```

Or accept a `Recorder` handle directly (avoids one level of `Box`):

```rust
pub fn evaluate_gate_compiled_with_recorder(
    specs: &[Spec],
    bindings: &IndexMap<String, Value>,
    bc: &lex_bytecode::Program,
    recorder: &lex_trace::Recorder,
) -> GateVerdict { ... }
```

…and have it call `vm.set_tracer(Box::new(recorder.handle()))` (mirroring [`Recorder`]'s shared-state pattern). Either spelling is fine; the existing untraced entry points stay unchanged.

## Acceptance

Same predicate as today's tests, plus:

- A spec whose body calls a host helper (e.g. `under_budget(a, b)` calling `projected_load(a, b)` etc.) returns `Allow`/`Deny` *and* the supplied tracer captures the nested call events with parent → child relationships.
- Untraced callers (the existing `evaluate_gate` / `evaluate_gate_compiled` signatures) continue to behave identically.

## Out of scope

- Effect-call tracing inside spec bodies (specs can't carry effect calls today; this is purely about call-event visibility).
- Streaming the trace — capture is enough; persistence and replay stay on the soft-agent side via `lex_store::Store::save_trace`.

## Related

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — the consumer that would wire the tracer through.
- `soft-agent`'s [`LexHost`](../../../crates/soft-agent/src/lex_host.rs) demonstrates the pattern when soft-agent owns the `Vm`.
- Closes by-design follow-up to [#187](https://github.com/alpibrusl/lex-lang/issues/187) (lex-trace ↔ lex-vcs integration documented; this is the runtime-side hook).
