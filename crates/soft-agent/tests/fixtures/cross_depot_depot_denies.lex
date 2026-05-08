fn config() -> AgentConfig {
  agent_new("depot1")
  |> agent_peers(["vehicle"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "RequestSession", fn_name: "on_request" },
     ])
}

fn on_request(
  state :: { ok :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: msg.from, a2a_topic: "DenySession",
    payload_json: "{}", prompt: "",
  }]
}
