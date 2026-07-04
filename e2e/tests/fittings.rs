//! M3 fittings: unlocks spend points and persist; loadouts validate against
//! facility stocking; equipped fittings change the simulation.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{CheatOrder, FittingId, HullKind, Loadout, ShipInput};
use homage_shared::{fittings, sim};

const CONNECT_TICKS: usize = 1024;

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

fn scuttle_and_respawn(net: &mut TestNet, client_id: u64, client_idx: usize) {
    net.client_send_self_destruct(client_idx);
    assert!(net.run_until(256, |net| net.server_ship(client_id).is_none()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(client_id).is_some()
    ));
}

#[test]
fn unlock_spends_points_and_survives_death() {
    let mut net = connect_one(6901);
    net.client_send_cheat(0, CheatOrder::GivePoints(50));
    assert!(net.run_until(256, |net| net.server_points(1) == 50));

    let cost = fittings::def(FittingId::ScatterGun).cost;
    net.client_send_unlock(0, FittingId::ScatterGun);
    assert!(
        net.run_until(256, |net| net.server_points(1) == 50 - cost),
        "unlock never charged; points {}",
        net.server_points(1)
    );

    // Repeat purchase must not double-charge.
    net.client_send_unlock(0, FittingId::ScatterGun);
    net.run_ticks(64);
    assert_eq!(net.server_points(1), 50 - cost, "double charge");

    // The unlock outlives the ship.
    net.teleport(1, Vec2::new(2500.0, 2500.0), 0.0);
    net.run_ticks(8);
    scuttle_and_respawn(&mut net, 1, 0);
    assert_eq!(net.server_points(1), 50 - cost, "points must persist");
}

/// A scatter gun selected at the mothership (which doesn't stock it) falls
/// back to the pulse cannon; hull mods stocked everywhere apply anywhere.
#[test]
fn loadout_respects_facility_stocking() {
    let mut net = connect_one(6902);
    net.client_send_cheat(0, CheatOrder::GivePoints(100));
    assert!(net.run_until(256, |net| net.server_points(1) == 100));
    net.client_send_unlock(0, FittingId::ScatterGun);
    net.client_send_unlock(0, FittingId::ArmorPlate);
    net.run_ticks(64);

    // Spawn at the mothership (no facility choice = mothership) asking for
    // both: the carrier-stocked weapon falls back, the everywhere-stocked
    // hull mod sticks.
    net.client_send_spawn_order_loadout(
        0,
        HullKind::Fighter,
        None,
        Loadout {
            weapon: FittingId::ScatterGun,
            utility: None,
            hull_mod: Some(FittingId::ArmorPlate),
        },
    );
    net.run_ticks(32);
    net.teleport(1, Vec2::new(2500.0, 2500.0), 0.0);
    net.run_ticks(8);
    scuttle_and_respawn(&mut net, 1, 0);

    let equipped = net.server_ship_equipped(1).expect("ship has Equipped");
    assert_eq!(
        equipped.weapon,
        FittingId::PulseCannon,
        "carrier-stocked weapon must fall back at the mothership"
    );
    assert_eq!(equipped.hull_mod, Some(FittingId::ArmorPlate));
    assert_eq!(
        net.server_ship_max_health(1),
        Some(sim::SHIP_HEALTH + 2),
        "armor plate must add hull"
    );
}

