# tms-agent — runs in the cloud.
#
# Dispatches deliveries to vehicles and tracks their lifecycle. Only agent
# carrying mcp(tms_db) for delivery records and mcp(optimizer) for routing.
# Cannot grant charging sessions; cannot speak OCPP.

# requires: messages.lex

type Delivery = {
  delivery_id :: DeliveryId,
  status      :: DeliveryStatus,
  assigned_to :: Option[VehicleId],
  pickup      :: Location,
  dropoff     :: Location,
  deadline    :: Time,
}

type DeliveryStatus =
  | Pending
  | Dispatched
  | Acknowledged
  | Completed
  | Failed

type TmsState = {
  fleet      :: List[VehicleId],
  deliveries :: Map[DeliveryId, Delivery],
}

fn config() -> agent.Config {
  agent.new("tms")
    |> agent.with_state(initial_state())
    |> agent.peers([peer.named("vehicle")])
    |> agent.system_prompt(
        "You are the transport management system. Assign pending deliveries " ++
        "to suitable vehicles and track lifecycle to completion.")
    |> agent.tools([tools.optimizer, tools.tms_db])
    |> agent.effects([llm_cloud, mcp, a2a_in, a2a_out, time])
    |> agent.handle(Topic.Acknowledge, on_ack)
    |> agent.handle(Topic.Complete,    on_complete)
    |> agent.tick(60.seconds, on_tick)        # periodic dispatch loop
}

fn on_tick(s :: TmsState) -> [llm_cloud, mcp, emit] (TmsState, List[Action]) {
  # On each tick, the LLM looks at pending deliveries plus optimizer output
  # and proposes Dispatch actions toward suitable vehicles.
  let pending = s.deliveries |> map.values |> list.filter(is_pending)
  if list.is_empty(pending) {
    (s, [])
  } else {
    let proposal = llm.propose(s, "dispatch round", pending)
    (s, proposal.actions)
  }
}

fn on_ack(
  s :: TmsState,
  msg :: A2AMessage,
  from :: peer.Ref,
) -> Result[(TmsState, List[Action]), Error] {
  match a2a.parse_part(msg, "ack") :: Result[DispatchAck, _] {
    Err(e)  => Err(e),
    Ok(ack) => Ok((s |> mark_acknowledged(ack.delivery_id, peer.id(from)), [])),
  }
}

fn on_complete(
  s :: TmsState,
  msg :: A2AMessage,
  _from :: peer.Ref,
) -> Result[(TmsState, List[Action]), Error] {
  match a2a.parse_part(msg, "completion") :: Result[DeliveryComplete, _] {
    Err(e) => Err(e),
    Ok(c)  => Ok((s |> mark_completed(c.delivery_id, c.completed_at), [])),
  }
}

fn is_pending(d :: Delivery) -> Bool {
  match d.status { Pending => true, _ => false }
}
