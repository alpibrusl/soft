# Lex-lang issues to file

Six issues to open against [`alpibrusl/lex-lang`](https://github.com/alpibrusl/lex-lang/issues/new) to support the soft Phase 1 demo.

Each file in this directory is one issue, formatted so the body below the `---` divider can be pasted directly into a new GitHub issue. The filing-instructions block at the top of each file gives the suggested title, labels, and source reference — those go into the GitHub form fields rather than the body.

| # | File | Title |
|---|---|---|
| 1 | [01-lex-agent-crate.md](./01-lex-agent-crate.md) | New crate `lex-agent` for autonomous agent runtime |
| 2 | [02-effect-tags.md](./02-effect-tags.md) | Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` |
| 3 | [03-mcp-tool-registry-wrapper.md](./03-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| 4 | [04-a2a-endpoints.md](./04-a2a-endpoints.md) | A2A endpoints exposed via `lex-api` |
| 5 | [05-spec-checker-runtime-gate.md](./05-spec-checker-runtime-gate.md) | Wire `spec-checker` as a runtime action gate |
| 6 | [06-lex-trace-vcs-integration.md](./06-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

## Filing flow

1. Open one of the files. Copy the **title** from the filing block.
2. Go to https://github.com/alpibrusl/lex-lang/issues/new.
3. Paste the title; paste the body (everything below the `---`).
4. Apply the suggested labels (create them if missing).
5. Once filed, drop the issue numbers back here and I'll cross-link them from `docs/proposal.md` §11.

## Note on duplication

The same issue bodies currently also live inline in `docs/proposal.md` §11. After the issues are filed and numbered, the proposal section can shrink to a short index pointing at this directory plus the GitHub issue numbers — say the word and I'll do that.
