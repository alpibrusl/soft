# depot.spec — grid-budget gate for the depot agent.
#
# Quantifies over a mix of state and action data:
#   - current_kw  : load already on the grid              (from state)
#   - budget_kw   : depot's hard contract limit           (from state)
#   - pv_kw       : current photovoltaic offset           (from state)
#   - power_kw    : power the outgoing GrantSession is committing to
#                                                          (from the
#                                                           SendA2a
#                                                           payload)
#
# This is a stronger invariant than reading `requested_kw` from state:
# it audits the *commitment depot is making*, not the request as
# recorded internally. If depot's handler ever produces a GrantSession
# whose `power_kw` exceeds available headroom — even by a bug in the
# handler — the gate denies the action.
#
# soft-runner's `default_float_bindings` extracts `power_kw` from the
# SendA2a action's payload and merges it with state floats; that's
# what makes this spec body resolvable at gate time.
spec depot_grid_budget {
  forall current_kw :: Float,
         power_kw   :: Float,
         budget_kw  :: Float,
         pv_kw      :: Float:
    current_kw + power_kw <= budget_kw + pv_kw
}
