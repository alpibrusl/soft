# Issue #187 — Clarify `lex-trace` ↔ `lex-vcs` integration

> **Status:** Closed by [PR #192](https://github.com/alpibrusl/lex-lang/pull/192) in lex-lang v0.2.0 (2026-05-06). Documentation only — no code change to `lex-trace` or `lex-vcs`.
>
> **Answers:**
>
> - **Traces are NOT `lex-vcs` branches.** They live at `<store>/traces/<run_id>/trace.json` as standalone artifacts.
> - **Run IDs are content-addressed**, so cross-store sync is **union semantics** — no three-way merge needed.
> - **Edge ↔ cloud sync** is "blob copy + attestation entry" — no new resolver, no changes to `lex-vcs`.
> - Hardening multi-edge with agent-identity-seeded `RunId::new(seed)` is a [follow-up](https://github.com/alpibrusl/lex-lang/issues/198) (not blocking Phase 1).

---

## Soft-side implications

The original soft proposal assumed three-way merge of trace branches via `lex-vcs store-merge`. The shipped answer is simpler — see `docs/proposal.md` §3.3, §6.4, §6.5 for the corrected description.

## Writer-side API soft will use

```rust
let recorder = lex_trace::Recorder::new();
let run_id   = lex_trace::RunId::new(seed);   // seed: agent_instance_id + intent_hash
let vm       = lex_runtime::Vm::with_tracer(&recorder);

// ... vm runs the agent's Lex code; recorder accumulates a TraceTree ...

let tree = recorder.into_tree();
lex_store::Store::save_trace(&tree)?;          // writes <store>/traces/<run_id>/trace.json
```

## Consumer

- [`docs/crates/soft-agent.md`](../../crates/soft-agent.md) — runs the VM with the recorder attached; persists per-run traces.
