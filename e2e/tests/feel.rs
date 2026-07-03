//! Feel-bar regression tests (DESIGN §4.2). These pin the movement and input
//! guideposts to tick counts so a physics or netcode change that degrades
//! handling fails CI instead of shipping mush.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::ShipInput;
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

fn thrust() -> Option<ShipInput> {
    Some(ShipInput {
        thrust: true,
        ..default()
    })
}

fn brake() -> Option<ShipInput> {
    Some(ShipInput {
        brake: true,
        ..default()
    })
}

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

fn connect_one(port: u16) -> TestNet {
    let mut net = TestNet::new(port, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()),
        "client never connected"
    );
    net
}

/// Ticks of sustained thrust until the ship reaches 95% of max speed.
fn ticks_to_near_max(net: &mut TestNet) -> usize {
    net.set_input(0, thrust());
    let mut ticks = 0;
    let target = 0.95 * sim::MAX_SPEED;
    while net.server_ship_velocity(1).unwrap().length() < target {
        net.tick();
        ticks += 1;
        assert!(ticks < 512, "never reached 95% of max speed");
    }
    ticks
}

/// Feel bar: roughly 1.5s from rest to max speed — fast enough to feel
/// responsive, slow enough that speed is earned.
#[test]
fn acceleration_hits_the_feel_bar() {
    let mut net = connect_one(6201);
    let ticks = ticks_to_near_max(&mut net);
    let seconds = ticks as f32 * sim::TICK_DT;
    assert!(
        (1.0..=2.2).contains(&seconds),
        "0 to 95% max speed took {seconds:.2}s ({ticks} ticks); feel bar is ~1.5s"
    );
}

/// Feel bar: reversing course (flip and thrust) must be meaningfully faster
/// than accelerating from rest — mistakes are correctable.
#[test]
fn reversal_is_faster_than_acceleration() {
    let mut net = connect_one(6202);
    net.teleport(1, Vec2::ZERO, 0.0);
    let accel_ticks = ticks_to_near_max(&mut net);

    // Flip the ship 180° while it barrels along +X, and thrust.
    net.set_rotation(1, core::f32::consts::PI);
    net.set_input(0, thrust());
    let mut reversal_ticks = 0;
    while net.server_ship_velocity(1).unwrap().x > 0.0 {
        net.tick();
        reversal_ticks += 1;
        assert!(reversal_ticks < 512, "ship never reversed");
    }

    assert!(
        (reversal_ticks as f32) < 0.8 * accel_ticks as f32,
        "reversal ({reversal_ticks} ticks) not meaningfully faster than \
         acceleration from rest ({accel_ticks} ticks)"
    );
}

/// Brake (S) bleeds speed much faster than coasting drag alone.
#[test]
fn brake_stops_faster_than_coasting() {
    let mut net = connect_one(6203);

    // Reach speed, then coast: measure what drag alone does in one second.
    ticks_to_near_max(&mut net);
    net.set_input(0, Some(ShipInput::default()));
    let start = net.server_ship_velocity(1).unwrap().length();
    net.run_ticks(64);
    let after_coast = net.server_ship_velocity(1).unwrap().length();

    // Back to speed, then brake for one second.
    ticks_to_near_max(&mut net);
    let brake_start = net.server_ship_velocity(1).unwrap().length();
    net.set_input(0, brake());
    net.run_ticks(64);
    let after_brake = net.server_ship_velocity(1).unwrap().length();

    let coast_loss = start - after_coast;
    let brake_loss = brake_start - after_brake;
    assert!(
        brake_loss > 2.0 * coast_loss,
        "brake loss {brake_loss:.0} u/s not >> coast loss {coast_loss:.0} u/s"
    );
    assert!(
        after_brake < 0.1 * brake_start,
        "one second of brake should nearly stop the ship (still at {after_brake:.0} u/s)"
    );
}

/// The client's own ship must advance every tick under thrust. If prediction
/// degrades to snapping on server updates (e.g. the predicted entity is
/// missing its physics components, so avian never integrates it locally),
/// the position freezes between packets — 20Hz stutter and input response
/// delayed by a round trip. Regression test for exactly that failure.
#[test]
fn predicted_ship_moves_every_tick_under_thrust() {
    let mut net = connect_one(6205);
    net.set_input(0, thrust());
    net.run_ticks(64); // get up to speed and past any spawn settling

    let mut last = net.predicted_ship_pos(0).expect("predicted ship exists");
    let mut static_ticks = 0;
    for _ in 0..64 {
        net.tick();
        let pos = net.predicted_ship_pos(0).expect("predicted ship exists");
        if pos.distance(last) < 0.01 {
            static_ticks += 1;
        }
        last = pos;
    }
    assert!(
        static_ticks <= 6,
        "predicted ship froze on {static_ticks}/64 ticks while thrusting — \
         client-side prediction is not integrating"
    );
}

/// Feel guidepost 5: a fire tap during cooldown is buffered and fires on the
/// first legal tick instead of being eaten.
#[test]
fn fire_tap_during_cooldown_is_buffered_not_eaten() {
    let mut net = connect_one(6204);
    net.teleport(1, Vec2::new(2000.0, 2000.0), 0.0);
    // Let the input timeline finish syncing (early inputs can arrive after
    // the server has already simulated their tick, and blips arriving late
    // are dropped — sustained inputs shrug that off, taps don't).
    net.run_ticks(64);

    // First shot: a short tap (a few ticks — sync can make a single update
    // run zero fixed ticks on the client, so one-tick taps are unreliable in
    // the harness; a real tap spans several 64Hz ticks anyway).
    net.set_input(0, fire());
    net.run_ticks(4);
    net.set_input(0, Some(ShipInput::default()));
    assert!(
        net.run_until(64, |net| net.server_bullet_count() == 1),
        "first tap never produced a bullet"
    );

    // Tap again while the cooldown (17 ticks) is still running, close enough
    // to its end that the 8-tick buffer covers the remainder for any
    // plausible client-ahead offset. Then keep the trigger released.
    net.run_ticks(8);
    net.set_input(0, fire());
    net.run_ticks(3);
    net.set_input(0, Some(ShipInput::default()));

    // Without buffering this tap lands mid-cooldown and vanishes; with
    // buffering a second bullet appears when the cooldown expires.
    assert!(
        net.run_until(64, |net| net.server_bullet_count() >= 2),
        "buffered fire tap was eaten by the cooldown"
    );
}
