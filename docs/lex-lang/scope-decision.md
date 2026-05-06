# Scope decision: agent runtime and A2A endpoints stay in soft

This document preserves the `lex-lang` team's response to soft's original proposal §11. Two of the six drafted issues (1 — `lex-agent` crate, 4 — A2A endpoints via `lex-api`) were declined as upstream lex-lang work and now live in the soft repo as the `soft-agent` and `soft-a2a` crates.

## Status of the six original draft issues

| Original | Outcome | Lives at |
|---|---|---|
| 1 — `lex-agent` crate | Declined upstream | [`docs/crates/soft-agent.md`](../crates/soft-agent.md) |
| 2 — Effect tags | Filed as **#184** | [`issues/184-effect-tags.md`](./issues/184-effect-tags.md) |
| 3 — MCP wrapper | Filed as **#185** | [`issues/185-mcp-tool-registry-wrapper.md`](./issues/185-mcp-tool-registry-wrapper.md) |
| 4 — A2A endpoints via `lex-api` | Declined upstream | [`docs/crates/soft-a2a.md`](../crates/soft-a2a.md) |
| 5 — `spec-checker` as runtime gate | Filed as **#186** | [`issues/186-spec-checker-runtime-gate.md`](./issues/186-spec-checker-runtime-gate.md) |
| 6 — `lex-trace` ↔ `lex-vcs` integration | Filed as **#187** | [`issues/187-lex-trace-vcs-integration.md`](./issues/187-lex-trace-vcs-integration.md) |

## Upstream reasoning (verbatim)

> Thanks for the proposal — the four primitives we said yes to (#184 effect tags, #185 MCP client, #186 spec-checker as runtime gate, #187 lex-trace ↔ lex-vcs integration) are filed and on the lex-lang roadmap.
>
> We're declining Issues 1 and 4 **as upstream lex-lang work**. We'd like both to live in the soft repo (or its own crate that depends on lex-lang) instead. Reasoning below — none of it changes Phase 1's feasibility, just where the code lives.
>
> ### Issue 1 — `lex-agent` crate
>
> The agent abstraction (identity, mailbox, handler dispatch, system-prompt + goal, peer/tool allowlists) is **a different layer than the language**. It's closer to actor-model territory than to "what lex-lang is."
>
> The substrate it needs from lex-lang already exists or is being added:
>
> | `lex-agent` needs | lex-lang provides |
> |---|---|
> | Content-addressed identity | `lex-vcs` SigId / StageId — content-addressed by AST canonicalization |
> | Effect set declaration | Existing effect-row types + #184's new tags |
> | MCP tool allowlist | Tool Registry + #185 |
> | Action gate | #186 |
> | Trace per action | `lex-trace` + #187 |
> | Auditable history | `lex-vcs` (already shipped, tier-2) |
>
> Build `soft-agent` (or `lex-agent` in soft's workspace) on top of those. If multiple downstream consumers materialize and the abstraction stabilizes, we'll happily upstream it later.
>
> This also keeps lex-lang's scope honest: a programming language with effects-as-types and an agent-native VCS, *not* a multi-agent runtime.
>
> ### Issue 4 — A2A endpoints via `lex-api`
>
> A2A is **emerging-standard** (the issue itself notes this) and is **a protocol for agents to talk**, not a language feature. Two concrete reasons to keep it downstream:
>
> 1. Pinning a specific A2A version inside lex-api creates a maintenance commitment we can't make today. The version churn cost should sit with whoever's actually consuming A2A — i.e. soft.
> 2. lex-api today is a thin JSON HTTP wrapper around lex-store / lex-vcs / merge sessions. A2A's `Message` / `Task` / `Parts` / agent-card surface is a different abstraction layer; mixing them couples A2A's lifecycle to lex-api's release cycle.
>
> Recommended structure:
>
> ```
> soft/
> ├── crates/
> │   ├── soft-agent/      # Issue 1 — depends on lex-{vcs,types,trace,api}
> │   └── soft-a2a/        # Issue 4 — depends on lex-api as a library, pins A2A version
> └── examples/phase1/
> ```
>
> `soft-a2a` can use `lex-api` as a library (its `serve_on(server, state)` is already exposed) to add A2A routes alongside the lex-native ones. No upstream change required; a future `lex serve --with-a2a` flag could even be a one-line addition that pulls in the soft-a2a crate as an optional feature.
>
> ### What this changes for Phase 1
>
> - **Nothing**, in terms of capabilities. soft Phase 1 (`vehicle` / `depot` / `tms` three-agent demo) needs in-process agents, MCP tool calls, action gating, and traceable audit. Issues #184–#187 cover the lex-lang side; `soft-agent` + `soft-a2a` cover the assembly.
> - **Cleaner ownership**: soft owns its agent runtime and its A2A version pin. lex-lang owns the primitives. Easier to evolve both independently.

## Soft response

Agreed on both — the reasoning is sound. Boundary recorded in:

- [`docs/crates/soft-agent.md`](../crates/soft-agent.md) and [`docs/crates/soft-a2a.md`](../crates/soft-a2a.md) — scope of the two new in-repo crates.
- [`docs/proposal.md`](../proposal.md) §5, §11, §12 — updated to reflect the boundary.
- [`examples/phase1/`](../../examples/phase1/) — unchanged; the API shape it targets is the same, only the repo of origin shifts.
