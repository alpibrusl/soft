# Issue 4 — A2A endpoints exposed via `lex-api`

> **Filing instructions**
> - **Repository:** `alpibrusl/lex-lang`
> - **Title:** `` A2A endpoints exposed via `lex-api` ``
> - **Suggested labels:** `lex-api`, `a2a`
> - **Source:** `alpibrusl/soft:docs/proposal.md`

---

## Summary

Expose A2A agent endpoints via the existing `lex-api` HTTP/JSON server. Agents become A2A-discoverable services with agent cards. Driven by the `soft` proposal in [`alpibrusl/soft:docs/proposal.md`](https://github.com/alpibrusl/soft/blob/claude/review-soft-proposal-fubbK/docs/proposal.md).

## Scope

- A2A protocol adapter:
  - `Message` and `Task` lifecycle.
  - Content `Parts` (text + structured data + files).
  - Agent cards for discovery and identity.
  - Streaming via SSE where A2A specifies it.
- Pin a specific A2A version. **Isolate the adapter in one module** so a future upgrade is a contained change — A2A is emerging-standard rather than settled.
- Wiring through `lex-api` so the same HTTP/JSON server serves the existing endpoints plus the A2A surface.

## Out of scope (for first cut)

- Cross-process A2A is acceptable as a target for this issue, but Phase 1 of `soft` only requires in-process. Cross-process can land later.
- Auth between A2A peers — design should not preclude it; first cut can rely on shared-network trust.

## Acceptance

- A `lex-agent` instance is reachable as an A2A endpoint.
- Returns a valid A2A agent card describing its capabilities.
- Round-trips a Message containing a structured Part.
- Each inbound and outbound message is recorded in `lex-trace`.

## Related

- Issue 1: `lex-agent` crate (consumer)
- Issue 2: `[a2a]` effect tag (prerequisite)
