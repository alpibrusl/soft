//! Phase 1 spec set: `grid_budget` + `soc_reserve` exercised as runtime
//! gates against realistic scalar bindings.

use indexmap::IndexMap;
use lex_bytecode::Value;
use soft_agent::{Gate, Verdict};

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

fn float_bindings(pairs: &[(&str, f64)]) -> IndexMap<String, Value> {
    let mut m = IndexMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), Value::Float(*v));
    }
    m
}

#[test]
fn grid_budget_allows_when_load_within_budget() {
    let gate = Gate::from_sources(&[GRID_BUDGET_SPEC], HOST).unwrap();
    let v = gate.evaluate(&float_bindings(&[
        ("current_kw", 80.0),
        ("delta_kw", 30.0),
        ("grid_kw", 100.0),
        ("pv_kw", 20.0),
    ]));
    assert!(matches!(v, Verdict::Allow));
}

#[test]
fn grid_budget_denies_when_over() {
    let gate = Gate::from_sources(&[GRID_BUDGET_SPEC], HOST).unwrap();
    let v = gate.evaluate(&float_bindings(&[
        ("current_kw", 80.0),
        ("delta_kw", 50.0),
        ("grid_kw", 100.0),
        ("pv_kw", 20.0),
    ]));
    assert!(matches!(v, Verdict::Deny { .. }));
}

#[test]
fn soc_reserve_allows_when_safely_above_reserve() {
    let gate = Gate::from_sources(&[SOC_RESERVE_SPEC], HOST).unwrap();
    let v = gate.evaluate(&float_bindings(&[
        ("current_soc", 0.85),
        ("energy_used", 0.30),
        ("reserve", 0.20),
    ]));
    assert!(matches!(v, Verdict::Allow));
}

#[test]
fn soc_reserve_denies_when_action_would_drop_below_floor() {
    let gate = Gate::from_sources(&[SOC_RESERVE_SPEC], HOST).unwrap();
    let v = gate.evaluate(&float_bindings(&[
        ("current_soc", 0.30),
        ("energy_used", 0.20),
        ("reserve", 0.25),
    ]));
    match v {
        Verdict::Deny { spec_name, .. } => assert_eq!(spec_name, "soc_reserve"),
        other => panic!("expected Deny, got {other:?}"),
    }
}

#[test]
fn both_specs_compile_into_one_gate() {
    // Phase 1 reality: vehicle-agent's gate carries `soc_reserve`,
    // depot-agent's gate carries `grid_budget`. But spec-checker also
    // accepts a multi-spec gate; verify we can compose them when needed.
    let gate = Gate::from_sources(&[GRID_BUDGET_SPEC, SOC_RESERVE_SPEC], HOST).unwrap();
    assert_eq!(gate.spec_count(), 2);

    let mut bindings = float_bindings(&[
        ("current_kw", 80.0),
        ("delta_kw", 30.0),
        ("grid_kw", 100.0),
        ("pv_kw", 20.0),
        ("current_soc", 0.85),
        ("energy_used", 0.30),
        ("reserve", 0.20),
    ]);
    let _ = bindings.len();
    assert!(matches!(gate.evaluate(&bindings), Verdict::Allow));
}
