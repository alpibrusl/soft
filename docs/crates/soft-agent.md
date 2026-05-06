# Crate: `soft-agent`

> **Status:** Scoping doc. Crate not yet implemented.
>
> **Why it lives here, not upstream.** This was originally drafted as `lex-lang` Issue 1. The `lex-lang` team declined — agent runtime is a layer above the language, closer to actor-model territory than to "what lex-lang is." Building it in `soft` keeps lex-lang's scope honest (a programming language with effects-as-types and an agent-native VCS) and lets soft own its own evolution. See [`docs/lex-lang/scope-decision.md`](../lex-lang/scope-decision.md) for the full reasoning.

## Purpose

`soft-agent` is the autonomous-agent runtime for the soft project. It sits on top of `lex-lang`'s primitives (effects, types, trace, vcs) and provides the actor-shaped abstractions that LLM-driven agents need.

## Scope

- An `agent` type carrying:
  - identity (content-addressed via `lex-vcs` SigId / StageId),
  - state,
  - declared peer list,
  - declared MCP tool allowlist,
  - declared effect set,
  - system prompt and goal (for LLM-driven agents).
- A mailbox abstraction over A2A messages. In-process for Phase 1; transport-pluggable. The wire-level A2A surface lives in [`soft-a2a`](./soft-a2a.md).
- Handler dispatch built on `std.flow`: receive → propose → gate → execute.
- Action-gate hook between "LLM proposed action" and "effect executes" that calls into `spec-checker` ([#186](https://github.com/alpibrusl/lex-lang/issues/186)) and writes to `lex-trace` ([#187](https://github.com/alpibrusl/lex-lang/issues/187)).

## Out of scope

- A2A wire transport — see [`soft-a2a`](./soft-a2a.md).
- MCP wire transport — see [#185](https://github.com/alpibrusl/lex-lang/issues/185).
- Effect handlers themselves — see [#184](https://github.com/alpibrusl/lex-lang/issues/184).

## Lex-lang dependencies

| Need | Provided by |
|---|---|
| Content-addressed identity | `lex-vcs` SigId / StageId (already shipped) |
| Effect set declaration | `lex-types` effect rows + [#184](https://github.com/alpibrusl/lex-lang/issues/184) new tags |
| MCP tool allowlist | Tool Registry + [#185](https://github.com/alpibrusl/lex-lang/issues/185) |
| Action gate | [#186](https://github.com/alpibrusl/lex-lang/issues/186) |
| Trace per action | `lex-trace` + [#187](https://github.com/alpibrusl/lex-lang/issues/187) |
| Auditable history | `lex-vcs` (already shipped) |

## Acceptance

Phase 1 of soft can run a three-agent demo (`vehicle`, `depot`, `tms`) in-process using `soft-agent`. See [`examples/phase1/`](../../examples/phase1/) for the API shape this crate is targeting.

## Workspace location (planned)

```
crates/soft-agent/
├── Cargo.toml
├── src/
└── README.md
```

## Future upstream path

If multiple downstream consumers materialise and the abstraction stabilises, upstreaming as `lex-agent` is on the table. Not in scope for Phase 1.
