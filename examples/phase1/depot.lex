# depot-agent — runs in the cloud (one per site).
#
# Owns the site's grid budget. Only agent in the system with mcp(ocpp) and
# the authority to emit Topic.GrantSession. The grid_budget spec is a
# runtime gate on every emitted mcp(ocpp) action — a RemoteStartTransaction
# whose projected site load would exceed the budget is denied before the
# OCPP call goes out.

# requires: messages.lex

type ChargerSlot = {
  charger_id :: ChargerId,
  start_time :: Time,
  end_time   :: Time,
  power_kw   :: Float,
}

type DepotState = {
  site_id        :: Text,
  grid_budget_kw :: Float,
  active         :: List[ChargerSlot],   # sessions in flight
  scheduled      :: List[ChargerSlot],   # granted, not yet started
  pv_available_kw :: Float,
}

fn config() -> agent.Config {
  agent.new("depot")
    |> agent.with_state(initial_state())
    |> agent.peers([peer.named("vehicle"), peer.named("tms"), peer.named("pv")])
    |> agent.system_prompt(
        "You manage a charging depot. Grant sessions when feasible; deny " ++
        "with a clear reason and retry-after when not. Stay under grid budget.")
    |> agent.tools([tools.optimizer, tools.ocpp])
    |> agent.specs([specs.grid_budget])
    |> agent.effects([llm_cloud, mcp, a2a_in, a2a_out])
    |> agent.handle(Topic.RequestSession, on_request)
    |> agent.handle(Topic.StatusReport,    on_status)
}

fn on_request(
  s :: DepotState,
  msg :: A2AMessage,
  from :: peer.Ref,
) -> [llm_cloud, mcp, emit] Result[(DepotState, List[Action]), Error] {
  match a2a.parse_part(msg, "request") :: Result[SessionRequest, _] {
    Err(e)  => Err(e),
    Ok(req) => {
      # The LLM proposes a plan informed by the optimizer. Typical proposals:
      #   - mcp.call("optimizer", "schedule", { req, site_state: s })
      #   - mcp.call("ocpp", "RemoteStartTransaction", { charger, vehicle })
      #   - a2a.send(from, Topic.GrantSession, SessionGrant({ ... }))
      # OR
      #   - a2a.send(from, Topic.DenySession, SessionDenial({ ... }))
      #
      # Each emitted action passes through specs.grid_budget before
      # execution. The OCPP RemoteStartTransaction action is the one the
      # spec actually constrains; the A2A reply is informational.
      let proposal = llm.propose(s, "session request", { req: req, site: s })
      Ok((s, proposal.actions))
    },
  }
}

fn on_status(
  s :: DepotState,
  msg :: A2AMessage,
  _from :: peer.Ref,
) -> Result[(DepotState, List[Action]), Error] {
  # Heartbeat only updates depot's view of the world; no actions.
  match a2a.parse_part(msg, "status") :: Result[VehicleStatus, _] {
    Err(e)      => Err(e),
    Ok(_status) => Ok((s, [])),
  }
}
