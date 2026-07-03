//! End-to-end smoke tests for the netcode foundation: connection,
//! replication, prediction, and lag-compensated combat. These are the
//! template for gameplay-feature tests (economy, building, ...).

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::ShipInput;
use homage_shared::sim;

/// Generous connection allowance: netcode handshake + lightyear sync.
const CONNECT_TICKS: usize = 1024;

fn thrust() -> Option<ShipInput> {
    Some(ShipInput {
        thrust: true,
        ..default()
    })
}

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

#[test]
fn clients_connect_and_replicate() {
    let mut net = TestNet::new(6101, &[1, 2]);

    // Both ships spawn on the server.
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "server never spawned both ships"
    );

    // Each client ends up with a predicted copy of its own ship and an
    // interpolated copy of the other.
    assert!(
        net.run_until(CONNECT_TICKS, |net| {
            (0..2).all(|i| net.client_ship_kinds(i) == (1, 1))
        }),
        "clients never saw 1 predicted + 1 interpolated ship; client 0 sees {:?}, client 1 sees {:?}",
        net.client_ship_kinds(0),
        net.client_ship_kinds(1),
    );
}

#[test]
fn thrust_moves_ship_and_prediction_agrees() {
    let mut net = TestNet::new(6102, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| {
            net.server_ships().len() == 1 && net.predicted_ship_pos(0).is_some()
        }),
        "client never connected"
    );

    let (start, _) = net.server_ship(1).unwrap();
    net.set_input(0, thrust());
    net.run_ticks(256); // 4 seconds of thrust

    let (end, _) = net.server_ship(1).unwrap();
    let moved = (end - start).length();
    assert!(
        moved > 200.0,
        "ship barely moved under thrust: {moved} units"
    );

    // The client's predicted position should roughly agree with the server's
    // (the client runs a few ticks ahead, so allow that much drift).
    let predicted = net.predicted_ship_pos(0).unwrap();
    let divergence = (predicted - end).length();
    assert!(
        divergence < 150.0,
        "prediction diverged from server: {divergence} units (server {end:?}, predicted {predicted:?})"
    );
}

#[test]
fn bullets_kill_and_respawn() {
    let mut net = TestNet::new(6103, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );

    // Line them up: shooter at the origin facing +X, target dead ahead.
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    // Let pose history and replication settle at the new positions.
    net.run_ticks(64);

    net.set_input(0, fire());
    assert!(
        net.run_until(1024, |net| {
            net.server_ship(2).is_none_or(|(_, health)| health < sim::SHIP_HEALTH)
        }),
        "no bullet ever hit the target"
    );

    // Keep firing until the target dies and despawns.
    assert!(
        net.run_until(2048, |net| net.server_ship(2).is_none()),
        "target never died"
    );
    net.set_input(0, None);

    // And it should come back at full health after the respawn delay.
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship(2).is_some_and(|(_, health)| health == sim::SHIP_HEALTH)
        ),
        "target never respawned"
    );
}
