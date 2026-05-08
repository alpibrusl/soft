spec grid_budget {
  forall current_kw :: Float, delta_kw :: Float, grid_kw :: Float, pv_kw :: Float:
    under_budget(current_kw, delta_kw, grid_kw, pv_kw)
}
