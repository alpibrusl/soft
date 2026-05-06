//! Three-layer integration test: external A2A client posts a Message →
//! soft-a2a server forwards into a soft-agent Mailbox → Runner dispatches
//! to a Lex handler that returns an action record → executor runs it →
//! trace records every step and persists through `lex_store::Store`.
//!
//! This is the smallest end-to-end check that all four upstream
//! lex-lang 0.2 primitives (effects, MCP wire, spec-checker — not used
//! here but available, lex-trace) cooperate with both soft crates.

use serde_json::json;
use soft_a2a::{
    A2aClient, A2aServer, AgentCard, Message, MessageMetadata, Part, Role,
};
use soft_agent::{AgentConfig, Effect, LexHost, Mailbox, Runner};
use std::time::Duration;
use tempfile::tempdir;

/// Lex handler module: takes (state, msg) and emits a single SendA2a
/// action echoing the sender's name back. Demonstrates that Lex code
/// compiled via `LexHost` is wired into the runner's dispatch via
/// `RunnerBuilder::handle_lex`.
const HANDLER: &str = r#"
type ActionRecord = {
  kind         :: Str,
  server       :: Str,
  tool         :: Str,
  args_json    :: Str,
  peer         :: Str,
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
    payload_json: "{\"reply\":\"pong\"}",
    prompt:       "",
  }]
}
"#;

#[test]
fn a2a_inbound_drives_lex_handler_through_runner() {
    // ---- 1. Build soft-agent with a Lex handler ----
    let host = LexHost::from_source(HANDLER).expect("lex source compiles");

    let agent = AgentConfig::new("ping-bot")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .expect("agent config valid");

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({"ok": true}))
        .mailbox(mailbox)
        .lex_host(host)
        .handle_lex("Ping", "on_ping")
        .build()
        .expect("runner builds");

    // ---- 2. soft-a2a server forwards inbound Messages into the mailbox ----
    let card = AgentCard::new("ping-bot", "integration test", "http://127.0.0.1:0/");
    let server = A2aServer::bind("127.0.0.1:0", card, sender).expect("server binds");
    let addr = server.local_addr().expect("listener has an addr");
    let _server_thread = server.spawn();

    // Tiny grace period for the listener to be accepting connections.
    std::thread::sleep(Duration::from_millis(50));

    // ---- 3. External A2A client sends a Message ----
    let msg = Message {
        message_id: "m-1".into(),
        role: Role::User,
        parts: vec![Part::Data {
            data: json!({"x": 1, "y": "hello"}),
        }],
        task_id: None,
        metadata: Some(MessageMetadata {
            from: "caller".into(),
            topic: "Ping".into(),
        }),
    };
    A2aClient::new()
        .send(&format!("http://{addr}"), &msg)
        .expect("client send ok");

    // Wait a moment for the message to land in the mailbox.
    std::thread::sleep(Duration::from_millis(50));

    // ---- 4. Runner picks the message up, dispatches to Lex handler ----
    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1, "one inbound a2a message processed");
    assert_eq!(report.total_allowed, 1, "the SendA2a echo must execute");
    assert_eq!(report.total_denied, 0);

    // ---- 5. Trace round-trips through lex_store::Store ----
    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).expect("open store");
    let run_id = runner.finalize(&store).expect("finalize trace");
    let tree = store.load_trace(&run_id.0).expect("trace loads back");

    let targets: Vec<&str> = tree.nodes.iter().map(|n| n.target.as_str()).collect();
    assert!(
        targets.contains(&"a2a.received"),
        "trace should record a2a.received, got: {targets:?}"
    );
    assert!(
        targets.contains(&"action.proposed"),
        "trace should record action.proposed, got: {targets:?}"
    );
    assert!(
        targets.contains(&"action.executed"),
        "trace should record action.executed, got: {targets:?}"
    );
}
