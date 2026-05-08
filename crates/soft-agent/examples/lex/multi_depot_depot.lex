fn config() -> AgentConfig {
  agent_new("depot")
  |> agent_peers(["vehicle"])
  |> agent_effects(["a2a"])
  |> agent_specs(["specs/grid_budget.spec"])
  |> agent_handles([
       { topic: "RequestSession", fn_name: "on_request_session" },
     ])
}

fn within(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  current + delta <= grid + pv
}

fn on_request_session(
  state :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float, requested_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  if within(state.current_kw, state.requested_kw, state.budget_kw, state.pv_kw) {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "GrantSession",
      payload_json: "{\"charger_id\":\"c-1\"}", prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "DenySession",
      payload_json: "{\"reason\":\"grid_budget\"}", prompt: "",
    }]
  }
}
