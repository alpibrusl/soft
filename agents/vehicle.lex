# vehicle.lex — autonomous truck.
#
# State expected:  { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int }
# Topics in:       Dispatch, GrantSession, DenySession
# Topics out:      Acknowledge | RequestSession (to depot) | Complete | Failed (to tms)
#
# Stateful Lex handlers — `tried` tracks fall-over attempts so the
# vehicle can re-route to a backup depot on the first denial.

fn config() -> AgentConfig {
  agent_new("vehicle")
  |> agent_peers(["depot", "depot2", "tms"])
  |> agent_effects(["a2a"])
  |> agent_specs(["vehicle.spec"])
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
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int },
       actions :: List[ActionRecord] } {
  if enough_soc(state.soc, state.energy_needed, state.reserve) {
    { state: { soc: state.soc, reserve: state.reserve,
               energy_needed: state.energy_needed, tried: 0 },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: msg.from, a2a_topic: "Acknowledge",
        payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
      }] }
  } else {
    { state: { soc: state.soc, reserve: state.reserve,
               energy_needed: state.energy_needed, tried: 1 },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: "depot", a2a_topic: "RequestSession",
        payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":50}", prompt: "",
      }] }
  }
}

fn on_grant(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "tms", a2a_topic: "Complete",
    payload_json: "{\"delivery_id\":\"d-1\"}", prompt: "",
  }]
}

fn on_deny(
  state :: { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> { state :: { soc :: Float, reserve :: Float, energy_needed :: Float, tried :: Int },
       actions :: List[ActionRecord] } {
  if state.tried >= 2 {
    { state: { soc: state.soc, reserve: state.reserve,
               energy_needed: state.energy_needed, tried: state.tried },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: "tms", a2a_topic: "Failed",
        payload_json: "{\"reason\":\"all_depots_denied\"}", prompt: "",
      }] }
  } else {
    { state: { soc: state.soc, reserve: state.reserve,
               energy_needed: state.energy_needed, tried: state.tried + 1 },
      actions: [{
        kind: "send_a2a", server: "", tool: "", args_json: "",
        peer: "depot2", a2a_topic: "RequestSession",
        payload_json: "{\"vehicle_id\":\"v-1\",\"power_kw\":50}", prompt: "",
      }] }
  }
}
