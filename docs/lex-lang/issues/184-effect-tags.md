# Issue #184 — Add effect tags `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]`

> **Status:** Closed by [PR #189](https://github.com/alpibrusl/lex-lang/pull/189) in lex-lang v0.2.0 (2026-05-06).
>
> **Shipped:** new `std.agent` module with four typed builtins:
>
> ```
> agent.local_complete(prompt)             :: [llm_local] Result[Str, Str]
> agent.cloud_complete(prompt)             :: [llm_cloud] Result[Str, Str]
> agent.send_a2a(peer, payload)            :: [a2a]       Result[Str, Str]
> agent.call_mcp(server, tool, args_json)  :: [mcp]       Result[Str, Str]
> ```
>
> Tags ship as snake_case (`llm_local`, `llm_cloud`) — soft's docs aligning to this.

---

## Original summary

Add four new runtime effects to `lex-runtime` so agent effect signatures can express what they may do.

## Why these four are distinct

- `[a2a]` and `[mcp]` are different protocols with different security and replay properties.
- `[llm_local]` and `[llm_cloud]` are different deployment surfaces with different latency, cost, and determinism properties.

## Acceptance — met

- A Lex program declaring `[llm_local]` cannot reach `[llm_cloud]` and vice versa, enforced at type-check.
- Each effect is loggable to `lex-trace` with enough information to replay.
- Existing examples (`weather_app.lex`, `chat_app.lex`, etc.) continue to type-check unchanged.

## Consumers

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — uses all four primitives.
- [`docs/crates/soft-a2a.md`](../../crates/soft-a2a.md) — wraps `agent.send_a2a` and exposes the receive side via `lex-api`.

## Related

- #185 (closed by [PR #190](https://github.com/alpibrusl/lex-lang/pull/190)) — MCP wrapper.
- #186 (closed by [PR #191](https://github.com/alpibrusl/lex-lang/pull/191)) — spec-checker as runtime gate.
- #187 (closed by [PR #192](https://github.com/alpibrusl/lex-lang/pull/192)) — `lex-trace` ↔ `lex-vcs`.
- [#196](https://github.com/alpibrusl/lex-lang/issues/196) (open) — LLM provider configuration.
