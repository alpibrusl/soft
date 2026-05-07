//! End-to-end test for the deploy-fleet spec gates.
//!
//! Loads `agents/depot.lex` + `agents/depot.spec` straight from the
//! repo (and `agents/vehicle.lex` + `agents/vehicle.spec`), wires up
//! the same `BindingsFn` that `soft-runner` installs, and asserts:
//!
//!   - the depot grants when (current + requested) ≤ (budget + pv),
//!   - the depot denies when it would exceed the grid budget,
//!   - the vehicle's spec allows a Dispatch when soc - energy ≥ reserve,
//!   - the vehicle's spec denies when the action would drop SOC below
//!     reserve.
//!
//! These specs are the soft-agent test harness's lifted Phase 1 specs
//! brought into the deploy fleet — wiring them here means CI catches a
//! regression in the gate path independent of the runner CLI.

use std::path::PathBuf;

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::{json, Value};
use soft_agent::{A2aMessage, Action, Gate, Mailbox, Runner, StepReport, Verdict};

fn agents_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("agents"))
        .expect("locate workspace agents/ dir")
}

fn read_agent(name: &str) -> String {
    let path = agents_dir().join(format!("{name}.lex"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_spec(name: &str) -> String {
    let path = agents_dir().join(format!("{name}.spec"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Same mapping `soft-runner::state_float_bindings` uses.
fn state_float_bindings(state: &Value, _action: &Action) -> IndexMap<String, LexValue> {
    let mut out = IndexMap::new();
    if let Some(obj) = state.as_object() {
        for (k, v) in obj {
            if let Some(f) = v.as_f64() {
                out.insert(k.clone(), LexValue::Float(f));
            }
        }
    }
    out
}

#[test]
fn depot_spec_compiles_against_stub_host() {
    let src = read_spec("depot");
    let gate = Gate::from_sources(&[&src], "fn _host() -> Int { 0 }")
        .expect("depot.spec compiles with stub host");
    assert_eq!(gate.spec_count(), 1);
}

#[test]
fn vehicle_spec_compiles_against_stub_host() {
    let src = read_spec("vehicle");
    let gate = Gate::from_sources(&[&src], "fn _host() -> Int { 0 }")
        .expect("vehicle.spec compiles with stub host");
    assert_eq!(gate.spec_count(), 1);
}

#[test]
fn depot_grants_when_within_budget() {
    let lex = read_agent("depot");
    let spec_src = read_spec("depot");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(&lex)
        .expect("depot.lex compiles")
        .mailbox(mailbox)
        .state(json!({
            "current_kw": 30.0,
            "budget_kw": 100.0,
            "pv_kw": 0.0,
            "requested_kw": 50.0,
        }))
        .gate(gate)
        .bindings_fn(Box::new(state_float_bindings))
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "vehicle".into(),
            topic: "RequestSession".into(),
            payload: json!({"vehicle_id": "v-1", "power_kw": 50}),
        })
        .unwrap();

    match runner.step().expect("step ok") {
        StepReport::Processed { allowed, denied } => {
            assert_eq!(allowed, 1, "depot should grant within-budget request");
            assert_eq!(denied, 0);
        }
        other => panic!("expected Processed, got {other:?}"),
    }
}

#[test]
fn depot_denies_when_over_budget() {
    let lex = read_agent("depot");
    let spec_src = read_spec("depot");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(&lex)
        .expect("depot.lex compiles")
        .mailbox(mailbox)
        .state(json!({
            "current_kw": 80.0,
            "budget_kw": 100.0,
            "pv_kw": 0.0,
            "requested_kw": 50.0,
        }))
        .gate(gate)
        .bindings_fn(Box::new(state_float_bindings))
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "vehicle".into(),
            topic: "RequestSession".into(),
            payload: json!({"vehicle_id": "v-1", "power_kw": 50}),
        })
        .unwrap();

    match runner.step().expect("step ok") {
        StepReport::Processed { allowed, denied } => {
            assert_eq!(
                allowed, 0,
                "depot should not allow when over budget — gate must deny"
            );
            assert_eq!(denied, 1);
        }
        other => panic!("expected Processed, got {other:?}"),
    }
}

#[test]
fn vehicle_dispatch_allowed_when_above_reserve() {
    let lex = read_agent("vehicle");
    let spec_src = read_spec("vehicle");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(&lex)
        .expect("vehicle.lex compiles")
        .mailbox(mailbox)
        .state(json!({
            "soc": 0.85,
            "reserve": 0.20,
            "energy_needed": 0.30,
            "tried": 0,
        }))
        .gate(gate)
        .bindings_fn(Box::new(state_float_bindings))
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    match runner.step().expect("step ok") {
        StepReport::Processed { allowed, denied } => {
            assert_eq!(
                allowed, 1,
                "vehicle should ack dispatch when SOC reserve is safe"
            );
            assert_eq!(denied, 0);
        }
        other => panic!("expected Processed, got {other:?}"),
    }
}

#[test]
fn vehicle_dispatch_denied_when_below_reserve() {
    let lex = read_agent("vehicle");
    let spec_src = read_spec("vehicle");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(&lex)
        .expect("vehicle.lex compiles")
        .mailbox(mailbox)
        .state(json!({
            "soc": 0.30,
            "reserve": 0.25,
            "energy_needed": 0.20,
            "tried": 0,
        }))
        .gate(gate)
        .bindings_fn(Box::new(state_float_bindings))
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    match runner.step().expect("step ok") {
        StepReport::Processed { allowed, denied } => {
            assert_eq!(
                allowed, 0,
                "vehicle SOC reserve violated — gate must deny the action"
            );
            assert_eq!(denied, 1);
        }
        other => panic!("expected Processed, got {other:?}"),
    }
}

#[test]
fn depot_spec_evaluates_directly() {
    // Sanity: the spec body compiles and the bindings the runner
    // would build evaluate to the expected verdicts.
    let spec_src = read_spec("depot");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let mut allow_b = IndexMap::new();
    allow_b.insert("current_kw".into(), LexValue::Float(30.0));
    allow_b.insert("requested_kw".into(), LexValue::Float(50.0));
    allow_b.insert("budget_kw".into(), LexValue::Float(100.0));
    allow_b.insert("pv_kw".into(), LexValue::Float(0.0));
    assert!(matches!(gate.evaluate(&allow_b), Verdict::Allow));

    let mut deny_b = IndexMap::new();
    deny_b.insert("current_kw".into(), LexValue::Float(80.0));
    deny_b.insert("requested_kw".into(), LexValue::Float(50.0));
    deny_b.insert("budget_kw".into(), LexValue::Float(100.0));
    deny_b.insert("pv_kw".into(), LexValue::Float(0.0));
    assert!(matches!(gate.evaluate(&deny_b), Verdict::Deny { .. }));
}
