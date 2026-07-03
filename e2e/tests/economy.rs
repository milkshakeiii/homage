//! M1 economy loop: crack asteroids into fragments, scoop them, and feel the
//! cargo mass penalty.

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

#[test]
fn shooting_an_asteroid_cracks_it_into_scoopable_fragments() {
    let mut net = connect_one(6501);

    // A rock dead ahead of a parked ship, far from everything else.
    net.spawn_asteroid(Vec2::new(2300.0, 2000.0), 40.0);
    net.teleport(1, Vec2::new(2000.0, 2000.0), 0.0);
    net.run_ticks(32);
    assert_eq!(net.server_asteroid_count(), 1);

    // Hold fire until the rock cracks.
    net.set_input(0, fire());
    assert!(
        net.run_until(1024, |net| net.server_asteroid_count() == 0),
        "asteroid never cracked"
    );
    net.set_input(0, Some(ShipInput::default()));

    let fragments = net.server_fragment_count();
    let expected = sim::asteroid_fragment_count(40.0) as usize;
    assert!(
        fragments >= expected.saturating_sub(2),
        "expected ~{expected} fragments, found {fragments}"
    );

    // Fragments replicate to the client.
    assert!(
        net.run_until(256, |net| net.client_fragment_count(0) >= expected - 2),
        "fragments never replicated; client sees {}",
        net.client_fragment_count(0)
    );
}

#[test]
fn flying_over_a_fragment_scoops_it() {
    let mut net = connect_one(6502);
    net.teleport(1, Vec2::new(2000.0, -2000.0), 0.0);
    net.run_ticks(16);

    // A fragment just ahead: thrust through it.
    net.spawn_fragment(Vec2::new(2150.0, -2000.0));
    net.set_input(0, thrust());
    assert!(
        net.run_until(256, |net| net.server_ship_cargo(1)
            .is_some_and(|(current, _)| current == sim::FRAGMENT_VALUE)),
        "fragment was never scooped; cargo {:?}",
        net.server_ship_cargo(1)
    );
    assert_eq!(net.server_fragment_count(), 0, "scooped fragment lingers");
}

#[test]
fn a_full_hold_rejects_further_scooping() {
    let mut net = connect_one(6503);
    net.teleport(1, Vec2::new(-2000.0, -2000.0), 0.0);
    net.set_ship_cargo(1, u16::MAX); // clamps to capacity
    net.run_ticks(8);

    net.spawn_fragment(Vec2::new(-1900.0, -2000.0));
    net.set_input(0, thrust());
    net.run_ticks(192);
    assert_eq!(
        net.server_fragment_count(),
        1,
        "full ship should leave ore floating"
    );
}

#[test]
fn depositing_at_the_mothership_banks_resources() {
    let mut net = connect_one(6505);
    let team = net.server_ship_team(1).unwrap();

    // Loaded ship parked inside its own mothership's deposit radius.
    net.set_ship_cargo(1, u16::MAX); // clamps to capacity (5)
    net.teleport(
        1,
        sim::team_anchor(team) + Vec2::new(sim::MOTHERSHIP_RADIUS + 60.0, 0.0),
        0.0,
    );

    let capacity = net.server_ship_cargo(1).unwrap().1 as u32;
    assert!(
        net.run_until(512, |net| net.server_bank(1) == capacity
            && net.server_ship_cargo(1).is_some_and(|(current, _)| current == 0)),
        "deposit never completed: bank {} cargo {:?}",
        net.server_bank(1),
        net.server_ship_cargo(1)
    );

    // The bank value reaches the owning client's HUD.
    assert!(
        net.run_until(256, |net| net.client_bank(0) == Some(capacity)),
        "bank never replicated to client: {:?}",
        net.client_bank(0)
    );
}

#[test]
fn enemy_dropoffs_refuse_your_ore() {
    let mut net = connect_one(6506);
    let team = net.server_ship_team(1).unwrap();

    net.set_ship_cargo(1, u16::MAX);
    net.teleport(
        1,
        sim::team_anchor(team.opponent()) + Vec2::new(sim::MOTHERSHIP_RADIUS + 60.0, 0.0),
        0.0,
    );
    net.run_ticks(256);
    assert_eq!(net.server_bank(1), 0, "enemy dropoff accepted a deposit");
    let (current, capacity) = net.server_ship_cargo(1).unwrap();
    assert_eq!(current, capacity, "cargo drained at an enemy dropoff");
}

#[test]
fn dying_with_cargo_drops_it_and_keeps_the_bank() {
    let mut net = TestNet::new(6507, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );

    // Victim (client 2) has banked ore AND a full hold.
    net.set_ship_cargo(2, u16::MAX);
    let world = net.server.world_mut();
    world
        .resource_mut::<homage_server::Banks>()
        .0
        .insert(lightyear::prelude::PeerId::Netcode(2), 7);

    // Line up the (opposite-team) kill far from everything.
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    let dropped_expected = net.server_ship_cargo(2).unwrap().0 as usize;
    assert!(dropped_expected > 0);

    net.set_input(0, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(2).is_none()),
        "target never died"
    );
    net.set_input(0, Some(ShipInput::default()));

    // The hold scattered as scoopable fragments; the bank survived.
    assert!(
        net.server_fragment_count() >= dropped_expected,
        "expected >= {dropped_expected} dropped fragments, found {}",
        net.server_fragment_count()
    );
    assert_eq!(net.server_bank(2), 7, "bank must survive death");

    // And the respawned ship comes back with an empty hold + intact bank.
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 256,
            |net| net.server_ship(2).is_some()
        ),
        "target never respawned"
    );
    assert_eq!(net.server_ship_cargo(2).unwrap().0, 0);
    assert!(
        net.run_until(256, |net| net.client_bank(1) == Some(7)),
        "respawned ship's bank not replicated: {:?}",
        net.client_bank(1)
    );
}

#[test]
fn cargo_mass_slows_the_ship() {
    let mut net = connect_one(6504);

    // Full-throttle sprint with an empty hold...
    net.teleport(1, Vec2::new(0.0, -3000.0), 0.0);
    net.set_input(0, thrust());
    net.run_ticks(128);
    let empty_speed = net.server_ship_velocity(1).unwrap().length();

    // ...then the same sprint fully loaded.
    net.set_input(0, Some(ShipInput::default()));
    net.teleport(1, Vec2::new(0.0, -3000.0), 0.0);
    net.set_ship_cargo(1, u16::MAX);
    net.run_ticks(8);
    net.set_input(0, thrust());
    net.run_ticks(128);
    let loaded_speed = net.server_ship_velocity(1).unwrap().length();

    assert!(
        loaded_speed < empty_speed * (1.0 - sim::CARGO_SPEED_PENALTY + 0.08),
        "full hold should slow the ship: empty {empty_speed:.0} vs loaded {loaded_speed:.0}"
    );
}
