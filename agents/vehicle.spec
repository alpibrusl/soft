# vehicle.spec — state-of-charge reserve gate for the vehicle agent.
#
# Quantifies over a record-typed `s` binding projected from the
# vehicle's state. The action shape isn't relevant for this
# invariant — vehicle's outgoing actions don't carry SOC data — so
# the gate doesn't need a record-typed `a` binding here.
#
# Lex-lang 0.3 (#208) shipped record-typed quantifiers and field
# access; soft-agent's `bindings::record_bindings` forwards the
# state JSON straight through as a `LexValue::Record`.
spec vehicle_soc_reserve {
  forall s :: { soc :: Float, reserve :: Float, energy_needed :: Float }:
    s.soc - s.energy_needed >= s.reserve
}
