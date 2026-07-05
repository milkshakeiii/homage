//! M4: the outfitter sub-carrier and the docking/refit model (DESIGN §6).

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{CheatOrder, FittingId, HullKind, Loadout, ShipInput};
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

/// Kill `victim_id` via the shooter at index 0, then wait for the respawn.
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

/// The whole sub-carrier story in one flow: outfitters only build at fleet
/// carriers; docking at one is the only path to its exclusive modules; the
/// undock is instant, free, and preserves the ship.
#[test]
fn outfitter_chain_and_docked_refit() {
    // Teams alternate: 1/3/5 vs 2/4/6.
    let mut net = TestNet::new(7201, &[1, 2, 3, 4, 5, 6]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 6),
        "clients never connected"
    );
    for bystander in [2, 3, 5, 6] {
        net.teleport(bystander, Vec2::new(-4000.0, 3000.0 + bystander as f32 * 200.0), 0.0);
    }

    // An outfitter ordered with no fleet carrier alive is denied (no charge).
    net.set_bank(2, 200);
    net.client_send_spawn_order(1, HullKind::Outfitter);
    net.run_ticks(32);
    kill_client(&mut net, 2);
    assert_eq!(
        net.server_ship_hull(2),
        Some(HullKind::Fighter),
        "outfitter must be denied without a fleet carrier"
    );
    assert_eq!(net.server_bank(2), 200, "denied build must not charge");

    // Client 4 builds the fleet carrier; client 2 then builds the outfitter
    // AT it (ships build ships, one level deeper).
    net.set_bank(4, 200);
    net.client_send_spawn_order(3, HullKind::FleetCarrier);
    net.run_ticks(32);
    kill_client(&mut net, 4);
    assert_eq!(net.server_ship_hull(4), Some(HullKind::FleetCarrier));
    let fleet_pos = Vec2::new(2200.0, -1800.0);
    net.teleport(4, fleet_pos, 0.0);
    net.run_ticks(64);

    let fleet_entity = net.client_find_ship(1, 4).expect("client 2 sees the fleet carrier");
    net.client_send_spawn_order_loadout(1, HullKind::Outfitter, Some(fleet_entity), Loadout::default());
    net.run_ticks(32);
    kill_client(&mut net, 2);
    assert_eq!(net.server_ship_hull(2), Some(HullKind::Outfitter));
    assert_eq!(net.server_bank(2), 200 - 30, "outfitter costs 30");
    let (outfitter_spawn, _) = net.server_ship(2).unwrap();
    assert!(
        outfitter_spawn.distance(fleet_pos) < 350.0,
        "outfitter must be built beside the fleet carrier"
    );
    let outfitter_pos = Vec2::new(2600.0, -1400.0);
    net.teleport(2, outfitter_pos, 0.0);
    net.run_ticks(64);

    // Client 6 (same team) unlocks blink — equippable ONLY by docking at
    // the outfitter. A strike-carrier-free spawn can't have it:
    net.client_send_cheat(5, CheatOrder::GivePoints(50));
    assert!(net.run_until(256, |net| net.server_points(6) == 50));
    net.client_send_unlock(5, FittingId::BlinkThruster);
    net.run_ticks(64);

    // Fly next to the outfitter and dock.
    net.set_auto_spawn(5, false);
    net.teleport(6, outfitter_pos + Vec2::new(80.0, 0.0), 0.0);
    net.run_ticks(16);
    let bank_before = net.server_bank(6);
    net.client_send_dock(5);
    assert!(
        net.run_until(256, |net| net.server_ship(6).is_none()),
        "dock never stowed the ship"
    );
    let outfitter_entity = net.client_find_ship(5, 2).expect("client 6 sees the outfitter");
    assert!(
        net.run_until(256, |net| net.client_docked_at(5) == Some(outfitter_entity)),
        "client never learned it was docked ({:?})",
        net.client_docked_at(5)
    );

    // Refit with blink and undock: instant, free, same hull.
    net.client_send_spawn_order_loadout(
        5,
        HullKind::Fighter,
        None,
        Loadout {
            utility: Some(FittingId::BlinkThruster),
            ..Default::default()
        },
    );
    net.run_ticks(16);
    net.client_send_spawn_confirm(5);
    assert!(
        net.run_until(128, |net| net.server_ship(6).is_some()),
        "undock must be immediate (no respawn delay)"
    );
    let (undock_pos, _) = net.server_ship(6).unwrap();
    assert!(
        undock_pos.distance(outfitter_pos) < 300.0,
        "must undock at the outfitter ({:.0} away)",
        undock_pos.distance(outfitter_pos)
    );
    assert_eq!(net.server_ship_hull(6), Some(HullKind::Fighter), "hull preserved");
    assert_eq!(
        net.server_ship_equipped(6).and_then(|l| l.utility),
        Some(FittingId::BlinkThruster),
        "the outfitter's exclusive must equip on undock"
    );
    assert_eq!(net.server_bank(6), bank_before, "undocking is free");
}

/// Docking at a dropoff facility deposits the hold while you shop, and the
/// stowed cargo/hull come back on undock.
#[test]
fn docking_at_a_fleet_carrier_deposits_the_hold() {
    let mut net = TestNet::new(7202, &[1, 2, 3, 4]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 4));
    net.teleport(2, Vec2::new(-4000.0, 3000.0), 0.0);
    net.teleport(3, Vec2::new(-4000.0, 3400.0), 0.0);

    net.set_bank(4, 200);
    net.client_send_spawn_order(3, HullKind::FleetCarrier);
    net.run_ticks(32);
    kill_client(&mut net, 4);
    assert_eq!(net.server_ship_hull(4), Some(HullKind::FleetCarrier));
    let fleet_pos = Vec2::new(2000.0, -2000.0);
    net.teleport(4, fleet_pos, 0.0);
    net.run_ticks(64);

    // A loaded friendly fighter docks: the hold banks instantly.
    net.set_auto_spawn(1, false);
    net.set_ship_cargo(2, u16::MAX);
    net.teleport(2, fleet_pos + Vec2::new(150.0, 0.0), 0.0);
    net.run_ticks(16);
    net.client_send_dock(1);
    assert!(
        net.run_until(256, |net| net.server_ship(2).is_none()),
        "dock never stowed the ship"
    );
    // The hover may have trickle-deposited a unit or two before the dock
    // banked the rest; the whole hold must be banked either way.
    assert_eq!(
        net.server_bank(2),
        sim::FIGHTER_CARGO_CAPACITY as u32,
        "docking at a dropoff must deposit the (whole) hold"
    );

    net.client_send_spawn_confirm(1);
    assert!(net.run_until(128, |net| net.server_ship(2).is_some()));
    assert_eq!(
        net.server_ship_cargo(2).map(|(current, _)| current),
        Some(0),
        "hold comes back empty after the deposit"
    );
}

/// Out of range: the dock request is refused and the ship stays put.
#[test]
fn docking_requires_being_at_the_facility() {
    let mut net = TestNet::new(7203, &[1]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()));
    net.set_auto_spawn(0, false);
    net.teleport(1, Vec2::new(3000.0, 3000.0), 0.0); // nowhere near anything
    net.run_ticks(16);
    net.client_send_dock(0);
    net.run_ticks(128);
    assert!(
        net.server_ship(1).is_some(),
        "an out-of-range dock request must be ignored"
    );
    assert_eq!(net.client_docked_at(0), None);
}
