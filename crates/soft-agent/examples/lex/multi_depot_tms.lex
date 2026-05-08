fn config() -> AgentConfig {
  agent_new("tms")
  |> agent_peers(["vehicle"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Acknowledge", fn_name: "terminate" },
       { topic: "Complete",    fn_name: "terminate" },
       { topic: "Failed",      fn_name: "terminate" },
     ])
}

fn terminate(
  state :: { running :: Bool },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  []
}
