//! Cross-depot fall-over: vehicle requests at depot1, depot1 denies
//! (always over-budget in this scenario), vehicle's `on_deny` redirects
//! the request to depot2, which grants. Exercises the stateful-Lex-
//! handler return so the vehicle remembers it has already tried depot1.

use serde_json::json;
use soft_agent::{A2aMessage, InProcessRouter, Mailbox, Runner};

const VEHICLE_LEX: &str = include_str!("fixtures/cross_depot_vehicle.lex");

const DEPOT_DENIES_LEX: &str = include_str!("fixtures/cross_depot_depot_denies.lex");

const DEPOT_GRANTS_LEX: &str = include_str!("fixtures/cross_depot_depot_grants.lex");

const TMS_LEX: &str = include_str!("fixtures/cross_depot_tms.lex");

#[test]
fn vehicle_falls_over_from_depot1_to_depot2() {
    let router = InProcessRouter::new();

    let (v_box, v_send) = Mailbox::new();
    router.register("vehicle", v_send.clone());
    let mut vehicle = Runner::from_lex_source(VEHICLE_LEX)
        .unwrap()
        .mailbox(v_box)
        .state(json!({"tried": 0}))
        .executor(Box::new(router.executor("vehicle")))
        .build()
        .unwrap();

    let (d1_box, d1_send) = Mailbox::new();
    router.register("depot1", d1_send);
    let mut depot1 = Runner::from_lex_source(DEPOT_DENIES_LEX)
        .unwrap()
        .mailbox(d1_box)
        .state(json!({"ok": false}))
        .executor(Box::new(router.executor("depot1")))
        .build()
        .unwrap();

    let (d2_box, d2_send) = Mailbox::new();
    router.register("depot2", d2_send);
    let mut depot2 = Runner::from_lex_source(DEPOT_GRANTS_LEX)
        .unwrap()
        .mailbox(d2_box)
        .state(json!({"ok": true}))
        .executor(Box::new(router.executor("depot2")))
        .build()
        .unwrap();

    let (t_box, t_send) = Mailbox::new();
    router.register("tms", t_send.clone());
    let mut tms = Runner::from_lex_source(TMS_LEX)
        .unwrap()
        .mailbox(t_box)
        .state(json!({"running": true}))
        .executor(Box::new(router.executor("tms")))
        .build()
        .unwrap();

    // Seed: tms dispatches to vehicle.
    v_send
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    // Drain everyone in rounds until quiescent.
    let mut total_messages = 0usize;
    for _ in 0..20 {
        let v = vehicle.drain().unwrap();
        let d1 = depot1.drain().unwrap();
        let d2 = depot2.drain().unwrap();
        let t = tms.drain().unwrap();
        let round_msgs = v.messages + d1.messages + d2.messages + t.messages;
        total_messages += round_msgs;
        if round_msgs == 0 {
            break;
        }
    }

    // Expected message flow:
    //   1. vehicle ← Dispatch (from tms)            (vehicle: 1)
    //   2. depot1  ← RequestSession (from vehicle)  (depot1: 1)
    //   3. vehicle ← DenySession (from depot1)      (vehicle: 2)
    //   4. depot2  ← RequestSession (from vehicle)  (depot2: 1)
    //   5. vehicle ← GrantSession (from depot2)     (vehicle: 3)
    //   6. tms     ← Complete (from vehicle)        (tms: 1)
    // = 6 messages total.
    assert_eq!(total_messages, 6, "expected 6 messages in fall-over flow");

    // The vehicle's state advanced past `tried = 1` after depot1 denied,
    // proving the stateful Lex handler ran twice (Dispatch + Deny).
    assert_eq!(
        vehicle.state()["tried"],
        2,
        "vehicle should have escalated to depot2"
    );
}
