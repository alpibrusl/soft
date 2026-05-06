//! End-to-end depot demo: build a depot agent, wire a runner with a real
//! spec gate, send two messages with different state, and persist the
//! trace through `lex_store::Store::save_trace`.
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
    A2aMessage, Action, AgentConfig, Effect, Gate, Mailbox, MockExecutor, Runner,
};
use tempfile::tempdir;

const HOST: &str = "fn host() -> Int { 0 }";

// `spec-checker` 0.2 quantifiers are scalar (Int / Float / Bool / Str), so
// the demo spec binds a single `ok :: Bool` extracted from state in the
// runner's `bindings_fn`. Real Phase 1 specs (`grid_budget`, `soc_reserve`)
// likewise project the relevant scalars at gate time.
const DEMO_SPEC: &str = "
spec demo {
  forall ok :: Bool:
    ok
}
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== soft-agent depot demo ===\n");

    let agent = AgentConfig::new("depot")
        .peers(["vehicle", "tms"])
        .mcp_servers(["optimizer", "ocpp"])
        .effects([Effect::LlmCloud, Effect::Mcp, Effect::A2a])
        .system_prompt("You manage a depot.")
        .specs(["specs/grid_budget.spec"])
        .build()?;
    println!("Built agent: {} effects={:?} mcp={{ocpp,optimizer}}",
        agent.id().as_str(),
        agent.effects().iter().map(|e| e.name()).collect::<Vec<_>>());

    let gate = Gate::from_sources(&[DEMO_SPEC], HOST)?;
    println!("Built gate: {} spec(s)", gate.spec_count());

    let (mailbox, sender) = Mailbox::new();

    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({"ok": true})) // initial: gate will allow
        .mailbox(mailbox)
        .gate(gate)
        .bindings_fn(Box::new(|state, _action| {
            let mut m = IndexMap::new();
            let ok = state.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            m.insert("ok".into(), LexValue::Bool(ok));
            m
        }))
        .executor(Box::new(MockExecutor::new()))
        .handle(
            "RequestSession",
            Box::new(|state, _msg| {
                // The "LLM" proposes a single OCPP RemoteStartTransaction.
                let server = "ocpp".to_string();
                let proposal = vec![Action::CallMcp {
                    server,
                    tool: "remote_start_transaction".into(),
                    args: json!({"charger_id": "charger-1", "power_kw": 50.0}),
                }];
                eprintln!("  handler: state.ok = {}; proposing 1 action",
                    state.get("ok").unwrap_or(&json!(false)));
                proposal
            }),
        )
        .build()?;

    // ---- Round 1: state.ok = true → allowed ----
    println!("\n--- round 1: state.ok = true ---");
    sender.send(A2aMessage {
        from: "vehicle".into(),
        topic: "RequestSession".into(),
        payload: json!({"vehicle_id": "v-1"}),
    })?;
    let r1 = runner.drain()?;
    println!("  drained: messages={} allowed={} denied={}",
        r1.messages, r1.total_allowed, r1.total_denied);

    // ---- Round 2: flip state.ok → false → gate denies ----
    *runner.state_mut() = json!({"ok": false});
    println!("\n--- round 2: state.ok = false ---");
    sender.send(A2aMessage {
        from: "vehicle".into(),
        topic: "RequestSession".into(),
        payload: json!({"vehicle_id": "v-2"}),
    })?;
    let r2 = runner.drain()?;
    println!("  drained: messages={} allowed={} denied={}",
        r2.messages, r2.total_allowed, r2.total_denied);

    // ---- Persist + reload trace ----
    let dir = tempdir()?;
    let store = lex_store::Store::open(dir.path())?;
    let run_id = runner.finalize(&store)?;

    println!("\n--- trace persisted ---");
    println!("  run_id: {}", run_id.0);
    println!("  path:   {}/traces/{}/trace.json", dir.path().display(), run_id.0);

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
