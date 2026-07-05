//! M2: Captain archetype hulls — omnidirectional drift and the resource
//! controller as a mobile dropoff.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{HullKind, ShipInput};
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

/// Kill client 2 (via enemy client 1) after ordering `hull`; returns after
/// the respawn.
fn respawn_client2_as(port: u16, hull: HullKind) -> TestNet {
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
    assert_eq!(net.server_ship_hull(2), Some(hull));
    net
}

/// Captain hulls: WASD nudges in screen space, no facing. D pushes +X,
/// W pushes +Y, regardless of rotation.
#[test]
fn captain_hulls_drift_omnidirectionally() {
    let mut net = respawn_client2_as(6605, HullKind::ResourceController);
    net.teleport(2, Vec2::new(2000.0, -2000.0), 0.0);
    net.run_ticks(8);

    // D → +X.
    net.set_input(
        1,
        Some(ShipInput {
            turn_right: true,
            ..default()
        }),
    );
    net.run_ticks(64);
    let vel = net.server_ship_velocity(2).unwrap();
    assert!(
        vel.x > 40.0 && vel.y.abs() < 10.0,
        "D should push a Captain hull +X; got {vel:?}"
    );

    // W → +Y (screen up), even though the hull has no facing to turn.
    net.set_input(
        1,
        Some(ShipInput {
            thrust: true,
            ..default()
        }),
    );
    net.run_ticks(96);
    let vel = net.server_ship_velocity(2).unwrap();
    assert!(
        vel.y > 40.0,
        "W should push a Captain hull +Y; got {vel:?}"
    );
}

/// A friendly resource controller accepts deposits like a rolling mothership.
#[test]
fn resource_controller_is_a_mobile_dropoff() {
    // Three clients: 1 and 3 land on one team, 2 on the other.
    let mut net = TestNet::new(6606, &[1, 2, 3]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 3),
        "clients never connected"
    );
    let team1 = net.server_ship_team(1).unwrap();
    let team3 = net.server_ship_team(3).unwrap();
    assert_eq!(team1, team3, "clients 1 and 3 should share a team");

    // Client 3 becomes a resource controller (killed by the enemy, client 2).
    net.set_bank(3, 100);
    net.client_send_spawn_order(2, HullKind::ResourceController);
    net.run_ticks(32);
    net.teleport(2, Vec2::ZERO, 0.0);
    net.teleport(3, Vec2::new(300.0, 0.0), 0.0);
    net.teleport(1, Vec2::new(-3000.0, 3000.0), 0.0); // out of the way
    net.run_ticks(64);
    net.set_input(1, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(3).is_none()),
        "client 3 never died"
    );
    net.set_input(1, Some(ShipInput::default()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship_hull(3) == Some(HullKind::ResourceController)
    ));

    // Park the controller in open space; bring in a loaded friendly hauler.
    net.teleport(3, Vec2::new(1500.0, -2500.0), 0.0);
    net.teleport(1, Vec2::new(1560.0, -2500.0), 0.0);
    net.set_ship_cargo(1, u16::MAX);
    let before = net.server_bank(1);
    assert!(
        net.run_until(512, |net| {
            net.server_ship_cargo(1).is_some_and(|(current, _)| current == 0)
        }),
        "hauler never finished depositing at the controller; cargo {:?}",
        net.server_ship_cargo(1)
    );
    assert_eq!(
        net.server_bank(1) - before,
        sim::FIGHTER_CARGO_CAPACITY as u32,
        "deposit at a friendly controller should bank the full hold"
    );
}

/// The fleet carrier is infrastructure (DESIGN §6): it hosts combat spawns,
/// stocks carrier-tier fittings (but NOT the strike carrier's exclusives),
/// and accepts deposits like a rolling mothership.
#[test]
fn fleet_carrier_hosts_spawns_stocks_and_takes_deposits() {
    use homage_shared::protocol::{CheatOrder, FittingId, Loadout};

    let mut net = TestNet::new(6607, &[1, 2, 3, 4]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 4));
    net.teleport(2, Vec2::new(0.0, 3000.0), 0.0);
    net.teleport(3, Vec2::new(0.0, -3000.0), 0.0);

    // Client 4 becomes its team's FLEET carrier.
    net.set_bank(4, 200);
    net.client_send_spawn_order(3, HullKind::FleetCarrier);
    net.run_ticks(32);
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(4, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(net.run_until(2048, |net| net.server_ship(4).is_none()));
    net.set_input(0, Some(ShipInput::default()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship_hull(4) == Some(HullKind::FleetCarrier)
    ));
    let carrier_pos = Vec2::new(2000.0, -2000.0);
    net.teleport(4, carrier_pos, 0.0);
    net.run_ticks(64);

    // Client 2 spawns a fighter there asking for a strike-exclusive weapon
    // (must fall back) and a carrier-tier hull mod (must stick).
    net.client_send_cheat(1, CheatOrder::GivePoints(100));
    assert!(net.run_until(256, |net| net.server_points(2) == 100));
    net.client_send_unlock(1, FittingId::LongLance);
    net.client_send_unlock(1, FittingId::CompactedHold);
    net.run_ticks(64);
    let carrier_entity = net.client_find_ship(1, 4).expect("sees fleet carrier");
    net.client_send_spawn_order_loadout(
        1,
        HullKind::Fighter,
        Some(carrier_entity),
        Loadout {
            weapon: FittingId::LongLance,
            utility: None,
            hull_mod: Some(FittingId::CompactedHold),
        },
    );
    net.run_ticks(32);
    net.teleport(2, Vec2::new(-3000.0, 3000.0), 0.0);
    net.run_ticks(8);
    net.client_send_self_destruct(1);
    assert!(net.run_until(256, |net| net.server_ship(2).is_none()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(2).is_some()
    ));

    let (spawn_pos, _) = net.server_ship(2).unwrap();
    assert!(
        spawn_pos.distance(carrier_pos) < 350.0,
        "fighter should spawn beside the fleet carrier ({:.0} away)",
        spawn_pos.distance(carrier_pos)
    );
    let equipped = net.server_ship_equipped(2).unwrap();
    assert_eq!(
        equipped.weapon,
        homage_shared::protocol::FittingId::PulseCannon,
        "strike-exclusive weapon must fall back at a fleet carrier"
    );
    assert_eq!(equipped.hull_mod, Some(FittingId::CompactedHold));
    let (_, capacity) = net.server_ship_cargo(2).unwrap();
    assert_eq!(capacity, 7, "compacted hold: 5 * 1.5");

    // And it takes deposits.
    net.set_ship_cargo(2, u16::MAX);
    let before = net.server_bank(2);
    assert!(
        net.run_until(512, |net| net
            .server_ship_cargo(2)
            .is_some_and(|(current, _)| current == 0)),
        "deposit at the fleet carrier never completed"
    );
    assert_eq!(net.server_bank(2) - before, 7);
}
