# depot.lex — charging depot. Owns its grid budget; receives PV updates
# from a pv-agent.
#
# State:    { current_kw :: Float, budget_kw :: Float, pv_kw :: Float, requested_kw :: Float }
# In:       RequestSession (from vehicle), PvUpdate (from pv)
# Out:      GrantSession | DenySession (to vehicle)

fn config() -> AgentConfig {
  agent_new("depot")
  |> agent_peers(["vehicle", "pv"])
  |> agent_effects(["a2a"])
  |> agent_specs(["depot.spec"])
  |> agent_handles([
       { topic: "RequestSession", fn_name: "on_request_session" },
       { topic: "PvUpdate",       fn_name: "on_pv_update" },
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

# pv-agent broadcasts PvUpdate every tick. We bump pv_kw by 5.0 per
# update — minimal proof of state mutation in pure Lex. Real production
# would parse a numeric value out of msg.payload_json once the Lex stdlib
# exposes JSON parsing.
fn on_pv_update(
  state :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float, requested_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float, requested_kw :: Float },
       actions :: List[ActionRecord] } {
  { state: { current_kw: state.current_kw, budget_kw: state.budget_kw,
             pv_kw: state.pv_kw + 5.0, requested_kw: state.requested_kw },
    actions: [] }
}
