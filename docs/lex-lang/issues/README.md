# Lex-lang issues

Issues we've filed (or drafted) against [`alpibrusl/lex-lang`](https://github.com/alpibrusl/lex-lang/issues) in support of soft.

## Filed and closed (in v0.2.0)

| GitHub | File | Title |
|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | [184-effect-tags.md](./184-effect-tags.md) | Add effect tags `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]` |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | [185-mcp-tool-registry-wrapper.md](./185-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | [186-spec-checker-runtime-gate.md](./186-spec-checker-runtime-gate.md) | Wire `spec-checker` as a runtime action gate |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | [187-lex-trace-vcs-integration.md](./187-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

## Open follow-ups (filed by lex-lang in response to our expectations doc)

| GitHub | Title |
|---|---|
| [#196](https://github.com/alpibrusl/lex-lang/issues/196) | LLM provider configuration |
| [#197](https://github.com/alpibrusl/lex-lang/issues/197) | `agent.call_mcp` connection cache |
| [#198](https://github.com/alpibrusl/lex-lang/issues/198) | Edge runtime cross-compile + verified aarch64 binaries |

## Drafts (to file)

Drafts surfaced while building soft-agent v0–v3 and soft-a2a v0. Bodies in [`drafts/`](./drafts/); paste each into a new issue at `https://github.com/alpibrusl/lex-lang/issues/new`. Once filed, rename the file to `<number>-<topic>.md` and move it out of `drafts/` next to the others.

| File | Title (proposed) | Priority |
|---|---|---|
| [drafts/parser-underscore-handling.md](./drafts/parser-underscore-handling.md) | Parser: allow `_name` and `let _` for unused-binding ergonomics | Low — papercuts only, current workarounds work. |

## Resolved before filing

| Topic | Resolution |
|---|---|
| [drafts/spec-checker-tracer-hook.md](./drafts/spec-checker-tracer-hook.md) | Shipped in lex-lang v0.2.1 as `spec_checker::evaluate_gate_compiled_traced` + `impl Tracer for lex_trace::Handle`. Wired through `soft_agent::Gate::evaluate_traced` and the runner's per-action gate path. |

## Two original drafts not filed upstream

The earlier proposal drafted six issues. Two were declined by `lex-lang` and now live in this repo as soft-internal crate scoping docs:

- Issue 1 (`lex-agent` crate) → [`docs/crates/soft-agent.md`](../../crates/soft-agent.md)
- Issue 4 (A2A endpoints via `lex-api`) → [`docs/crates/soft-a2a.md`](../../crates/soft-a2a.md)

See [`../scope-decision.md`](../scope-decision.md) for the upstream team's reasoning.

## Cross-references

- Master proposal: [`docs/proposal.md`](../../proposal.md)
- Phase 1 sketch: [`examples/phase1/`](../../../examples/phase1/)
- Soft-internal crate scoping: [`docs/crates/`](../../crates/)
