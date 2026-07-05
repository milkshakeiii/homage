//! M3.5: the replicated roster (scoreboard data) — per-player K/D and
//! points, visible to everyone, surviving death.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{HullKind, ShipInput};
use homage_shared::{hulls, sim};

const CONNECT_TICKS: usize = 1024;

#[test]
fn roster_replicates_kd_and_points_to_everyone() {
    let mut net = TestNet::new(7001, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );

    // Kill: client 1 destroys client 2.
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(
        0,
        Some(ShipInput {
            fire: true,
            ..default()
        }),
    );
    assert!(net.run_until(2048, |net| net.server_ship(2).is_none()));
    net.set_input(0, Some(ShipInput::default()));

    let killer_points =
        sim::SHIP_HEALTH as u32 * sim::POINTS_PER_HIT + hulls::kill_bounty(HullKind::Fighter);

    // Both clients see both entries — including the dead player's.
    for idx in [0, 1] {
        assert!(
            net.run_until(512, |net| {
                let roster = net.client_roster(idx);
                roster.len() == 2
                    && roster[0] == (1, roster[0].1, 1, 0, killer_points)
                    && roster[1].0 == 2
                    && roster[1].2 == 0 // kills
                    && roster[1].3 == 1 // deaths
            }),
            "client {idx} roster wrong: {:?}",
            net.client_roster(idx)
        );
    }

    // The victim respawns; the death stays on the board.
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(2).is_some()
    ));
    let roster = net.client_roster(0);
    assert_eq!(roster[1].3, 1, "death must persist after respawn");
}
