# Issue 1 — New crate `lex-agent` for autonomous agent runtime

> **Filing instructions**
> - **Repository:** `alpibrusl/lex-lang`
> - **Title:** `` New crate `lex-agent` for autonomous agent runtime ``
> - **Suggested labels:** `crate`, `proposal`, `agents`
> - **Source:** `alpibrusl/soft:docs/proposal.md` (and `examples/phase1/` for a usage sketch)

---

## Summary

Add a new crate `lex-agent` that provides the runtime primitives for autonomous agents in Lex. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Scope

- An `agent` type carrying:
  - identity (content-addressed),
  - state,
  - declared peer list,
  - declared MCP tool allowlist,
  - declared effect set,
  - system prompt and goal (for LLM-driven agents).
- A mailbox abstraction over A2A messages (in-process for Phase 1; transport-pluggable).
- Handler dispatch built on `std.flow`: receive → propose → gate → execute.
- Action-gate hook between "LLM proposed action" and "effect executes" that calls into `spec-checker` and writes to `lex-trace`.

## Out of scope

- A2A wire transport (separate issue — see Issue 4).
- MCP wire transport (Issue 3).
- Effect handlers themselves (Issue 2).

## Acceptance

Phase 1 of `soft` can run a three-agent demo (`vehicle`, `depot`, `tms`) in-process using `lex-agent`. See `examples/phase1/` in `alpibrusl/soft` for the API shape this issue is targeting.

## Related

- Issue 2: effect tags
- Issue 3: MCP wrapper
- Issue 4: A2A endpoints
- Issue 5: spec-checker as runtime gate
- Issue 6: lex-trace ↔ lex-vcs integration