/// The scatter gun really fires three pellets (spawned at a carrier).
#[test]
fn scatter_gun_fires_three_pellets() {
    // 1/3 share a team; 2/4 share a team.
    let mut net = TestNet::new(6903, &[1, 2, 3, 4]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 4));
    net.teleport(2, Vec2::new(0.0, 3000.0), 0.0);
    net.teleport(3, Vec2::new(0.0, -3000.0), 0.0);

    // Client 4 (team 2) becomes a carrier; client 2 will spawn a scatter
    // fighter at it.
    net.set_bank(4, 100);
    net.client_send_spawn_order(3, HullKind::StrikeCarrier);
    net.run_ticks(32);
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(4, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(net.run_until(2048, |net| net.server_ship(4).is_none()));
    net.set_input(0, Some(ShipInput::default()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship_hull(4) == Some(HullKind::StrikeCarrier)
    ));
    let carrier_pos = Vec2::new(2500.0, -900.0);
    net.teleport(4, carrier_pos, 0.0);
    net.run_ticks(64);

    net.client_send_cheat(1, CheatOrder::GivePoints(50));
    assert!(net.run_until(256, |net| net.server_points(2) == 50));
    net.client_send_unlock(1, FittingId::ScatterGun);
    net.run_ticks(64);
    let carrier_entity = net.client_find_ship(1, 4).expect("sees carrier");
    net.client_send_spawn_order_loadout(
        1,
        HullKind::Fighter,
        Some(carrier_entity),
        Loadout {
            weapon: FittingId::ScatterGun,
            ..Default::default()
        },
    );
    net.run_ticks(32);
    net.teleport(2, Vec2::ZERO, 0.0);
    net.run_ticks(16);
    net.client_send_self_destruct(1);
    assert!(net.run_until(256, |net| net.server_ship(2).is_none()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(2).is_some()
    ));
    assert_eq!(
        net.server_ship_equipped(2).map(|l| l.weapon),
        Some(FittingId::ScatterGun),
        "scatter gun should equip at the carrier"
    );

    // Move somewhere clean and fire one volley: exactly three pellets.
    net.teleport(2, Vec2::new(-2500.0, -3000.0), 0.0);
    net.run_ticks(64);
    net.set_input(1, fire());
    net.run_ticks(4);
    net.set_input(1, Some(ShipInput::default()));
    assert!(
        net.run_until(64, |net| net.server_bullet_count() == 3),
        "one scatter volley should be 3 pellets, saw {}",
        net.server_bullet_count()
    );
}

/// Afterburner: holding the ability key pushes past the stock speed cap.
#[test]
fn afterburner_raises_the_speed_cap() {
    let mut net = connect_one(6904);
    net.client_send_cheat(0, CheatOrder::GivePoints(50));
    assert!(net.run_until(256, |net| net.server_points(1) == 50));
    net.client_send_unlock(0, FittingId::Afterburner);
    net.run_ticks(64);
    net.client_send_spawn_order_loadout(
        0,
        HullKind::Fighter,
        None,
        Loadout {
            utility: Some(FittingId::Afterburner),
            ..Default::default()
        },
    );
    net.run_ticks(32);
    net.teleport(1, Vec2::new(0.0, -3000.0), 0.0);
    net.run_ticks(8);
    scuttle_and_respawn(&mut net, 1, 0);
    assert_eq!(
        net.server_ship_equipped(1).and_then(|l| l.utility),
        Some(FittingId::Afterburner)
    );

    net.set_input(
        0,
        Some(ShipInput {
            thrust: true,
            ability: true,
            ..default()
        }),
    );
    net.run_ticks(256);
    let speed = net.server_ship_velocity(1).unwrap().length();
    assert!(
        speed > sim::MAX_SPEED * 1.1,
        "burner should beat the stock cap: {speed:.0} vs {}",
        sim::MAX_SPEED
    );
}

/// The bug Henry hit: unlocks happen on the death screen, where the ship
/// components that mirror wealth don't exist. The server must answer unlock
/// orders with an authoritative WealthUpdate so the dead client can see the
/// unlock (and then equip it).
#[test]
fn unlocking_while_dead_reports_back() {
    let mut net = connect_one(6905);
    net.client_send_cheat(0, CheatOrder::GivePoints(50));
    assert!(net.run_until(256, |net| net.server_points(1) == 50));

    // Die, stay dead (no auto-confirm), and unlock from the death screen.
    net.set_auto_spawn(0, false);
    net.teleport(1, Vec2::new(2500.0, 2500.0), 0.0);
    net.run_ticks(8);
    net.client_send_self_destruct(0);
    assert!(net.run_until(256, |net| net.server_ship(1).is_none()));

    net.client_send_unlock(0, FittingId::Afterburner);
    let cost = fittings::def(FittingId::Afterburner).cost;
    assert!(
        net.run_until(256, |net| net.server_points(1) == 50 - cost),
        "unlock never processed while dead"
    );
    assert!(
        net.run_until(256, |net| {
            let (_, points, unlocked) = net.client_wealth(0);
            points == 50 - cost && unlocked.contains(&FittingId::Afterburner)
        }),
        "dead client never learned about its unlock: {:?}",
        net.client_wealth(0)
    );
}
