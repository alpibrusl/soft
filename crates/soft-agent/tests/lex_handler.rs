//! Lex-side handlers: register a Lex function as the handler for a topic,
//! send an A2A message, verify the runner dispatches into Lex, parses the
//! returned action list, and runs the action through the gate + executor.

use serde_json::json;
use soft_agent::{A2aMessage, Action, AgentConfig, Effect, LexHost, Mailbox, Runner};
use tempfile::tempdir;

/// Lex handler module. Defines `on_ping` which echoes the sender's name
/// back as a SendA2a action. Note Lex's record type syntax `{ f :: T, ... }`
/// (not `Record[...]`) and the all-fields-present action record shape.
const HANDLERS: &str = r#"
type ActionRecord = {
  kind         :: Str,
  server       :: Str,
  tool         :: Str,
  args_json    :: Str,
  peer         :: Str,
  a2a_topic    :: Str,
  payload_json :: Str,
  prompt       :: Str,
}

fn on_ping(
  state :: { ok :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind:         "send_a2a",
    server:       "",
    tool:         "",
    args_json:    "",
    peer:         msg.from,
    a2a_topic:    "Pong",
    payload_json: "{\"reply\":\"pong\"}",
    prompt:       "",
  }]
}

fn always_empty(
  state :: { ok :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
"#;

#[test]
fn lex_handler_dispatches_to_action_list() {
    let host = LexHost::from_source(HANDLERS).expect("compiles");

    let agent = AgentConfig::new("ping-bot")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({"ok": true}))
        .mailbox(mailbox)
        .lex_host(host)
        .handle_lex("Ping", "on_ping")
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "Ping".into(),
            payload: json!({"x": 1}),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    assert_eq!(report.total_allowed, 1, "send_a2a action should be allowed");
    assert_eq!(report.total_denied, 0);

    // Verify trace round-trips.
    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).unwrap();
    let run_id = runner.finalize(&store).unwrap();
    let tree = store.load_trace(&run_id.0).unwrap();
    assert!(tree.nodes.iter().any(|n| n.target == "action.executed"));
}

#[test]
fn lex_handler_returning_empty_list_does_nothing() {
    let host = LexHost::from_source(HANDLERS).expect("compiles");
    let agent = AgentConfig::new("quiet")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({"ok": true}))
        .mailbox(mailbox)
        .lex_host(host)
        .handle_lex("Ping", "always_empty")
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "Ping".into(),
            payload: json!(null),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    assert_eq!(report.total_allowed, 0);
    assert_eq!(report.total_denied, 0);
}

#[test]
fn build_fails_when_lex_handler_has_no_host() {
    let agent = AgentConfig::new("orphan")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, _sender) = Mailbox::new();
    let result = Runner::builder()
        .agent(agent)
        .mailbox(mailbox)
        .handle_lex("Ping", "on_ping")
        // intentionally no .lex_host(...)
        .build();
    let err = result.err().expect("build should fail without lex_host");
    assert!(matches!(err, soft_agent::Error::InvalidConfig(_)));
}

#[test]
fn lex_and_rust_handlers_can_coexist() {
    let host = LexHost::from_source(HANDLERS).expect("compiles");
    let agent = AgentConfig::new("hybrid")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({"ok": true}))
        .mailbox(mailbox)
        .lex_host(host)
        .handle_lex("LexPing", "on_ping")
        .handle("RustPing", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "RustPong".into(),
                payload: json!({"reply": "rust-pong"}),
            }]
        })
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "LexPing".into(),
            payload: json!(null),
        })
        .unwrap();
    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "RustPing".into(),
            payload: json!(null),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 2);
    assert_eq!(report.total_allowed, 2);
}
