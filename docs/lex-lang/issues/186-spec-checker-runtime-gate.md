# Issue #186 — Wire `spec-checker` as a runtime action gate

> **Status:** Filed as [`alpibrusl/lex-lang#186`](https://github.com/alpibrusl/lex-lang/issues/186).
> Source: `alpibrusl/soft:docs/proposal.md`. Suggested labels: `spec-checker`, `runtime`.

---

## Summary

Reuse `spec-checker` as a runtime invariant gate that runs between an agent's proposed action and its execution. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Scope

- An API roughly: `gate(state, proposed_effect, specs) -> Allow | Deny(reason)`.
- **Performance target:** per-action verdict in single-digit milliseconds for Phase 1's spec set (two specs, ~5 vehicles, ~3 chargers).
- Trace integration: every gate decision (Allow or Deny + reason) is recorded to `lex-trace`.
- Spec authoring is **library-first**, no grammar change. The ≤500-token grammar constraint stays intact.

## Open questions

- `spec-checker` today is built around randomized property checking and SMT-LIB export. Is the existing implementation suitable for synchronous per-action gating, or is a different evaluation mode needed? Possibly the runtime gate uses a deterministic sub-mode, with the SMT-LIB path reserved for offline verification.
- Spec authoring ergonomics. The two Phase 1 specs (see [`alpibrusl/soft:examples/phase1/specs/`](../../../examples/phase1/specs/)) are simple `forall state, action: P(state, action)` predicates. If existing `spec-checker` syntax is heavier than that, a thin sugar layer may help.

## Acceptance

- Phase 1's two safety specs can be expressed and gate proposed effects in the demo:
  - `grid_budget` — site grid load (active + scheduled + delta from proposed action) does not exceed budget.
  - `soc_reserve` — vehicle projected SoC after proposed action does not drop below reserve.
- Gate verdicts are recorded in `lex-trace` so audit replay reproduces the decision history.

## Related

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — calls `gate` from the action-gate hook
- #187 — `lex-trace` integration (consumes gate decisions)
