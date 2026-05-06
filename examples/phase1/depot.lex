# depot-agent — runs in the cloud (one per site).
#
# Compile-time effect signature [llm_cloud, mcp, a2a] enforces:
#   - cannot reach a local LLM (no llm_local)
#
# Runtime guarantees enforced by soft-agent's tool registry:
#   - MCP allowlist is { ocpp, optimizer } only
#   - depot is the only agent in the system whose allowlist contains "ocpp"
#     and the only agent that registers Topic.GrantSession as outgoing,
#     so RemoteStartTransaction calls and grant-session messages can only
#     originate from depot-agent.
#
# The grid_budget spec is a runtime gate on every proposed mcp action — a
# RemoteStartTransaction whose projected site load would exceed the budget
# returns Deny (or Inconclusive, treated as Deny in soft-agent) before the
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
    |> agent.mcp_servers(["optimizer", "ocpp"])
    |> agent.specs([specs.grid_budget])
    |> agent.effects([llm_cloud, mcp, a2a])
    |> agent.handle(Topic.RequestSession, on_request)
    |> agent.handle(Topic.StatusReport,   on_status)
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
      #   - agent.call_mcp("optimizer", "schedule_session", { req, site_state: s })
      #   - agent.call_mcp("ocpp",      "remote_start_transaction", { ... })
      #   - agent.send_a2a(from, "GrantSession", SessionGrant({ ... }))
      # OR
      #   - agent.send_a2a(from, "DenySession",  SessionDenial({ ... }))
      #
      # Each emitted action passes through specs.grid_budget before
      # execution. The OCPP remote_start_transaction action is the one the
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
