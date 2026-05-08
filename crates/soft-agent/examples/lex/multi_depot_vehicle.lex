fn config() -> AgentConfig {
  agent_new("vehicle")
  |> agent_peers(["depot", "tms"])
  |> agent_effects(["a2a"])
  |> agent_specs(["specs/soc_reserve.spec"])
  |> agent_handles([
       { topic: "Dispatch",     fn_name: "on_dispatch" },
       { topic: "GrantSession", fn_name: "on_grant" },
       { topic: "DenySession",  fn_name: "on_deny" },
     ])
}

fn enough_soc(soc :: Float, energy :: Float, reserve :: Float) -> Bool {
  soc - energy >= reserve
}

fn on_dispatch(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  if enough_soc(state.soc, state.energy_needed, state.reserve) {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: msg.from, a2a_topic: "Acknowledge",
      payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
    }]
  } else {
    [{
      kind: "send_a2a", server: "", tool: "", args_json: "",
      peer: "depot", a2a_topic: "RequestSession",
      payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":50}", prompt: "",
    }]
  }
}

fn on_grant(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Complete",
    payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
  }]
}

fn on_deny(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Failed",
    payload_json: "{\"reason\":\"depot_denied\"}", prompt: "",
  }]
}
