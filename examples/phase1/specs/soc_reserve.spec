# spec: soc_reserve
#
# A vehicle agent must never propose an action whose foreseeable effect
# would drop SoC below the configured reserve. The relevant action class
# in Phase 1 is Acknowledging a Dispatch — accepting a delivery whose
# energy budget cannot be met without breaching reserve.
#
# Applied agents: vehicle-agent

spec soc_reserve {
  forall v :: VehicleState, a :: Action:
    let projected_soc := project_soc(v, a)
    projected_soc >= v.reserve_pct
}

# project_soc(v, a):
#   v.soc - estimated_energy_consumed(v, a) / battery_capacity_kwh
#
# where estimated_energy_consumed depends on the action kind:
#   a == a2a.send(_, Topic.Acknowledge, ack)
#     -> energy required to complete the corresponding DispatchOrder from
#        v.location, accounting for any planned charging stops in the route
#   a == action.proceed_to(charger_id, _)
#     -> energy to reach charger_id from v.location
#   otherwise
#     -> 0
fn project_soc(v :: VehicleState, a :: Action) -> Float { ... }
