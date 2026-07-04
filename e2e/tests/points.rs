//! M3: points — awarded automatically for team-positive actions (DESIGN §5),
//! persistent through death like the bank.

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

/// Damage pays per hit, a kill pays the victim's hull bounty on top, and the
/// total survives the earner's own death.
#[test]
fn combat_awards_points_and_they_survive_death() {
    let mut net = TestNet::new(6801, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "clients never connected"
    );
    net.teleport(1, Vec2::ZERO, 0.0);
    net.teleport(2, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);

    net.set_input(0, fire());
    assert!(
        net.run_until(2048, |net| net.server_ship(2).is_none()),
        "victim never died"
    );
    net.set_input(0, Some(ShipInput::default()));

    let expected = sim::SHIP_HEALTH as u32 * sim::POINTS_PER_HIT
        + hulls::kill_bounty(HullKind::Fighter);
    assert_eq!(
        net.server_points(1),
        expected,
        "kill should pay {expected} (3 hits + fighter bounty)"
    );
    assert!(
        net.run_until(256, |net| net.client_points(0) == Some(expected)),
        "points never replicated to the shooter's HUD ({:?})",
        net.client_points(0)
    );

    // The shooter scuttles; points persist through the respawn.
    net.client_send_self_destruct(0);
    assert!(net.run_until(256, |net| net.server_ship(1).is_none()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship(1).is_some()
    ));
    assert_eq!(net.server_points(1), expected, "points must survive death");
    assert!(
        net.run_until(256, |net| net.client_points(0) == Some(expected)),
        "persisted points never replicated after respawn"
    );
}

/// Hauling pays too: every deposited ore unit awards a point, so pure
/// economy players level alongside fighters.
#[test]
fn deposits_award_points() {
    let mut net = TestNet::new(6802, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()),
        "client never connected"
    );
    let team = net.server_ship_team(1).unwrap();
    net.set_ship_cargo(1, u16::MAX);
    net.teleport(
        1,
        sim::team_anchor(team) + Vec2::new(sim::MOTHERSHIP_RADIUS + 60.0, 0.0),
        0.0,
    );
    let capacity = net.server_ship_cargo(1).unwrap().1 as u32;
    assert!(
        net.run_until(512, |net| net
            .server_ship_cargo(1)
            .is_some_and(|(current, _)| current == 0)),
        "deposit never completed"
    );
    assert_eq!(
        net.server_points(1),
        capacity * sim::POINTS_PER_ORE_DEPOSITED,
        "each deposited unit should pay a point"
    );
}
