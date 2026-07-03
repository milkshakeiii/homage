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
    kill_and_respawn_as(port, HullKind::Harvester)
}

/// Client 2 dies and respawns as `hull` (bankrolled); returns the net.
fn kill_and_respawn_as(port: u16, hull: HullKind) -> TestNet {
    let mut net = TestNet::new(port, &[1, 2]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2));
    net.set_bank(2, 100);
    net.client_send_spawn_order(1, hull);
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

/// Shooter (index 0, client 1) kills `victim_id`, which respawns.
fn kill_client(net: &mut TestNet, victim_id: u64) {
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(victim_id, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(victim_id).is_none()),
        "client {victim_id} never died"
    );
    net.set_input(0, Some(ShipInput::default()));
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship(victim_id).is_some()
        ),
        "client {victim_id} never respawned"
    );
}

/// Self-destruct: the solo path to a new hull. Scuttling drops cargo like
/// any death and the normal respawn (with the standing spawn order) follows.
#[test]
fn self_destruct_swaps_hulls_solo() {
    let mut net = TestNet::new(6608, &[1]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()));
    net.teleport(1, Vec2::new(2500.0, -2500.0), 0.0);
    net.set_bank(1, 100);
    net.set_ship_cargo(1, u16::MAX);
    net.client_send_spawn_order(0, HullKind::Harvester);
    net.run_ticks(32);

    net.client_send_self_destruct(0);
    assert!(
        net.run_until(256, |net| net.server_ship(1).is_none()),
        "self-destruct never destroyed the ship"
    );
    assert!(
        net.server_fragment_count() >= sim::FIGHTER_CARGO_CAPACITY as usize,
        "scuttling should scatter the hold ({} fragments)",
        net.server_fragment_count()
    );
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship_hull(1) == Some(HullKind::Harvester)
        ),
        "never respawned as the ordered harvester (hull {:?})",
        net.server_ship_hull(1)
    );
}

/// Combat hulls require a live friendly strike carrier (DESIGN §2): a rich
/// order for a corvette with no carrier is denied without charging.
#[test]
fn corvette_is_denied_without_a_carrier() {
    let mut net = TestNet::new(6607, &[1, 2]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2));
    net.set_bank(2, 100);
    net.client_send_spawn_order(1, HullKind::Corvette);
    net.run_ticks(32);
    kill_client(&mut net, 2);
    assert_eq!(
        net.server_ship_hull(2),
        Some(HullKind::Fighter),
        "combat hull must be denied with no friendly carrier"
    );
    assert_eq!(net.server_bank(2), 100, "denied purchase must not charge");
}

/// With a friendly carrier alive: the corvette spawns beside it, and the
/// Gunship archetype fires along the mouse aim, not the hull facing — a
/// corvette pointing +X hits a target due +Y when the turret says so.
#[test]
fn corvette_spawns_at_carrier_and_turret_fires_along_aim() {
    // Teams for [1,2,3,4] alternate: 1/3 together, 2/4 together.
    let mut net = TestNet::new(6604, &[1, 2, 3, 4]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 4),
        "clients never connected"
    );
    let (t1, t2) = (
        net.server_ship_team(1).unwrap(),
        net.server_ship_team(2).unwrap(),
    );
    assert_eq!(net.server_ship_team(4), Some(t2), "4 should join team 2");
    assert_ne!(t1, t2);

    // Park bystanders far from the kill corridor along +X from the origin.
    net.teleport(2, Vec2::new(0.0, 3000.0), 0.0);
    net.teleport(3, Vec2::new(0.0, -3000.0), 0.0);

    // Client 4 becomes team 2's strike carrier, parked forward.
    net.set_bank(4, 100);
    net.client_send_spawn_order(3, HullKind::StrikeCarrier);
    net.run_ticks(32);
    kill_client(&mut net, 4);
    assert_eq!(net.server_ship_hull(4), Some(HullKind::StrikeCarrier));
    let carrier_pos = Vec2::new(2500.0, 500.0);
    net.teleport(4, carrier_pos, 0.0);
    net.run_ticks(16);

    // Client 2 buys a corvette; it must spawn beside the carrier.
    net.set_bank(2, 100);
    net.client_send_spawn_order(1, HullKind::Corvette);
    net.run_ticks(32);
    kill_client(&mut net, 2);
    assert_eq!(net.server_ship_hull(2), Some(HullKind::Corvette));
    let (spawn_pos, _) = net.server_ship(2).unwrap();
    assert!(
        spawn_pos.distance(carrier_pos) < 300.0,
        "corvette spawned {:.0} units from its carrier",
        spawn_pos.distance(carrier_pos)
    );

    // Corvette (client 2) faces +X; the enemy fighter (client 1) sits due
    // +Y of it. Only turret-aimed fire can connect.
    net.teleport(2, Vec2::new(2000.0, 1000.0), 0.0);
    net.teleport(1, Vec2::new(2000.0, 1300.0), 0.0);
    net.run_ticks(64);

    let mut input = ShipInput {
        fire: true,
        ..default()
    };
    input.set_aim_radians(core::f32::consts::FRAC_PI_2); // straight up
    net.set_input(1, Some(input));

    assert!(
        net.run_until(1024, |net| net
            .server_ship(1)
            .is_some_and(|(_, health)| health < sim::SHIP_HEALTH)),
        "turret-aimed fire never hit the +Y target (health {:?})",
        net.server_ship(1)
    );
}
