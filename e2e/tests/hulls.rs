//! M2: hull purchase on respawn — spawn orders, cost deduction, fallback,
//! and per-hull stats reaching the simulation.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{HullKind, ShipInput};
use homage_shared::{hulls, sim};

const CONNECT_TICKS: usize = 1024;

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

fn thrust() -> Option<ShipInput> {
    Some(ShipInput {
        thrust: true,
        ..default()
    })
}

#[test]
fn spawn_order_buys_a_harvester_on_respawn() {
    let mut net = TestNet::new(6601, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );

    let cost = hulls::stats(HullKind::Harvester).cost;
    net.set_bank(2, cost + 5);
    net.client_send_spawn_order(1, HullKind::Harvester);
    net.run_ticks(32); // let the order reach the server

    // Kill the victim; the respawn should be a paid-for harvester.
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(2).is_none()),
        "victim never died"
    );
    net.set_input(0, Some(ShipInput::default()));
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship_hull(2) == Some(HullKind::Harvester)
        ),
        "victim never respawned as a harvester (hull: {:?})",
        net.server_ship_hull(2)
    );
    assert_eq!(net.server_bank(2), 5, "harvester cost not deducted");
    let (_, capacity) = net.server_ship_cargo(2).unwrap();
    assert_eq!(
        capacity,
        hulls::stats(HullKind::Harvester).cargo_capacity,
        "harvester stats not applied"
    );
}

#[test]
fn broke_players_fall_back_to_the_free_fighter() {
    let mut net = TestNet::new(6602, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );
    net.set_bank(2, 3); // less than the harvester's cost
    net.client_send_spawn_order(1, HullKind::Harvester);
    net.run_ticks(32);

    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(2).is_none()),
        "victim never died"
    );
    net.set_input(0, Some(ShipInput::default()));
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship(2).is_some()
        ),
        "victim never respawned"
    );
    assert_eq!(
        net.server_ship_hull(2),
        Some(HullKind::Fighter),
        "broke player should get the free fighter"
    );
    assert_eq!(net.server_bank(2), 3, "no money should move on a fallback");
}

/// Hull stats reach the live simulation: a harvester tops out well below a
/// fighter (handling is hull identity, DESIGN §4.2 guidepost 8).
#[test]
fn harvester_handles_like_a_harvester() {
    let mut net = kill_and_respawn_with_harvester(6603);
    net.teleport(2, Vec2::new(0.0, -3000.0), 0.0);
    net.run_ticks(8);
    net.set_input(1, thrust());
    net.run_ticks(192);
    let speed = net.server_ship_velocity(2).unwrap().length();
    let harvester_max = hulls::stats(HullKind::Harvester).max_speed;
    assert!(
        (speed - harvester_max).abs() < 20.0,
        "harvester should top out near {harvester_max}, got {speed:.0}"
    );
}

fn kill_and_respawn_with_harvester(port: u16) -> TestNet {
    let mut net = kill_and_respawn_prepared(port);
    assert_eq!(net.server_ship_hull(2), Some(HullKind::Harvester));
    net
}

fn kill_and_respawn_prepared(port: u16) -> TestNet {
    let mut net = TestNet::new(port, &[1, 2]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2));
    net.set_bank(2, 100);
    net.client_send_spawn_order(1, HullKind::Harvester);
    net.run_ticks(32);
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(net.run_until(2048, |net| net.server_ship(2).is_none()));
    net.set_input(0, Some(ShipInput::default()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(2).is_some()
    ));
    net
}
