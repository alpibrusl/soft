fn config() -> AgentConfig {
  agent_new("vehicle")
  |> agent_peers(["depot1", "depot2", "tms"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Dispatch",     fn_name: "on_dispatch" },
       { topic: "DenySession",  fn_name: "on_deny" },
       { topic: "GrantSession", fn_name: "on_grant" },
     ])
}

fn on_dispatch(
  state :: { tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { tried :: Int }, actions :: List[ActionRecord] } {
  { state: { tried: 1 },
    actions: [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: "depot1", a2a_topic: "RequestSession",
      payload_json: "{}", prompt: "",
    }] }
}

fn on_deny(
  state :: { tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { tried :: Int }, actions :: List[ActionRecord] } {
  if state.tried >= 2 {
    # Already tried both depots — give up, tell tms.
    { state: { tried: state.tried },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: "tms", a2a_topic: "Failed",
        payload_json: "{}", prompt: "",
      }] }
  } else {
    # First denial — try depot2.
    { state: { tried: state.tried + 1 },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: "depot2", a2a_topic: "RequestSession",
        payload_json: "{}", prompt: "",
      }] }
  }
}

fn on_grant(
  state :: { tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Complete",
    payload_json: "{}", prompt: "",
  }]
}
