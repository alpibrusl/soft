# Issue 2 — Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]`

> **Filing instructions**
> - **Repository:** `alpibrusl/lex-lang`
> - **Title:** `` Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` ``
> - **Suggested labels:** `effects`, `runtime`
> - **Source:** `alpibrusl/soft:docs/proposal.md`

---

## Summary

Add four new runtime effects to `lex-runtime` so agent effect signatures can express what they may do. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Why these four are distinct

- `[a2a]` and `[mcp]` are different protocols with different security and replay properties. Conflating them loses the ability to type-check tool access separately from peer access.
- `[llm-local]` and `[llm-cloud]` are different deployment surfaces with different latency, cost, and determinism properties. An edge `vehicle-agent` typed `[llm-local, telemetry-read, a2a-out]` must not be able to accidentally call a cloud LLM.

## Scope

- Effect tags in `lex-types`.
- Runtime handlers in `lex-runtime`.
- Policy-gate integration: `--allow-effects llm_local,a2a,...`.
- Trace integration: each effect invocation is recorded with enough information to replay (model snapshot hash + sampling seed for `[llm-*]`, message envelope for `[a2a]`, tool name + arguments + effect manifest for `[mcp]`).

## Out of scope

- The actual A2A and MCP wire formats (Issues 3 and 4).
- The `lex-agent` crate that consumes these effects (Issue 1).

## Acceptance

- A Lex program declaring `[llm-local]` cannot reach `[llm-cloud]` and vice versa, enforced at type-check.
- Each effect is loggable to `lex-trace` with enough information to replay.
- Existing examples (`weather_app.lex`, `chat_app.lex`, etc.) continue to type-check and run.

## Related

- Issue 1: `lex-agent` crate (consumer)
- Issue 3: MCP wrapper (uses `[mcp]`)
- Issue 4: A2A endpoints (uses `[a2a]`)
- Issue 6: trace integration depends on each effect being loggable
