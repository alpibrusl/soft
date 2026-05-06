# Lex-lang issues

Four issues filed in [`alpibrusl/lex-lang`](https://github.com/alpibrusl/lex-lang/issues) to support the soft Phase 1 demo. Bodies mirrored here for traceability and to preserve the cross-references back into soft.

| GitHub | File | Title |
|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | [184-effect-tags.md](./184-effect-tags.md) | Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | [185-mcp-tool-registry-wrapper.md](./185-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | [186-spec-checker-runtime-gate.md](./186-spec-checker-runtime-gate.md) | Wire `spec-checker` as a runtime action gate |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | [187-lex-trace-vcs-integration.md](./187-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

## Two original drafts not filed upstream

The earlier proposal drafted six issues. Two were declined by `lex-lang` and now live in this repo as soft-internal crate scoping docs:

- Issue 1 (`lex-agent` crate) → [`docs/crates/soft-agent.md`](../../crates/soft-agent.md)
- Issue 4 (A2A endpoints via `lex-api`) → [`docs/crates/soft-a2a.md`](../../crates/soft-a2a.md)

See [`../scope-decision.md`](../scope-decision.md) for the upstream team's reasoning.

## Cross-references

- Master proposal: [`docs/proposal.md`](../../proposal.md)
- Phase 1 sketch: [`examples/phase1/`](../../../examples/phase1/)
- Soft-internal crate scoping: [`docs/crates/`](../../crates/)
