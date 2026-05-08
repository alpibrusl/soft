spec soc_reserve {
  forall current_soc :: Float, energy_used :: Float, reserve :: Float:
    above_reserve(current_soc, energy_used, reserve)
}
