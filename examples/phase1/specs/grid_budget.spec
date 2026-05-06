# spec: grid_budget
#
# Site total active charging power, including the proposed action's effect,
# must not exceed the depot's grid budget. Evaluated by the runtime gate
# against every action a depot-agent proposes, before the action executes.
#
# Applied agents: depot-agent (and any agent carrying mcp(ocpp) for this site)

spec grid_budget {
  forall s :: DepotState, a :: Action:
    let projected_load_kw := projected_site_load(s, a)
    projected_load_kw <= s.grid_budget_kw + s.pv_available_kw
}

# projected_site_load(s, a):
#   sum of active session power_kw +
#   sum of scheduled session power_kw whose window contains now() +
#   delta from a, where:
#     a == mcp(ocpp).RemoteStartTransaction(charger, ...) -> +charger.rated_kw
#     a == mcp(ocpp).RemoteStopTransaction(session, ...)  -> -session.power_kw
#     otherwise                                            -> 0
fn projected_site_load(s :: DepotState, a :: Action) -> Float { ... }
