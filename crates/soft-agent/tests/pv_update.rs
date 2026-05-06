//! pv-agent + depot, exercising the new tick mechanism *and* the
//! stateful-Lex-handler return shape.
//!
//! pv-agent ticks every 30ms, sending a `PvUpdate` action to depot.
//! depot's Lex handler returns `{ state, actions }`, replacing its own
//! `pv_kw` state. After a few ticks we drain both and assert that the
//! depot's state reflects the PV updates.

use serde_json::json;
use soft_agent::{InProcessRouter, Mailbox, Runner};
use std::time::Duration;

// pv-agent: every Tick, broadcast a PvUpdate to depot. The actual
// payload is the agent's own `pv_kw` value, hardcoded here as a string
// (lex-lang 0.2 stdlib doesn't ship JSON parsing for our nested-record
// payload, so we keep the value side simple — just demonstrates the
// flow). State stays unchanged.
const PV_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("pv")
  |> agent_peers(["depot"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Tick", fn_name: "on_tick" },
     ])
}

fn on_tick(
  state :: { pv_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "depot", a2a_topic: "PvUpdate",
    payload_json: "{}",
    prompt: "",
  }]
}
"#;

// depot: each PvUpdate bumps depot's `pv_kw` by 5.0. Demonstrates that
// the Lex handler can return `{ state, actions }` and the runner will
// swap state. Real production would parse a numeric value out of the
// payload; this is the minimal proof of the state-mutation pipeline.
const DEPOT_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("depot")
  |> agent_peers(["pv"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "PvUpdate", fn_name: "on_pv_update" },
     ])
}

fn on_pv_update(
  state :: { pv_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { pv_kw :: Float }, actions :: List[ActionRecord] } {
  { state: { pv_kw: state.pv_kw + 5.0 }, actions: [] }
}
"#;

#[test]
fn pv_agent_ticks_and_depot_state_updates() {
    let router = InProcessRouter::new();

    // pv-agent.
    let (pv_box, pv_send) = Mailbox::new();
    router.register("pv", pv_send.clone());
    let mut pv = Runner::from_lex_source(PV_LEX)
        .unwrap()
        .mailbox(pv_box)
        .state(json!({"pv_kw": 25.0}))
        .tick(Duration::from_millis(30), "Tick", pv_send.clone())
        .executor(Box::new(router.executor("pv")))
        .build()
        .unwrap();

    // depot.
    let (depot_box, depot_send) = Mailbox::new();
    router.register("depot", depot_send);
    let mut depot = Runner::from_lex_source(DEPOT_LEX)
        .unwrap()
        .mailbox(depot_box)
        .state(json!({"pv_kw": 0.0}))
        .executor(Box::new(router.executor("depot")))
        .build()
        .unwrap();

    // Wait long enough for ~3 ticks.
    std::thread::sleep(Duration::from_millis(100));

    // Drain pv (ticks emit PvUpdate to depot).
    let pv_report = pv.drain().expect("pv drain");
    assert!(pv_report.messages >= 2, "pv should have received >=2 ticks");

    // Drain depot (consumes the PvUpdate messages, mutates state).
    let depot_report = depot.drain().expect("depot drain");
    assert_eq!(depot_report.messages, pv_report.messages);

    let final_pv_kw = depot.state()["pv_kw"].as_f64().unwrap();
    assert!(
        final_pv_kw >= 10.0,
        "depot's pv_kw should have advanced via stateful Lex handler; got {final_pv_kw}"
    );
    assert_eq!(
        final_pv_kw,
        depot_report.messages as f64 * 5.0,
        "each PvUpdate bumps pv_kw by 5.0"
    );

    drop(pv);
    drop(depot);
}
