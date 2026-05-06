# Lex-lang issues

Issues we've filed (or drafted) against [`alpibrusl/lex-lang`](https://github.com/alpibrusl/lex-lang/issues) in support of soft.

## Filed and closed (in v0.2.0)

| GitHub | File | Title |
|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | [184-effect-tags.md](./184-effect-tags.md) | Add effect tags `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]` |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | [185-mcp-tool-registry-wrapper.md](./185-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | [186-spec-checker-runtime-gate.md](./186-spec-checker-runtime-gate.md) | Wire `spec-checker` as a runtime action gate |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | [187-lex-trace-vcs-integration.md](./187-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

## Closed by lex-lang in v0.2.2 (2026-05-06)

| GitHub | Closing PR | Title |
|---|---|---|
| [#196](https://github.com/alpibrusl/lex-lang/issues/196) | [#203](https://github.com/alpibrusl/lex-lang/pull/203) | LLM provider configuration — env-var shape we sketched (`OLLAMA_HOST`, `LEX_LLM_LOCAL_MODEL`, `LEX_LLM_CLOUD_BASE_URL`, `LEX_LLM_CLOUD_API_KEY`) plus an `EffectHandler` escape hatch. `agent.local_complete` and `agent.cloud_complete` now make real HTTP calls. |
| [#197](https://github.com/alpibrusl/lex-lang/issues/197) | [#203](https://github.com/alpibrusl/lex-lang/pull/203) | `agent.call_mcp` connection cache — `McpClientCache` (LRU, default cap 16) keyed by command-line, owned by `DefaultHandler`. |
| [#198](https://github.com/alpibrusl/lex-lang/issues/198) (docs half) | [#204](https://github.com/alpibrusl/lex-lang/pull/204) | Edge runtime cross-compile recipes — `aarch64-unknown-linux-{gnu,musl}` and `aarch64-darwin`, pre-built binaries on every `v*` release. The hardware-verification half stays on soft. |
| (drafted) #200 | [#205](https://github.com/alpibrusl/lex-lang/pull/205) | Parser: `_name` identifiers + `let _` discard. Drafted by us in `drafts/parser-underscore-handling.md`; lex-lang adopted and shipped. |

## Resolved before filing

| Topic | Resolution |
|---|---|
| [drafts/spec-checker-tracer-hook.md](./drafts/spec-checker-tracer-hook.md) | Shipped in lex-lang v0.2.1 as `spec_checker::evaluate_gate_compiled_traced` + `impl Tracer for lex_trace::Handle`. Wired through `soft_agent::Gate::evaluate_traced` and the runner's per-action gate path. |
| [drafts/parser-underscore-handling.md](./drafts/parser-underscore-handling.md) | Shipped in lex-lang v0.2.2 as PR #205 — `_name` and `let _` both work. We can simplify our DSL preamble + agent .lex files to use `_state`/`_msg` for unused params. |

## Open soft-side work (none blocking)

- Verify lex-lang's pre-built aarch64 binaries on real Jetson + Mac Studio mini hardware (the verification half of #198).
- An LLM-driven example agent now that `agent.local_complete`/`cloud_complete` are wired upstream.

## Two original drafts not filed upstream

The earlier proposal drafted six issues. Two were declined by `lex-lang` and now live in this repo as soft-internal crate scoping docs:

- Issue 1 (`lex-agent` crate) → [`docs/crates/soft-agent.md`](../../crates/soft-agent.md)
- Issue 4 (A2A endpoints via `lex-api`) → [`docs/crates/soft-a2a.md`](../../crates/soft-a2a.md)

See [`../scope-decision.md`](../scope-decision.md) for the upstream team's reasoning.

## Cross-references

- Master proposal: [`docs/proposal.md`](../../proposal.md)
- Phase 1 sketch: [`examples/phase1/`](../../../examples/phase1/)
- Soft-internal crate scoping: [`docs/crates/`](../../crates/)
