//! v0 smoke tests: agent builder + effect set + mailbox + action mapping.
//!
//! Runtime loop, gate, and trace integration are not exercised here — they
//! land in the next slice.

use soft_agent::{Action, AgentConfig, Effect, Error, Mailbox, MailboxSender};

#[test]
fn build_vehicle_agent() {
    let agent = AgentConfig::new("vehicle")
        .peers(["depot", "tms"])
        .mcp_servers(["telemetry", "local_planner"])
        .effects([Effect::LlmLocal, Effect::Mcp, Effect::A2a])
        .system_prompt("You are an autonomous truck.")
        .specs(["specs/soc_reserve.spec"])
        .build()
        .expect("vehicle config should be valid");

    assert_eq!(agent.id().as_str(), "vehicle");
    assert!(agent.effects().contains(Effect::LlmLocal));
    assert!(!agent.effects().contains(Effect::LlmCloud));
    assert!(agent.allows_mcp_server("telemetry"));
    assert!(!agent.allows_mcp_server("ocpp"));
    assert!(agent.knows_peer("depot"));
    assert!(!agent.knows_peer("pv"));
    assert_eq!(agent.spec_paths(), &["specs/soc_reserve.spec".to_string()]);
    assert_eq!(agent.system_prompt(), Some("You are an autonomous truck."));
}

#[test]
fn build_depot_agent() {
    let agent = AgentConfig::new("depot")
        .peers(["vehicle", "tms", "pv"])
        .mcp_servers(["optimizer", "ocpp"])
        .effects([Effect::LlmCloud, Effect::Mcp, Effect::A2a])
        .specs(["specs/grid_budget.spec"])
        .build()
        .expect("depot config should be valid");

    assert!(agent.allows_mcp_server("ocpp"));
    assert!(!agent.allows_mcp_server("tms_db"));
    assert!(agent.effects().contains(Effect::LlmCloud));
}

#[test]
fn rejects_both_llm_effects() {
    let err = AgentConfig::new("frankenstein")
        .mcp_servers(["x"])
        .effects([Effect::LlmLocal, Effect::LlmCloud, Effect::Mcp])
        .build()
        .unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn rejects_empty_effects() {
    let err = AgentConfig::new("ghost").build().unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn rejects_mcp_without_servers() {
    let err = AgentConfig::new("naked-mcp")
        .effects([Effect::LlmLocal, Effect::Mcp])
        .build()
        .unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)));
}

#[test]
fn action_effect_mapping() {
    let cases = [
        (
            Action::CallMcp {
                server: "optimizer".into(),
                tool: "schedule_session".into(),
                args: serde_json::json!({}),
            },
            Effect::Mcp,
        ),
        (
            Action::SendA2a {
                peer: "depot".into(),
                topic: "Probe".into(),
                payload: serde_json::json!({}),
            },
            Effect::A2a,
        ),
        (
            Action::LocalLlm {
                prompt: "what's my SoC?".into(),
            },
            Effect::LlmLocal,
        ),
        (
            Action::CloudLlm {
                prompt: "plan the day".into(),
            },
            Effect::LlmCloud,
        ),
    ];
    for (action, expected) in cases {
        assert_eq!(action.effect(), expected, "wrong effect for {action:?}");
    }
}

#[test]
fn mailbox_round_trip() {
    let (mbox, sender): (Mailbox, MailboxSender) = Mailbox::new();
    sender
        .send(soft_agent::A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: serde_json::json!({"delivery_id": "d-1"}),
        })
        .unwrap();
    let msg = mbox.recv().unwrap();
    assert_eq!(msg.from, "tms");
    assert_eq!(msg.topic, "Dispatch");
    assert_eq!(msg.payload["delivery_id"], "d-1");
}
