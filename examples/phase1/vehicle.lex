# vehicle-agent — runs on truck edge hardware (Jetson Orin / Mac Studio mini).
#
# Effect signature is the load-bearing line. By construction this agent
# cannot:
#   - reach a cloud LLM (no llm_cloud)
#   - call OCPP or any other cloud MCP server (no mcp)
#   - grant a charging session (no Topic.GrantSession in handle())
#   - dispatch deliveries (no Topic.Dispatch in handle())
#
# Read the effect set as the agent's authority.

# requires: messages.lex

type VehicleState = {
  vehicle_id       :: VehicleId,
  location         :: Location,
  soc              :: Float,        # 0.0 .. 1.0
  reserve_pct      :: Float,        # 0.0 .. 1.0; spec floor
  active_session   :: Option[SessionId],
  pending_delivery :: Option[DispatchOrder],
}

fn config() -> agent.Config {
  agent.new("vehicle")
    |> agent.with_state(initial_state())
    |> agent.peers([peer.named("depot"), peer.named("tms")])
    |> agent.system_prompt(
        "You are an autonomous truck. Accept feasible deliveries, request " ++
        "charging when SoC nears reserve, report status on heartbeat.")
    |> agent.tools([tools.telemetry_read, tools.local_planner])
    |> agent.specs([specs.soc_reserve])
    |> agent.effects([llm_local, telemetry_read, a2a_in, a2a_out])
    |> agent.handle(Topic.Dispatch,     on_dispatch)
    |> agent.handle(Topic.GrantSession, on_grant)
    |> agent.handle(Topic.DenySession,  on_deny)
}

fn on_dispatch(
  s :: VehicleState,
  msg :: A2AMessage,
  from :: peer.Ref,
) -> [llm_local, telemetry_read, emit] Result[(VehicleState, List[Action]), Error] {
  match a2a.parse_part(msg, "delivery") :: Result[DispatchOrder, _] {
    Err(e)    => Err(e),
    Ok(order) => {
      # The LLM decides whether to accept and whether to ask for charging.
      # The runtime gate (specs.soc_reserve) checks each emitted action
      # before it executes; an Acknowledge whose projected SoC drops below
      # reserve will be denied and the LLM is asked to re-propose.
      let proposal = llm.propose(s, "incoming dispatch", order)
      Ok((s |> with_pending(order), proposal.actions))
    },
  }
}

fn on_grant(
  s :: VehicleState,
  msg :: A2AMessage,
  _from :: peer.Ref,
) -> [emit] Result[(VehicleState, List[Action]), Error] {
  match a2a.parse_part(msg, "grant") :: Result[SessionGrant, _] {
    Err(e) => Err(e),
    Ok(g)  => Ok((s |> with_session(g.session_id),
                  [action.proceed_to(g.charger_id, g.start_time)])),
  }
}

fn on_deny(
  s :: VehicleState,
  msg :: A2AMessage,
  _from :: peer.Ref,
) -> [llm_local, emit] Result[(VehicleState, List[Action]), Error] {
  match a2a.parse_part(msg, "denial") :: Result[SessionDenial, _] {
    Err(e) => Err(e),
    Ok(d)  => {
      # Let the LLM decide: retry later, try a different depot, or escalate.
      let proposal = llm.propose(s, "session denied", d)
      Ok((s, proposal.actions))
    },
  }
}
