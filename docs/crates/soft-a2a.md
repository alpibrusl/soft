# Crate: `soft-a2a`

> **Status:** Scoping doc. Crate not yet implemented.
>
> **Why it lives here, not upstream.** This was originally drafted as `lex-lang` Issue 4. The `lex-lang` team declined for two reasons: (1) A2A is emerging-standard and pinning a version inside `lex-api` would create a maintenance commitment lex-lang can't make; the version churn cost belongs with the consumer. (2) `lex-api` today is a thin JSON HTTP wrapper around `lex-store` / `lex-vcs` / merge sessions, and A2A's `Message` / `Task` / `Parts` / agent-card surface is a different abstraction layer. See [`docs/lex-lang/scope-decision.md`](../lex-lang/scope-decision.md) for the full reasoning.

## Purpose

`soft-a2a` exposes A2A endpoints for the agents in [`soft-agent`](./soft-agent.md). It uses `lex-api` as a library (its `serve_on(server, state)` is already exposed) to add A2A routes alongside the lex-native ones.

## Scope

- A2A protocol adapter:
  - `Message` and `Task` lifecycle.
  - Content `Parts` (text + structured data + files).
  - Agent cards for discovery and identity.
  - Streaming via SSE where A2A specifies it.
- **Pin one A2A version.** Isolate the wire-format adapter in a single module so a future migration is contained.
- Wiring: `soft-a2a` registers A2A routes on a `lex-api` server instance.

## Out of scope (for first cut)

- Cross-process A2A is acceptable as a target; Phase 1 of soft only requires in-process.
- Auth between A2A peers — design should not preclude it; first cut can rely on shared-network trust.

## Lex-lang dependencies

| Need | Provided by |
|---|---|
| HTTP/JSON server | `lex-api` (used as a library; `serve_on(server, state)` is exposed) |
| `[a2a]` effect tag | [#184](https://github.com/alpibrusl/lex-lang/issues/184) |
| Trace per message | `lex-trace` + [#187](https://github.com/alpibrusl/lex-lang/issues/187) |

## Acceptance

- A `soft-agent` instance is reachable as an A2A endpoint.
- Returns a valid A2A agent card describing its capabilities.
- Round-trips a Message containing a structured Part.
- Each inbound and outbound message is recorded in `lex-trace`.

## Future upstream path

If the A2A version pin stabilises and multiple downstream consumers materialise, a `lex serve --with-a2a` flag could pull `soft-a2a` (or a successor) in as an optional feature in `lex-lang`. Not in scope for Phase 1.

## Workspace location (planned)

```
crates/soft-a2a/
├── Cargo.toml
├── src/
└── README.md
```
