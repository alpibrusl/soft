# pv.lex — solar-availability broadcaster. Self-initiated via the
# tick mechanism: every tick fires a synthetic Tick message, the
# handler responds with a PvUpdate to depot.
#
# Run with `--tick Tick=2s` (or whatever cadence) to enable.
#
# State:  { pv_kw :: Float }
# In:     Tick (synthetic, from runner)
# Out:    PvUpdate (to depot)

fn config() -> AgentConfig {
  agent_new("pv")
  |> agent_peers(["depot"])
  |> agent_effects(["a2a"])
  |> agent_handles([
       { topic: "Tick", fn_name: "on_tick" },
     ])
}

fn on_tick(
  state :: { pv_kw :: Float },
  msg   :: { from :: Str, topic :: Str, payload_json :: Str },
) -> List[ActionRecord] {
  [{
    kind: "send_a2a", server: "", tool: "", args_json: "",
    peer: "depot", a2a_topic: "PvUpdate",
    payload_json: "{}", prompt: "",
  }]
}
