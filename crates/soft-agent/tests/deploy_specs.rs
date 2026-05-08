//! End-to-end test for the deploy-fleet spec gates.
//!
//! Loads `agents/depot.lex` + `agents/depot.spec` straight from the
//! repo (and `agents/vehicle.lex` + `agents/vehicle.spec`), wires up
//! `record_bindings` (the `BindingsFn` that `soft-runner`
//! installs), and asserts:
//!
//!   - the depot grants when (current + power_kw) ≤ (budget + pv);
//!     `power_kw` is read from the *action's* SendA2a payload, not
//!     from state — exercising action-derived bindings.
//!   - the depot denies when committing to that power would exceed
//!     the grid budget,
//!   - the vehicle's spec allows a Dispatch when soc - energy ≥ reserve,
//!   - the vehicle's spec denies when the action would drop SOC below
//!     reserve,
//!   - and that the GrantSession payload contract (`power_kw` field
//!     embedded by the lex handler) is honored end-to-end.
//!
//! These specs are the soft-agent test harness's lifted Phase 1 specs
//! brought into the deploy fleet — wiring them here means CI catches a
//! regression in the gate path independent of the runner CLI.

use std::path::PathBuf;

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::json;
use soft_agent::{record_bindings, A2aMessage, Gate, Mailbox, Runner, StepReport, Verdict};

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
        .bindings_fn(Box::new(record_bindings))
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
        .bindings_fn(Box::new(record_bindings))
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
        .bindings_fn(Box::new(record_bindings))
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
        .bindings_fn(Box::new(record_bindings))
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
fn depot_spec_evaluates_directly_with_record_bindings() {
    // Sanity: the on-disk depot.spec compiles and accepts record-shaped
    // `s` and `a` bindings of the kind `record_bindings` produces. Unit-
    // level proof that the lex-lang 0.3 record-quantifier path works
    // against our actual deploy-fleet spec.
    let spec_src = read_spec("depot");
    let gate = Gate::from_sources(&[&spec_src], "fn _host() -> Int { 0 }").unwrap();

    let allow_b = build_depot_record_bindings(30.0, 100.0, 0.0, 50.0);
    assert!(matches!(gate.evaluate(&allow_b), Verdict::Allow));

    let deny_b = build_depot_record_bindings(80.0, 100.0, 0.0, 50.0);
    assert!(matches!(gate.evaluate(&deny_b), Verdict::Deny { .. }));
}

fn build_depot_record_bindings(
    current_kw: f64,
    budget_kw: f64,
    pv_kw: f64,
    power_kw: f64,
) -> IndexMap<String, LexValue> {
    let mut s = IndexMap::new();
    s.insert("current_kw".into(), LexValue::Float(current_kw));
    s.insert("budget_kw".into(), LexValue::Float(budget_kw));
    s.insert("pv_kw".into(), LexValue::Float(pv_kw));

    let mut a = IndexMap::new();
    a.insert("power_kw".into(), LexValue::Float(power_kw));

    let mut b = IndexMap::new();
    b.insert("s".into(), LexValue::Record(s));
    b.insert("a".into(), LexValue::Record(a));
    b
}

#[test]
fn depot_grant_carries_power_kw_in_payload() {
    // The depot's GrantSession action embeds `power_kw` so the spec
    // can check it. Without that field, `record_bindings`
    // wouldn't bind `power_kw` and the spec would be Inconclusive (a
    // soft-Deny under the runner's fail-safe policy). This test pins
    // the contract by exercising the gate path end-to-end and
    // checking that *Allow* — not Inconclusive — is the verdict.
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
        .bindings_fn(Box::new(record_bindings))
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "vehicle".into(),
            topic: "RequestSession".into(),
            payload: json!({"vehicle_id": "v-1", "power_kw": 50}),
        })
        .unwrap();

    let report = runner.step().expect("step ok");
    match report {
        StepReport::Processed { allowed, denied } => {
            assert_eq!(
                allowed, 1,
                "with power_kw in the GrantSession payload the spec must Allow"
            );
            assert_eq!(denied, 0);
        }
        other => panic!("expected Processed, got {other:?}"),
    }
}
