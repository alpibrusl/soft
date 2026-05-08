# depot.spec — grid-budget gate for the depot agent.
#
# Quantifies over two record-typed bindings:
#   - `s` :: { current_kw, budget_kw, pv_kw } — projected from the
#     depot's state record at gate time.
#   - `a` :: { power_kw } — projected from the *outgoing*
#     SendA2a.payload (e.g. the GrantSession the depot is committing
#     to). The lex handler embeds power_kw in every GrantSession so
#     the spec can audit the commitment, not the internal request.
#
# Lex-lang 0.3 (#208) shipped record-typed quantifiers and field
# access, so the spec body destructures the records directly — no
# flattening helper required (cf. soft-agent's `bindings::record_bindings`,
# which simply forwards state and action.payload as `Record` values).
#
# This is the same invariant as 0.2's flat-binding form
# (current_kw + power_kw ≤ budget_kw + pv_kw); the only change is
# binding shape.
spec depot_grid_budget {
  forall s :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float },
         a :: { power_kw :: Float }:
    s.current_kw + a.power_kw <= s.budget_kw + s.pv_kw
}
