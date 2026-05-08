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

fn on_dispatch(
  state :: { ready :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind:         "send_a2a",
    server:       "",
    tool:         "",
    args_json:    "",
    peer:         "depot",
    a2a_topic:    "RequestSession",
    payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":30}",
    prompt:       "",
  }]
}

fn on_grant(
  state :: { ready :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
