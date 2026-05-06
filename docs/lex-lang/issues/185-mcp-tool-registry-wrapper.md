# Issue #185 — MCP server wrapper for Tool Registry

> **Status:** Filed as [`alpibrusl/lex-lang#185`](https://github.com/alpibrusl/lex-lang/issues/185).
> Source: `alpibrusl/soft:docs/proposal.md`. Suggested labels: `tool-registry`, `mcp`.

---

## Summary

Allow MCP servers to be registered as Tool Registry entries with effect manifests, so a Lex program can call MCP tools through the existing tool-registry surface. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Background

`lex` already ships a Tool Registry (`lex tool-registry serve`) that pairs tools with effect manifests, plus ACLI compliance for tool discovery. The soft Phase 1 demo needs to call existing Python services (OCPP, OCPI, CSMS, telemetry, scheduling-optimizer) from Lex agents. Each Python service is exposed as an MCP server. This issue is the bridge between MCP servers and the Tool Registry.

## Scope

- A wrapper that consumes an MCP server's tool list and produces Tool Registry entries.
- Effect manifest derivation: each MCP tool is annotated with an effect (e.g. a `mcp(ocpp)` tool annotated with `[mcp(ocpp), net]`).
- Wire-format glue between MCP's JSON-RPC and the Tool Registry's expected shape.
- A way to scope an agent's MCP allowlist by server identity (so an agent typed for `mcp(optimizer)` cannot reach `mcp(ocpp)`).

## Open questions

- How much of this is already covered by ACLI compliance? May reduce scope significantly.
- Authentication and credential handling at the MCP boundary — out of scope for first cut, but the design should not preclude adding it.
- Streaming MCP tools (resources, prompts, sampling) — Phase 1 only needs synchronous tool calls.

## Acceptance

- A Python service exposed as an MCP server can be called from a Lex agent via the Tool Registry.
- The call is correctly typed against the agent's `[mcp(...)]` effect.
- The call is recorded in `lex-trace` with enough information to replay.

## Related

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — consumer
- #184 — `[mcp]` effect tag (prerequisite)
