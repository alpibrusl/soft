//! Single agent defined entirely in Lex (no Rust-side `AgentConfig`
//! builder calls). Demonstrates the soft-agent DSL preamble + the
//! `Runner::from_lex_source` loader.
//!
//! Run with:
//!   cargo run -p soft-agent --example single_lex

use serde_json::json;
use soft_agent::{A2aMessage, Mailbox, Runner};
use tempfile::tempdir;

const VEHICLE_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("vehicle")
  |> agent_peers(["depot", "tms"])
  |> agent_mcp_servers(["telemetry", "local_planner"])
  |> agent_effects(["llm_local", "mcp", "a2a"])
  |> agent_specs(["specs/soc_reserve.spec"])
  |> agent_system_prompt("You are an autonomous truck.")
  |> agent_handles([
       { topic: "Dispatch",     fn_name: "on_dispatch" },
       { topic: "GrantSession", fn_name: "on_grant" },
       { topic: "DenySession",  fn_name: "on_deny" },
     ])
}

fn enough_soc(soc :: Float, energy :: Float, reserve :: Float) -> Bool {
  soc - energy >= reserve
}

fn on_dispatch(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  if enough_soc(state.soc, state.energy_needed, state.reserve) {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "Acknowledge",
      payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: "depot", a2a_topic: "RequestSession",
      payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":50}", prompt: "",
    }]
  }
}

fn on_grant(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Complete",
    payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
  }]
}

fn on_deny(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Failed",
    payload_json: "{\"reason\":\"depot_denied\"}", prompt: "",
  }]
}
"#;

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
    println!(
        "  mcp allowlist (runtime): {{telemetry, local_planner}} — `ocpp` blocked"
    );
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
