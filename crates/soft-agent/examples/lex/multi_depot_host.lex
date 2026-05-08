fn projected_load(current :: Float, delta :: Float) -> Float {
  current + delta
}

fn budget_total(grid :: Float, pv :: Float) -> Float {
  grid + pv
}

fn under_budget(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  projected_load(current, delta) <= budget_total(grid, pv)
}

fn soc_after(current :: Float, used :: Float) -> Float {
  current - used
}

fn above_reserve(current :: Float, used :: Float, reserve :: Float) -> Bool {
  soc_after(current, used) >= reserve
}
