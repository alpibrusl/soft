# Issue 6 — Clarify `lex-trace` ↔ `lex-vcs` integration

> **Filing instructions**
> - **Repository:** `alpibrusl/lex-lang`
> - **Title:** `` Clarify `lex-trace` ↔ `lex-vcs` integration ``
> - **Suggested labels:** `lex-trace`, `lex-vcs`, `question`
> - **Source:** `alpibrusl/soft:docs/proposal.md`

---

## Summary

Question issue. Document and (if needed) implement the integration between `lex-trace` and `lex-vcs` so that an edge agent's local trace can be merged into a cloud audit log via `store-merge` on reconnect.

This is the substrate the soft Phase 1 demo's headline deliverable depends on: *replay the simulated day's trace from cloud + edge audit logs and prove no spec was ever violated.*

## Specifics to clarify

- Is each trace currently a `lex-vcs` branch, or is it a separate artifact in `lex-store`?
- If separate, what is the recommended path to land the integration the original soft draft (`alpibrusl/lex-lang:docs/proposals/soft.md` on the `claude/review-agent-vcs-architecture-9EZ74` branch) referenced as **tier-2**?
- Does `store-merge` already handle three-way merge of trace data, or does that need a custom resolver? Trace records are append-only per process, which should make the resolver case simple, but it should be confirmed.
- How should the truck (edge) and cloud agree on identity for a trace branch? Content-addressed agent identity from Issue 1 may be the natural anchor.

## Acceptance

- A clear answer documented in `lex-lang/docs/`.
- If implementation work is required, it is scoped as a follow-up issue (or issues) blocking the soft Phase 1 audit-replay deliverable.

## Related

- Issue 1: `lex-agent` crate (produces the trace)
- Issue 2: effect tags (each effect is a trace record)
- Issue 5: spec-checker as runtime gate (gate decisions are trace records)
