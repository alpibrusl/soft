//! Stateful Lex handlers: returning `{ state: ..., actions: [...] }`
//! lets a Lex handler propose actions *and* swap the runner's state in
//! one shot — without crossing the FFI boundary for a separate mutator.

use serde_json::json;
use soft_agent::{A2aMessage, Mailbox, Runner};

const COUNTER_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("counter")
  |> agent_peers(["caller"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Bump", fn_name: "on_bump" },
     ])
}

fn on_bump(
  state :: { count :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { count :: Int }, actions :: List[ActionRecord] } {
  { state: { count: state.count + 1 }, actions: [] }
}
"#;

#[test]
fn lex_handler_returning_state_updates_runner_state() {
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(COUNTER_LEX)
        .unwrap()
        .mailbox(mailbox)
        .state(json!({"count": 0}))
        .build()
        .unwrap();

    for _ in 0..3 {
        sender
            .send(A2aMessage {
                from: "caller".into(),
                topic: "Bump".into(),
                payload: json!(null),
            })
            .unwrap();
    }
    let report = runner.drain().unwrap();
    assert_eq!(report.messages, 3);
    assert_eq!(
        runner.state()["count"],
        3,
        "state should advance with each tick"
    );
}

const STATELESS_LEX: &str = r#"
fn config() -> AgentConfig {
  agent_new("stateless")
  |> agent_peers(["caller"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Probe", fn_name: "on_probe" },
     ])
}

fn on_probe(
  state :: { ok :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
"#;

#[test]
fn stateless_lex_handler_still_supported() {
    // Backward compat: a plain `List[ActionRecord]` return continues to
    // work and leaves state untouched.
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::from_lex_source(STATELESS_LEX)
        .unwrap()
        .mailbox(mailbox)
        .state(json!({"ok": true}))
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "Probe".into(),
            payload: json!(null),
        })
        .unwrap();
    runner.drain().unwrap();
    assert_eq!(runner.state(), &json!({"ok": true}));
}
