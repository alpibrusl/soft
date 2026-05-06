//! Multi-agent depot demo, all three agents authored entirely in Lex
//! (config + handlers). Soft-agent's DSL preamble ([`soft_agent::DSL_PREAMBLE`])
//! is auto-prepended by [`Runner::from_lex_source`].
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
    A2aMessage, Action, Gate, InProcessRouter, Mailbox, MailboxSender, Runner,
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
fn config() -> AgentConfig {
  agent_new("vehicle")
  |> agent_peers(["depot", "tms"])
  |> agent_effects(["a2a"])
  |> agent_specs(["specs/soc_reserve.spec"])
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

const DEPOT_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("depot")
  |> agent_peers(["vehicle"])
  |> agent_effects(["a2a"])
  |> agent_specs(["specs/grid_budget.spec"])
  |> agent_handles([
       { topic: "RequestSession", fn_name: "on_request_session" },
     ])
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
      payload_json: "{\"charger_id\":\"c-1\"}", prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "DenySession",
      payload_json: "{\"reason\":\"grid_budget\"}", prompt: "",
    }]
  }
}
"#;

const TMS_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("tms")
  |> agent_peers(["vehicle"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Acknowledge", fn_name: "terminate" },
       { topic: "Complete",    fn_name: "terminate" },
       { topic: "Failed",      fn_name: "terminate" },
     ])
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
    vehicle_state: Value,
    depot_state: Value,
}

fn fnum(state: &Value, key: &str, default: f64) -> f64 {
    state.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

fn vehicle_bindings_fn() -> soft_agent::BindingsFn {
    Box::new(|state, action| {
        let mut m = IndexMap::new();
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
    vehicle_state: Value,
    depot_state: Value,
) -> Result<(Runner, Runner, Runner, MailboxSender), Box<dyn std::error::Error>> {
    let soc_gate = Gate::from_sources(&[SOC_RESERVE_SPEC], HOST)?;
    let grid_gate = Gate::from_sources(&[GRID_BUDGET_SPEC], HOST)?;

    // Vehicle.
    let (v_box, v_send) = Mailbox::new();
    router.register("vehicle", v_send.clone());
    let vehicle = Runner::from_lex_source(VEHICLE_LEX)?
        .mailbox(v_box)
        .state(vehicle_state)
        .gate(soc_gate)
        .bindings_fn(vehicle_bindings_fn())
        .executor(Box::new(router.executor("vehicle")))
        .build()?;

    // Depot.
    let (d_box, d_send) = Mailbox::new();
    router.register("depot", d_send);
    let depot = Runner::from_lex_source(DEPOT_LEX)?
        .mailbox(d_box)
        .state(depot_state)
        .gate(grid_gate)
        .bindings_fn(depot_bindings_fn())
        .executor(Box::new(router.executor("depot")))
        .build()?;

    // TMS.
    let (t_box, t_send) = Mailbox::new();
    router.register("tms", t_send);
    let tms = Runner::from_lex_source(TMS_LEX)?
        .mailbox(t_box)
        .state(json!({"running": true}))
        .executor(Box::new(router.executor("tms")))
        .build()?;

    Ok((vehicle, depot, tms, v_send))
}

fn drain_until_quiescent(
    label: &str,
    vehicle: &mut Runner,
    depot: &mut Runner,
    tms: &mut Runner,
) {
    let mut totals = [(0usize, 0usize); 3];
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
        build_runners(&router, scenario.vehicle_state.clone(), scenario.depot_state.clone())
            .unwrap();

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
    println!(
        "=== multi-agent depot demo ({} authored entirely in Lex via DSL) ===",
        "vehicle + depot + tms"
    );

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
        return Err("replay invariant violated".into());
    }
    println!("\n{} traces; no spec was bypassed.", all_runs.len());
    Ok(())
}
