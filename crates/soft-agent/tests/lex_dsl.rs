//! Verifies the Lex agent DSL: a `config()` function in Lex is parsed
//! into a real `AgentConfig` and the agent's handler list is registered
//! through `handle_lex`.

use serde_json::json;
use soft_agent::{A2aMessage, Effect, Mailbox, Runner};

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
     ])
}

fn on_dispatch(
  state :: { ready :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: msg.from, a2a_topic: "Acknowledge",
    payload_json: "{}", prompt: "",
  }]
}

fn on_grant(
  state :: { ready :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
"#;

#[test]
fn lex_dsl_builds_a_runner_with_correct_config() {
    let (mailbox, _) = Mailbox::new();
    let runner = Runner::from_lex_source(VEHICLE_LEX)
        .expect("vehicle.lex compiles + parses")
        .mailbox(mailbox)
        .state(json!({"ready": true}))
        .build()
        .expect("runner builds");

    let agent = runner.agent();
    assert_eq!(agent.id().as_str(), "vehicle");
    assert!(agent.knows_peer("depot"));
    assert!(agent.knows_peer("tms"));
    assert!(agent.allows_mcp_server("telemetry"));
    assert!(agent.allows_mcp_server("local_planner"));
    assert!(!agent.allows_mcp_server("ocpp"));
    assert!(agent.effects().contains(Effect::LlmLocal));
    assert!(agent.effects().contains(Effect::Mcp));
    assert!(agent.effects().contains(Effect::A2a));
    assert!(!agent.effects().contains(Effect::LlmCloud));
    assert_eq!(agent.system_prompt(), Some("You are an autonomous truck."));
    assert_eq!(agent.spec_paths(), &["specs/soc_reserve.spec".to_string()]);
}

#[test]
fn lex_dsl_runner_dispatches_to_lex_handler() {
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(VEHICLE_LEX)
        .unwrap()
        .mailbox(mailbox)
        .state(json!({"ready": true}))
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    let report = runner.drain().unwrap();
    assert_eq!(report.messages, 1);
    assert_eq!(
        report.total_allowed, 1,
        "on_dispatch's SendA2a should execute"
    );
}

#[test]
fn lex_dsl_rejects_unknown_effect_in_config() {
    let bad_src = r#"
fn config() -> AgentConfig {
  agent_new("bad")
  |> agent_effects(["llm_local", "made_up_effect"])
}
"#;
    let err = Runner::from_lex_source(bad_src)
        .err()
        .expect("should fail fast");
    let msg = err.to_string();
    assert!(msg.contains("made_up_effect"), "got: {msg}");
}

#[test]
fn lex_dsl_handles_minimal_config() {
    // No handlers, no specs — just the bare minimum the validator accepts.
    let minimal = r#"
fn config() -> AgentConfig {
  agent_new("minimal")
  |> agent_effects(["a2a"])
  |> agent_handles([])
}
"#;
    let (mailbox, _) = Mailbox::new();
    let runner = Runner::from_lex_source(minimal)
        .unwrap()
        .mailbox(mailbox)
        .build()
        .unwrap();
    assert_eq!(runner.agent().id().as_str(), "minimal");
}

#[test]
fn lex_dsl_pure_unit_test_via_parse_lex_config() {
    // Round-trip the DSL helpers without going through Runner.
    use soft_agent::parse_lex_config;
    use soft_agent::LexHost;

    let src = format!(
        "{}\n{}",
        soft_agent::DSL_PREAMBLE,
        r#"
fn config() -> AgentConfig {
  agent_new("probe")
  |> agent_peers(["a", "b", "c"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "T1", fn_name: "h1" },
       { topic: "T2", fn_name: "h2" },
     ])
}
"#
    );
    let host = LexHost::from_source(&src).unwrap();
    let result = host.call("config", Vec::new()).unwrap();
    let setup = parse_lex_config(&result.value).unwrap();

    let agent = setup.config.build().unwrap();
    assert_eq!(agent.id().as_str(), "probe");
    assert_eq!(setup.handlers.len(), 2);
    assert_eq!(setup.handlers[0], ("T1".into(), "h1".into()));
    assert_eq!(setup.handlers[1], ("T2".into(), "h2".into()));
}
