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

fn on_request_session(
  state :: { ok :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind:         "send_a2a",
    server:       "",
    tool:         "",
    args_json:    "",
    peer:         msg.from,
    a2a_topic:    "GrantSession",
    payload_json: "{\"charger_id\":\"charger-1\",\"power_kw\":30}",
    prompt:       "",
  }]
}
