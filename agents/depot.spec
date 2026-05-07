# depot.spec — grid-budget gate for the depot agent.
#
# Quantifies over the scalar floats in depot's state record:
#   - current_kw  : load already on the grid
#   - requested_kw: incoming session's requested power
#   - budget_kw   : depot's hard contract limit
#   - pv_kw       : current photovoltaic offset (added headroom)
#
# Invariant: any GrantSession must keep projected load (current +
# requested) at or below the available budget (grid + pv). The
# soft-runner builds bindings from state's top-level Float fields, so
# the quantifier names match state field names.
spec depot_grid_budget {
  forall current_kw   :: Float,
         requested_kw :: Float,
         budget_kw    :: Float,
         pv_kw        :: Float:
    current_kw + requested_kw <= budget_kw + pv_kw
}
