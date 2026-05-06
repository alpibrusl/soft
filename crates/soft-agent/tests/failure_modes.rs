//! Failure-mode scenarios from `docs/proposal.md` §7.4:
//!   1. Charger drops mid-session — executor errors but spec invariants
//!      are not violated.
//!   2. Vehicle goes offline, then reconnects — trace blob-copied from an
//!      "edge" `Store` into a "cloud" `Store` (the §192 model: union
//!      semantics on content-addressed run IDs).

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::{json, Value};
use soft_agent::{
    A2aMessage, Action, ActionExecutor, AgentConfig, Effect, ExecError, Gate, Mailbox,
    Runner,
};
use tempfile::tempdir;

const HOST: &str = r#"
fn projected_load(current :: Float, delta :: Float) -> Float { current + delta }
fn budget_total(grid :: Float, pv :: Float)         -> Float { grid + pv }
fn under_budget(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  projected_load(current, delta) <= budget_total(grid, pv)
}
"#;

const GRID_BUDGET: &str = r#"
spec grid_budget {
  forall current_kw :: Float, delta_kw :: Float, grid_kw :: Float, pv_kw :: Float:
    under_budget(current_kw, delta_kw, grid_kw, pv_kw)
}
"#;

/// Executor that fails for a specific MCP tool — simulates the charger
/// going offline mid-session.
struct FlakyMcpExecutor {
    fail_tool: String,
}

impl ActionExecutor for FlakyMcpExecutor {
    fn execute(&mut self, action: &Action) -> Result<Value, ExecError> {
        if let Action::CallMcp { tool, server, .. } = action {
            if tool == &self.fail_tool {
                return Err(ExecError::Failed(format!(
                    "charger {server} offline; tool `{tool}` failed"
                )));
            }
        }
        Ok(json!({"executed": true}))
    }
}

#[test]
fn charger_drop_mid_session_does_not_violate_invariant() {
    // Depot has plenty of headroom — gate must allow.
    let gate = Gate::from_sources(&[GRID_BUDGET], HOST).unwrap();
    let agent = AgentConfig::new("depot")
        .peers(["vehicle"])
        .mcp_servers(["ocpp"])
        .effects([Effect::Mcp, Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({
            "current_kw": 30.0, "budget_kw": 100.0, "pv_kw": 20.0, "delta_kw": 50.0,
        }))
        .mailbox(mailbox)
        .gate(gate)
        .bindings_fn(Box::new(|state, _action| {
            fn fnum(v: &Value, k: &str) -> f64 {
                v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0)
            }
            let mut m = IndexMap::new();
            m.insert("current_kw".into(), LexValue::Float(fnum(state, "current_kw")));
            m.insert("delta_kw".into(),   LexValue::Float(fnum(state, "delta_kw")));
            m.insert("grid_kw".into(),    LexValue::Float(fnum(state, "budget_kw")));
            m.insert("pv_kw".into(),      LexValue::Float(fnum(state, "pv_kw")));
            m
        }))
        .executor(Box::new(FlakyMcpExecutor {
            fail_tool: "remote_start_transaction".into(),
        }))
        .handle("StartCharging", |_state, _msg| {
            vec![Action::CallMcp {
                server: "ocpp".into(),
                tool: "remote_start_transaction".into(),
                args: json!({"charger_id": "charger-1", "vehicle_id": "v-1"}),
            }]
        })
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "vehicle".into(),
            topic: "StartCharging".into(),
            payload: json!({}),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    // The action passes the gate (allowed) and is executed (executor's
    // returned Err is recorded into action.executed's outcome, not a
    // gate-level Deny).
    assert_eq!(report.total_allowed, 1);
    assert_eq!(report.total_denied, 0);

    // The trace records the action.executed with an error in its outcome
    // — informational, but no spec was violated.
    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).unwrap();
    let run_id = runner.finalize(&store).unwrap();
    let tree = store.load_trace(&run_id.0).unwrap();
    let executed = tree
        .nodes
        .iter()
        .find(|n| n.target == "action.executed")
        .expect("action.executed should be in the trace");
    assert!(
        executed.error.is_some(),
        "executor error should land in TraceNode.error"
    );

    // Replay invariant: executed (1) <= allowed (1) — no bypass.
    let counts = soft_agent::replay::scan_trace(&tree);
    assert!(!counts.has_violation(), "{counts:?}");
}

#[test]
fn offline_reconnect_blob_copy_preserves_trace() {
    // Edge runner produces a trace; persisted to an "edge" Store.
    let agent = AgentConfig::new("vehicle-edge")
        .peers(["depot"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .handle("Heartbeat", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "HeartbeatAck".into(),
                payload: json!({"ok": true}),
            }]
        })
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "depot".into(),
            topic: "Heartbeat".into(),
            payload: json!(null),
        })
        .unwrap();
    runner.drain().unwrap();

    let edge_dir = tempdir().unwrap();
    let edge_store = lex_store::Store::open(edge_dir.path()).unwrap();
    let run_id = runner.finalize(&edge_store).expect("edge trace persists");

    // ---- "vehicle goes offline, then reconnects" ----
    // Edge keeps the trace locally. On reconnect we blob-copy into the
    // cloud store — exactly the §192 recipe (no merge resolver).
    let cloud_dir = tempdir().unwrap();
    let cloud_store = lex_store::Store::open(cloud_dir.path()).unwrap();

    let edge_tree = edge_store.load_trace(&run_id.0).expect("edge trace loads");
    let copied = cloud_store.save_trace(&edge_tree).expect("cloud receives trace");
    assert_eq!(copied, run_id.0);

    // Cloud now has the same trace.
    let cloud_tree = cloud_store.load_trace(&run_id.0).expect("cloud trace loads");
    assert_eq!(cloud_tree, edge_tree, "trees must match byte-for-byte");

    // Run IDs are content-addressed; if the same agent runs again with
    // identical inputs, the run_id collides — that's union-merge semantics
    // in action. Listing traces shows exactly one entry.
    let cloud_traces = cloud_store.list_traces().unwrap();
    assert_eq!(cloud_traces, vec![run_id.0]);
}
