//! Multi-agent depot demo: vehicle + depot + tms in three Runners,
//! linked by an `InProcessRouter`. Exercises both Phase 1 specs
//! (`grid_budget` on depot, `soc_reserve` on vehicle) end-to-end.
//!
//! Three rounds with different initial state:
//!   A. Vehicle has plenty of SoC → accepts dispatch directly.
//!   B. Vehicle's SoC is low → requests charging; depot has headroom → grants.
//!   C. Same as B but depot is near grid budget → denies.
//!
//! Run with:
//!   cargo run -p soft-agent --example multi_depot

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::{json, Value};
use soft_agent::{
    A2aMessage, Action, AgentConfig, Effect, Gate, InProcessRouter, LexHost,
    Mailbox, MailboxSender, Runner,
};
use tempfile::tempdir;

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

fn soc_after(current :: Float, used :: Float) -> Float {
  current - used
}

fn above_reserve(current :: Float, used :: Float, reserve :: Float) -> Bool {
  soc_after(current, used) >= reserve
}
"#;

const GRID_BUDGET_SPEC: &str = r#"
spec grid_budget {
  forall current_kw :: Float, delta_kw :: Float, grid_kw :: Float, pv_kw :: Float:
    under_budget(current_kw, delta_kw, grid_kw, pv_kw)
}
"#;

const SOC_RESERVE_SPEC: &str = r#"
spec soc_reserve {
  forall current_soc :: Float, energy_used :: Float, reserve :: Float:
    above_reserve(current_soc, energy_used, reserve)
}
"#;

const VEHICLE_LEX: &str = r#"
type ActionRecord = {
  kind :: Str, server :: Str, tool :: Str, args_json :: Str,
  peer :: Str, a2a_topic :: Str, payload_json :: Str, prompt :: Str,
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
      payload_json: "{\"delivery_id\":\"d-1\"}",
      prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: "depot", a2a_topic: "RequestSession",
      payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":50}",
      prompt: "",
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
    payload_json: "{\"delivery_id\":\"d-1\"}",
    prompt: "",
  }]
}

fn on_deny(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Failed",
    payload_json: "{\"reason\":\"depot_denied\"}",
    prompt: "",
  }]
}
"#;

const DEPOT_LEX: &str = r#"
type ActionRecord = {
  kind :: Str, server :: Str, tool :: Str, args_json :: Str,
  peer :: Str, a2a_topic :: Str, payload_json :: Str, prompt :: Str,
}

fn within(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  current + delta <= grid + pv
}

fn on_request_session(
  state :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float, requested_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  if within(state.current_kw, state.requested_kw, state.budget_kw, state.pv_kw) {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "GrantSession",
      payload_json: "{\"charger_id\":\"c-1\"}",
      prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "DenySession",
      payload_json: "{\"reason\":\"grid_budget\"}",
      prompt: "",
    }]
  }
}
"#;

// TMS just terminates: any incoming message → no further action.
const TMS_LEX: &str = r#"
type ActionRecord = {
  kind :: Str, server :: Str, tool :: Str, args_json :: Str,
  peer :: Str, a2a_topic :: Str, payload_json :: Str, prompt :: Str,
}

