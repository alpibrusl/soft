# A2A topics and Part shapes shared across the Phase 1 agents.
#
# Topics name the kind of intent. Each topic has one Part schema, sent as a
# structured Part in the A2A Message. Free-form text Parts are also allowed
# alongside structured Parts (the LLM consumes both), but the structured
# Part is what handlers and specs reason about.

type VehicleId  = Text
type SessionId  = Text
type ChargerId  = Text
type DeliveryId = Text

type Location = { lat :: Float, lon :: Float }

type Topic =
  | RequestSession
  | GrantSession
  | DenySession
  | Dispatch
  | Acknowledge
  | Complete
  | StatusReport

# RequestSession :: vehicle -> depot
type SessionRequest = {
  vehicle_id   :: VehicleId,
  arrival_time :: Time,
  energy_kwh   :: Float,
  deadline     :: Time,
}

# GrantSession :: depot -> vehicle
type SessionGrant = {
  session_id :: SessionId,
  charger_id :: ChargerId,
  start_time :: Time,
  end_time   :: Time,
  power_kw   :: Float,
}

# DenySession :: depot -> vehicle
type SessionDenial = {
  reason      :: Text,
  retry_after :: Option[Time],
}

# Dispatch :: tms -> vehicle
type DispatchOrder = {
  delivery_id :: DeliveryId,
  pickup      :: Location,
  dropoff     :: Location,
  deadline    :: Time,
}

# Acknowledge :: vehicle -> tms
type DispatchAck = {
  delivery_id :: DeliveryId,
  eta         :: Time,
}

# Complete :: vehicle -> tms
type DeliveryComplete = {
  delivery_id  :: DeliveryId,
  completed_at :: Time,
}

# StatusReport :: vehicle -> {depot, tms}
type VehicleStatus = {
  vehicle_id :: VehicleId,
  location   :: Location,
  soc        :: Float,
  in_session :: Option[SessionId],
}
