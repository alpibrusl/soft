# soft-a2a

A2A protocol surface for soft agents.

> **Status:** v0 — minimal Message + AgentCard schema, server (`tiny_http`)
> and client (`ureq`). 4/4 round-trip tests pass.
>
> See [`../../docs/crates/soft-a2a.md`](../../docs/crates/soft-a2a.md) for scope.

## What ships in v0

- `wire`: A2A `Message`, `Part`, `Role`, `MessageMetadata`, `AgentCard`,
  `Capabilities`, `Skill`. Pinned to [`A2A_VERSION`].
- `server::A2aServer` — binds an HTTP listener, exposes
  `GET /a2a/agent-card` and `POST /a2a/messages`, forwards inbound
  messages into a [`soft_agent::MailboxSender`].
- `client::A2aClient` — `send(peer_url, &Message)` and
  `fetch_agent_card(peer_url)`.

## Convention

Inbound A2A messages must include a top-level `metadata` field with
`from` (sender peer ID) and `topic` (the topic handler to dispatch into):

```json
{
  "message_id": "...",
  "role": "user",
  "parts": [{ "kind": "data", "data": { "x": 1 } }],
  "metadata": { "from": "tester", "topic": "RequestSession" }
}
```

This is soft-a2a's routing layer on top of A2A's base envelope; it keeps
the wire format interoperable with other A2A implementations while giving
soft-agent the routing fields it needs.

## Build + test

```
cargo test -p soft-a2a
```

## Deferred to next slice

- A2A Tasks lifecycle (we ship Message-only today).
- SSE streaming for long-running tasks.
- Authentication (today: shared-network trust).
- Signatures / agent identity verification beyond `agent_card.url`.
