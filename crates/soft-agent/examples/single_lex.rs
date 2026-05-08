//! Single agent defined entirely in Lex (no Rust-side `AgentConfig`
//! builder calls). Demonstrates the soft-agent DSL preamble + the
//! `Runner::from_lex_source` loader.
//!
//! Run with:
//!   cargo run -p soft-agent --example single_lex

use serde_json::json;
use soft_agent::{A2aMessage, Mailbox, Runner};
use tempfile::tempdir;

const VEHICLE_LEX: &str = include_str!("lex/single_vehicle.lex");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== single-agent demo (config + handlers entirely in Lex) ===\n");

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(VEHICLE_LEX)?
        .mailbox(mailbox)
        .state(json!({"soc": 0.85, "reserve": 0.20, "energy_needed": 0.30}))
        .build()?;

    let agent = runner.agent();
    println!("agent: {}", agent.id().as_str());
    println!(
        "  effects: {:?}",
        agent.effects().iter().map(|e| e.name()).collect::<Vec<_>>()
    );
    println!("  mcp allowlist (runtime): {{telemetry, local_planner}} — `ocpp` blocked");
    println!("  system prompt: {:?}", agent.system_prompt());
    println!();

    println!("--- round 1: SoC plenty, vehicle accepts dispatch directly ---");
    sender.send(A2aMessage {
        from: "tms".into(),
        topic: "Dispatch".into(),
        payload: json!({"delivery_id": "d-1"}),
    })?;
    let r = runner.drain()?;
    println!(
        "  drained: messages={} allowed={} denied={}",
        r.messages, r.total_allowed, r.total_denied
    );

    println!("\n--- round 2: SoC low, vehicle requests charging ---");
    *runner.state_mut() = json!({"soc": 0.30, "reserve": 0.20, "energy_needed": 0.30});
    sender.send(A2aMessage {
        from: "tms".into(),
        topic: "Dispatch".into(),
        payload: json!({"delivery_id": "d-2"}),
    })?;
    let r = runner.drain()?;
    println!(
        "  drained: messages={} allowed={} denied={}",
        r.messages, r.total_allowed, r.total_denied
    );

    let dir = tempdir()?;
    let store = lex_store::Store::open(dir.path())?;
    let run_id = runner.finalize(&store)?;
    let tree = store.load_trace(&run_id.0)?;
    println!("\n--- trace: {} events ---", tree.nodes.len());
    for node in tree.nodes.iter().take(10) {
        let summary = node
            .input
            .get("kind")
            .and_then(|v| v.as_str())
            .or_else(|| node.input.get("topic").and_then(|v| v.as_str()))
            .unwrap_or("");
        println!("  {:<22} {}", node.target, summary);
    }
    if tree.nodes.len() > 10 {
        println!("  ... ({} more)", tree.nodes.len() - 10);
    }

    let counts = soft_agent::replay::scan_trace(&tree);
    println!(
        "\nreplay counts: proposed={} allowed={} denied={} executed={}",
        counts.proposed, counts.allowed, counts.denied, counts.executed
    );
    println!("\n=== done ===");
    Ok(())
}
