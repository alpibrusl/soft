# Issue #185 — MCP server wrapper for Tool Registry

> **Status:** Closed by [PR #190](https://github.com/alpibrusl/lex-lang/pull/190) in lex-lang v0.2.0 (2026-05-06).
>
> **Shipped:** real stdio JSON-RPC client behind `agent.call_mcp(server, tool, args_json) :: [mcp] Result[Str, Str]`. Spawn-per-call, stateless. Performs the JSON-RPC `initialize` handshake. `Drop` reaps the subprocess.
>
> Returns the server's `result` JSON serialized to a Lex `Str`, or `Err(reason)`.

---

## Original summary

Allow MCP servers to be called from Lex agents. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Resolved questions

- **Server-name resolution:** `server` is a stdio command line (whitespace-separated argv). Soft maintains the name → command mapping in `soft-agent` — see [`mocks/README.md`](../../../mocks/README.md) for the planned `--mcp name=cmd` shape.
- **Per-server scoping:** lex 0.2 ships flat `[mcp]`. Per-server scoping (e.g. `[mcp(ocpp)]`) is enforced at runtime by soft-agent's tool registry. Parameterized effects are a possible future upstream ask if the runtime check turns into a real source of bugs.

## Open follow-up

- [#197](https://github.com/alpibrusl/lex-lang/issues/197) — connection cache for `agent.call_mcp`. Awaits soft-side benchmarks; we'll produce them as part of Phase 1.

## Consumer

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — uses `agent.call_mcp` from every agent's MCP-effecting handler.
