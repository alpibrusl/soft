# vehicle.spec — state-of-charge reserve gate for the vehicle agent.
#
# Quantifies over scalar floats in the vehicle's state:
#   - soc           : current state of charge (0.0..1.0)
#   - energy_needed : energy this delivery will consume
#   - reserve       : floor SOC must stay above
#
# Invariant: any outbound action must preserve the post-delivery
# reserve (soc - energy_needed >= reserve). Phase 1 spec from
# tests/phase1_specs.rs, lifted into the deploy fleet.
spec vehicle_soc_reserve {
  forall soc           :: Float,
         energy_needed :: Float,
         reserve       :: Float:
    soc - energy_needed >= reserve
}
