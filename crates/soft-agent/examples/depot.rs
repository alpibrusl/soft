//! End-to-end depot demo: build a depot agent, wire a runner with the
//! real Phase 1 grid_budget spec, send messages with realistic state,
//! and persist the trace through `lex_store::Store::save_trace`.
//!
//! Also demonstrates [`LexHost`] — calls a Lex helper function and shows
//! the resulting per-call trace tree.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p soft-agent --example depot
//! ```

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::json;
use soft_agent::{
    A2aMessage, Action, AgentConfig, Effect, Gate, LexHost, Mailbox, MockExecutor, Runner,
};
use tempfile::tempdir;

// Lex host program with helpers the spec calls into.
const HOST: &str = r#"
fn projected_load(current :: Float, delta :: Float) -> Float {
  current + delta
}

fn budget_total(grid :: Float, pv :: Float) -> Float {
  grid + pv
}

fn under_budget(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  projected_load(current, delta) <= budget_total(grid, pv)
}
"#;

// Real Phase 1 grid_budget spec — projects four scalars from
// (depot state, proposed action), delegates the check to an `under_budget`
// helper compiled into the host program.
const GRID_BUDGET_SPEC: &str = r#"
spec grid_budget {
  forall current_kw :: Float, delta_kw :: Float, grid_kw :: Float, pv_kw :: Float:
    under_budget(current_kw, delta_kw, grid_kw, pv_kw)
}
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== soft-agent depot demo ===\n");

    // ---------- LexHost demonstration ----------
    // Calling a Lex helper directly under a Recorder. The trace tree
    // captures the outer `under_budget` call plus the two nested calls
    // it makes (`projected_load`, `budget_total`).
    println!("--- LexHost ---");
    let host = LexHost::from_source(HOST)?;
    let probe = host.call(
        "under_budget",
        vec![
            LexValue::Float(80.0),
            LexValue::Float(30.0),
            LexValue::Float(100.0),
            LexValue::Float(20.0),
        ],
    )?;
    println!(
        "  under_budget(80, 30, 100, 20) = {} (nested calls captured: {})",
        probe.value.to_json(),
        count_calls(&probe.tree.nodes)
    );

    // ---------- Build the agent ----------
    let agent = AgentConfig::new("depot")
        .peers(["vehicle", "tms"])
        .mcp_servers(["optimizer", "ocpp"])
        .effects([Effect::LlmCloud, Effect::Mcp, Effect::A2a])
        .system_prompt("You manage a depot.")
        .specs(["specs/grid_budget.spec"])
        .build()?;
    println!("\n--- Agent ---");
    println!(
        "  built {} effects={:?} mcp={{ocpp,optimizer}}",
        agent.id().as_str(),
        agent.effects().iter().map(|e| e.name()).collect::<Vec<_>>()
    );

    // ---------- Build the gate ----------
    let gate = Gate::from_sources(&[GRID_BUDGET_SPEC], HOST)?;
    println!("  gate: {} spec(s)", gate.spec_count());

    // ---------- Wire the runner ----------
    let (mailbox, sender) = Mailbox::new();

    let mut runner = Runner::builder()
        .agent(agent)
        // Initial site state: 80 kW currently flowing, 100 kW budget,
        // 20 kW PV — leaves 40 kW of headroom.
        .state(json!({
            "site_id": "depot-madrid",
            "active_kw": 80.0,
            "grid_budget_kw": 100.0,
            "pv_available_kw": 20.0,
        }))
        .mailbox(mailbox)
        .gate(gate)
        .bindings_fn(Box::new(|state, action| {
            let current_kw = state.get("active_kw").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let grid_kw = state
                .get("grid_budget_kw")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let pv_kw = state
                .get("pv_available_kw")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let delta_kw = if let Action::CallMcp { args, tool, .. } = action {
                if tool == "remote_start_transaction" {
                    args.get("power_kw").and_then(|v| v.as_f64()).unwrap_or(0.0)
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let mut m = IndexMap::new();
            m.insert("current_kw".into(), LexValue::Float(current_kw));
            m.insert("delta_kw".into(), LexValue::Float(delta_kw));
            m.insert("grid_kw".into(), LexValue::Float(grid_kw));
            m.insert("pv_kw".into(), LexValue::Float(pv_kw));
            m
        }))
        .executor(Box::new(MockExecutor::new()))
        .handle("RequestSession", |_state, msg| {
            let power_kw = msg
                .payload
                .get("power_kw")
                .and_then(|v| v.as_f64())
                .unwrap_or(50.0);
            vec![Action::CallMcp {
                server: "ocpp".into(),
                tool: "remote_start_transaction".into(),
                args: json!({
                    "charger_id": "charger-1",
                    "vehicle_id": msg.payload.get("vehicle_id"),
                    "power_kw": power_kw,
                }),
            }]
        })
        .build()?;

    // ---------- Round 1: 30 kW request — fits (80+30=110 ≤ 120) ----------
    println!("\n--- round 1: vehicle requests 30 kW (state 80 kW, budget 100+20=120) ---");
    sender.send(A2aMessage {
        from: "vehicle".into(),
        topic: "RequestSession".into(),
        payload: json!({"vehicle_id": "v-1", "power_kw": 30.0}),
    })?;
    let r1 = runner.drain()?;
    println!(
        "  drained: messages={} allowed={} denied={}",
        r1.messages, r1.total_allowed, r1.total_denied
    );

    // ---------- Round 2: 50 kW request — exceeds (80+50=130 > 120) ----------
    println!("\n--- round 2: vehicle requests 50 kW (would exceed budget) ---");
    sender.send(A2aMessage {
        from: "vehicle".into(),
        topic: "RequestSession".into(),
        payload: json!({"vehicle_id": "v-2", "power_kw": 50.0}),
    })?;
    let r2 = runner.drain()?;
    println!(
        "  drained: messages={} allowed={} denied={}",
        r2.messages, r2.total_allowed, r2.total_denied
    );

    // ---------- Round 3: lower active load to 40 kW, retry 50 kW ----------
    runner
        .state_mut()
        .as_object_mut()
        .unwrap()
        .insert("active_kw".into(), json!(40.0));
    println!("\n--- round 3: site now 40 kW, retry 50 kW (40+50=90 ≤ 120) ---");
    sender.send(A2aMessage {
        from: "vehicle".into(),
        topic: "RequestSession".into(),
        payload: json!({"vehicle_id": "v-2", "power_kw": 50.0}),
    })?;
    let r3 = runner.drain()?;
    println!(
        "  drained: messages={} allowed={} denied={}",
        r3.messages, r3.total_allowed, r3.total_denied
    );

    // ---------- Persist + reload trace ----------
    let dir = tempdir()?;
    let store = lex_store::Store::open(dir.path())?;
    let run_id = runner.finalize(&store)?;

    println!("\n--- trace persisted ---");
    println!("  run_id: {}", run_id.0);
    println!(
        "  path:   {}/traces/{}/trace.json",
        dir.path().display(),
        run_id.0
    );

    let tree = store.load_trace(&run_id.0)?;
    println!("  events recorded: {}", tree.nodes.len());
    for node in &tree.nodes {
        let summary = node
            .input
            .get("kind")
            .and_then(|v| v.as_str())
            .or_else(|| node.input.get("topic").and_then(|v| v.as_str()))
            .unwrap_or("");
        println!("    {:<20} {}", node.target, summary);
    }

    println!("\n=== done ===");
    Ok(())
}

fn count_calls(nodes: &[lex_trace::TraceNode]) -> usize {
    let mut n = 0;
    for node in nodes {
        if matches!(node.kind, lex_trace::TraceNodeKind::Call) {
            n += 1;
        }
        n += count_calls(&node.children);
    }
    n
}
