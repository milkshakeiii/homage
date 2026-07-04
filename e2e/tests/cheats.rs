//! Dev cheats: manual-testing aids must themselves keep working.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{CheatOrder, ShipInput};
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

fn connect_one(port: u16) -> TestNet {
    let mut net = TestNet::new(port, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()),
        "client never connected"
    );
    net
}

#[test]
fn give_ore_cheat_fills_the_bank() {
    let mut net = connect_one(6701);
    net.client_send_cheat(0, CheatOrder::GiveOre(50));
    assert!(
        net.run_until(256, |net| net.server_bank(1) == 50),
        "cheat ore never arrived; bank {}",
        net.server_bank(1)
    );
    assert!(
        net.run_until(256, |net| net.client_bank(0) == Some(50)),
        "cheat ore never replicated to the HUD"
    );
}

/// The target drone is a real, shootable, killable enemy — and it stays dead
/// (no respawn task for linkless ships).
#[test]
fn target_drone_takes_hits_and_stays_dead() {
    let mut net = connect_one(6702);
    net.teleport(1, Vec2::new(2000.0, 2000.0), 0.0);
    net.run_ticks(16);

    net.client_send_cheat(0, CheatOrder::SpawnTargetDrone(Vec2::new(2300.0, 2000.0)));
    assert!(
        net.run_until(256, |net| net.server_ships().len() == 2),
        "drone never spawned"
    );

    // Shoot it dead.
    net.set_input(
        0,
        Some(ShipInput {
            fire: true,
            ..default()
        }),
    );
    assert!(
        net.run_until(2048, |net| net.server_ships().len() == 1),
        "drone never died"
    );
    net.set_input(0, Some(ShipInput::default()));

    // And it must not respawn.
    net.run_ticks(sim::RESPAWN_DELAY_TICKS as usize + 128);
    assert_eq!(net.server_ships().len(), 1, "drones must stay dead");
}

#[test]
fn teleport_cheat_moves_the_ship() {
    let mut net = connect_one(6703);
    let target = Vec2::new(3000.0, -1500.0);
    net.client_send_cheat(0, CheatOrder::Teleport(target));
    assert!(
        net.run_until(256, |net| net
            .server_ship(1)
            .is_some_and(|(pos, _)| pos.distance(target) < 50.0)),
        "teleport never landed; at {:?}",
        net.server_ship(1)
    );
}