fn terminate(
  state :: { running :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
"#;

#[derive(Clone)]
struct Scenario {
    label: &'static str,
    vehicle_state: serde_json::Value,
    depot_state: serde_json::Value,
}

fn fnum(state: &Value, key: &str, default: f64) -> f64 {
    state.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

fn vehicle_bindings_fn() -> soft_agent::BindingsFn {
    Box::new(|state, action| {
        let mut m = IndexMap::new();
        // Only "Acknowledge" actually consumes energy in this model. Asking
        // for a charging session does not.
        let used = if let Action::SendA2a { topic, .. } = action {
            if topic == "Acknowledge" {
                fnum(state, "energy_needed", 0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        m.insert("current_soc".into(), LexValue::Float(fnum(state, "soc", 1.0)));
        m.insert("energy_used".into(), LexValue::Float(used));
        m.insert("reserve".into(), LexValue::Float(fnum(state, "reserve", 0.2)));
        m
    })
}

fn depot_bindings_fn() -> soft_agent::BindingsFn {
    Box::new(|state, action| {
        let mut m = IndexMap::new();
        // Only an outbound "GrantSession" implies a future grid load delta.
        let delta = if let Action::SendA2a { topic, .. } = action {
            if topic == "GrantSession" {
                fnum(state, "requested_kw", 0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        m.insert("current_kw".into(), LexValue::Float(fnum(state, "current_kw", 0.0)));
        m.insert("delta_kw".into(), LexValue::Float(delta));
        m.insert("grid_kw".into(), LexValue::Float(fnum(state, "budget_kw", 0.0)));
        m.insert("pv_kw".into(), LexValue::Float(fnum(state, "pv_kw", 0.0)));
        m
    })
}

fn build_runners(
    router: &InProcessRouter,
    vehicle_state: serde_json::Value,
    depot_state: serde_json::Value,
) -> std::io::Result<(Runner, Runner, Runner, MailboxSender)> {
    let vehicle_host = LexHost::from_source(VEHICLE_LEX).expect("vehicle lex compiles");
    let depot_host = LexHost::from_source(DEPOT_LEX).expect("depot lex compiles");
    let tms_host = LexHost::from_source(TMS_LEX).expect("tms lex compiles");
    let soc_gate = Gate::from_sources(&[SOC_RESERVE_SPEC], HOST).expect("soc gate compiles");
    let grid_gate = Gate::from_sources(&[GRID_BUDGET_SPEC], HOST).expect("grid gate compiles");

    // Vehicle.
    let vehicle_agent = AgentConfig::new("vehicle")
        .peers(["depot", "tms"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (v_box, v_send) = Mailbox::new();
    router.register("vehicle", v_send.clone());
    let vehicle = Runner::builder()
        .agent(vehicle_agent)
        .state(vehicle_state)
        .mailbox(v_box)
        .lex_host(vehicle_host)
        .gate(soc_gate)
        .bindings_fn(vehicle_bindings_fn())
        .handle_lex("Dispatch", "on_dispatch")
        .handle_lex("GrantSession", "on_grant")
        .handle_lex("DenySession", "on_deny")
        .executor(Box::new(router.executor("vehicle")))
        .build()
        .unwrap();

    // Depot.
    let depot_agent = AgentConfig::new("depot")
        .peers(["vehicle"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (d_box, d_send) = Mailbox::new();
    router.register("depot", d_send);
    let depot = Runner::builder()
        .agent(depot_agent)
        .state(depot_state)
        .mailbox(d_box)
        .lex_host(depot_host)
        .gate(grid_gate)
        .bindings_fn(depot_bindings_fn())
        .handle_lex("RequestSession", "on_request_session")
        .executor(Box::new(router.executor("depot")))
        .build()
        .unwrap();

    // TMS.
    let tms_agent = AgentConfig::new("tms")
        .peers(["vehicle"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (t_box, t_send) = Mailbox::new();
    router.register("tms", t_send);
    let tms = Runner::builder()
        .agent(tms_agent)
        .state(json!({"running": true}))
        .mailbox(t_box)
        .lex_host(tms_host)
        .handle_lex("Acknowledge", "terminate")
        .handle_lex("Complete", "terminate")
        .handle_lex("Failed", "terminate")
        .executor(Box::new(router.executor("tms")))
        .build()
        .unwrap();

    Ok((vehicle, depot, tms, v_send))
}

fn drain_until_quiescent(
    label: &str,
    vehicle: &mut Runner,
    depot: &mut Runner,
    tms: &mut Runner,
) {
    let mut totals = [(0usize, 0usize); 3]; // (allowed, denied) for v, d, t
    for round in 0..20 {
        let v = vehicle.drain().unwrap();
        let d = depot.drain().unwrap();
        let t = tms.drain().unwrap();
        totals[0].0 += v.total_allowed;
        totals[0].1 += v.total_denied;
        totals[1].0 += d.total_allowed;
        totals[1].1 += d.total_denied;
        totals[2].0 += t.total_allowed;
        totals[2].1 += t.total_denied;
        if v.messages + d.messages + t.messages == 0 {
            println!(
                "  {label}: settled after {round} rounds; \
                 v(allow={},deny={}) d(allow={},deny={}) t(allow={},deny={})",
                totals[0].0, totals[0].1,
                totals[1].0, totals[1].1,
                totals[2].0, totals[2].1,
            );
            return;
        }
    }
    println!("  {label}: did not settle in 20 rounds");
}

fn run_scenario(scenario: &Scenario, store: &lex_store::Store) -> Vec<lex_trace::RunId> {
    println!("\n=== Scenario {} ===", scenario.label);
    let router = InProcessRouter::new();
    let (mut vehicle, mut depot, mut tms, vehicle_seed) =
        build_runners(&router, scenario.vehicle_state.clone(), scenario.depot_state.clone()).unwrap();

    // Seed: TMS dispatches a delivery to vehicle.
    vehicle_seed
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    drain_until_quiescent(scenario.label, &mut vehicle, &mut depot, &mut tms);

    let v_run = vehicle.finalize(store).expect("vehicle trace persists");
    let d_run = depot.finalize(store).expect("depot trace persists");
    let t_run = tms.finalize(store).expect("tms trace persists");
    vec![v_run, d_run, t_run]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== multi-agent depot demo (vehicle + depot + tms over InProcessRouter) ===");

    let dir = tempdir()?;
    let store = lex_store::Store::open(dir.path())?;

    let scenarios = [
        Scenario {
            label: "A: vehicle has SoC, accepts directly",
            vehicle_state: json!({"soc": 0.85, "reserve": 0.20, "energy_needed": 0.30}),
            depot_state: json!({"current_kw": 0.0, "budget_kw": 100.0, "pv_kw": 0.0, "requested_kw": 0.0}),
        },
        Scenario {
            label: "B: low SoC, depot has headroom → grants",
            vehicle_state: json!({"soc": 0.30, "reserve": 0.20, "energy_needed": 0.30}),
            depot_state: json!({"current_kw": 30.0, "budget_kw": 100.0, "pv_kw": 20.0, "requested_kw": 50.0}),
        },
        Scenario {
            label: "C: low SoC, depot near budget → denies",
            vehicle_state: json!({"soc": 0.30, "reserve": 0.20, "energy_needed": 0.30}),
            depot_state: json!({"current_kw": 90.0, "budget_kw": 100.0, "pv_kw": 20.0, "requested_kw": 50.0}),
        },
    ];

    let mut all_runs = Vec::new();
    for scenario in &scenarios {
        let runs = run_scenario(scenario, &store);
        all_runs.extend(runs);
    }

    // Roll up via the same scan_trace the soft-replay binary uses.
    println!("\n=== replay rollup ===");
    let mut totals = soft_agent::replay::Counts::default();
    for run in &all_runs {
        let tree = store.load_trace(&run.0)?;
        let c = soft_agent::replay::scan_trace(&tree);
        totals.add(&c);
    }
    println!(
        "  proposed={} allowed={} denied={} inconclusive={} executed={} skipped={}",
        totals.proposed, totals.allowed, totals.denied,
        totals.inconclusive, totals.executed, totals.skipped,
    );
    if totals.has_violation() {
        println!("\nVIOLATION: executed > allowed across {} traces", all_runs.len());
        return Err("replay invariant violated".into());
    }
    println!("\n{} traces; no spec was bypassed.", all_runs.len());
    Ok(())
}
