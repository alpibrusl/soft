# Crate: `soft-agent`

> **Status:** Skeleton on the branch (compiles against lex-lang v0.2.0). Implementation work begins now that the four upstream primitives are shipped (#184–#187 closed by PRs #189–#192).
>
> **Why it lives here, not upstream.** Originally drafted as `lex-lang` Issue 1 and declined — agent runtime is a layer above the language, closer to actor-model territory than to "what lex-lang is." Building it in `soft` keeps lex-lang's scope honest and lets soft own its own evolution. See [`docs/lex-lang/scope-decision.md`](../lex-lang/scope-decision.md).

## Purpose

`soft-agent` is the autonomous-agent runtime for the soft project. It sits on top of `lex-lang` v0.2.0's primitives and provides the actor-shaped abstractions LLM-driven agents need.

## Scope

- An `agent` type carrying:
  - identity (content-addressed via `lex-ast` SigId / StageId),
  - state,
  - declared peer list,
  - declared MCP allowlist (server names; resolved by soft-agent at runtime),
  - declared effect set (`[llm_local]` xor `[llm_cloud]`, plus `[mcp]` and `[a2a]` if needed),
  - system prompt and goal.
- A mailbox abstraction over A2A messages. In-process for Phase 1; transport-pluggable. The wire-level A2A surface lives in [`soft-a2a`](./soft-a2a.md).
- Handler dispatch: receive → propose → gate → execute. The handler shape (`agent.handle("Topic", fn)`) is soft-agent's API; it sits *above* the four lex-lang primitives the handlers ultimately call.
- Action-gate hook between "LLM proposed action" and "effect executes" that calls `spec_checker::evaluate_gate_compiled` and records the verdict (variant + spec name + reason) to `lex_trace::Recorder`.
- **Inconclusive → Deny** as default policy, fail-safe; per-spec policy override available if a fail-open use case appears.

## Out of scope

- A2A wire transport — see [`soft-a2a`](./soft-a2a.md).
- MCP wire transport — shipped upstream in lex-lang v0.2.0 ([`agent.call_mcp`](https://github.com/alpibrusl/lex-lang/pull/190)).
- LLM and effect handlers — shipped upstream in lex-lang v0.2.0 ([`std.agent`](https://github.com/alpibrusl/lex-lang/pull/189)).

## Lex-lang dependencies (all at 0.2)

| Need | Provided by | Crate |
|---|---|---|
| Effect tags + four typed builtins (`agent.{local_complete, cloud_complete, send_a2a, call_mcp}`) | std.agent (PR #189) | `lex-runtime` |
| MCP wire client behind `agent.call_mcp` | PR #190 | `lex-runtime` |
| Action gate (`evaluate_gate_compiled`) | PR #191 | `spec-checker` |
| Trace recorder + run identity | PR #192 — `Recorder`, `RunId::new(seed)` | `lex-trace` |
| Trace persistence | `Store::save_trace` | `lex-store` |
| Spec bindings (Rust → Lex `Value`) | `Value::from_json` | `lex-bytecode` |
| VM hosting Lex agent code | `Vm::with_tracer` | `lex-runtime` |
| Content-addressed identity | SigId / StageId | `lex-ast` |

Cargo deps already pinned in [`Cargo.toml`](./Cargo.toml); workspace pins lex-lang at `"0.2"` (semver — breaking changes upstream bump to 0.3+).

## Acceptance

Phase 1 of soft can run a three-agent demo (`vehicle`, `depot`, `tms`) in-process using `soft-agent`. See [`examples/phase1/`](../../examples/phase1/) for the API shape this crate is targeting.

## Future upstream path

If multiple downstream consumers materialise and the abstraction stabilises, upstreaming as `lex-agent` is on the table. Not in scope for Phase 1.
