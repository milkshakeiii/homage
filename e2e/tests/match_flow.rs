//! M4: the win condition — small arms bounce off motherships, torpedoes
//! don't, and a mothership kill announces the winner and resets the world.

use bevy::prelude::*;
use homage_e2e::TestNet;
use homage_shared::protocol::{HullKind, Loadout, ShipInput, Team};
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

fn fire() -> Option<ShipInput> {
    Some(ShipInput {
        fire: true,
        ..default()
    })
}

#[test]
fn torpedoes_kill_the_mothership_and_the_match_resets() {
    // 1/3 one team, 2/4 the other.
    let mut net = TestNet::new(7101, &[1, 2, 3, 4]);
    assert!(net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 4));
    let team1 = net.server_ship_team(1).unwrap();
    let team2 = team1.opponent();
    net.teleport(2, Vec2::new(0.0, 3000.0), 0.0);
    net.teleport(3, Vec2::new(0.0, -3000.0), 0.0);

    // Small arms: a fighter emptying into the enemy mothership does nothing.
    let anchor2 = sim::team_anchor(team2);
    net.teleport(1, anchor2 + Vec2::new(-400.0, 0.0), 0.0);
    net.run_ticks(16);
    net.set_input(0, fire());
    net.run_ticks(192);
    net.set_input(0, Some(ShipInput::default()));
    assert_eq!(
        net.server_mothership_health(team2),
        Some((sim::MOTHERSHIP_HEALTH, sim::MOTHERSHIP_HEALTH)),
        "small arms must bounce off the mothership"
    );

    // Client 4 becomes team 2's carrier so client 2 can field a bomber.
    net.teleport(1, Vec2::ZERO, 0.0);
    net.set_bank(4, 100);
    net.client_send_spawn_order(3, HullKind::StrikeCarrier);
    net.run_ticks(32);
    net.teleport(4, Vec2::new(300.0, 0.0), 0.0);
    net.run_ticks(64);
    net.set_input(0, fire());
    assert!(net.run_until(2048, |net| net.server_ship(4).is_none()));
    net.set_input(0, Some(ShipInput::default()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship_hull(4) == Some(HullKind::StrikeCarrier)
    ));
    net.teleport(4, Vec2::new(2000.0, -2000.0), 0.0);
    net.run_ticks(64);

    // Client 2 rides a bomber out of that carrier.
    net.set_bank(2, 100);
    let carrier_entity = net.client_find_ship(1, 4).expect("sees carrier");
    net.client_send_spawn_order_loadout(
        1,
        HullKind::Bomber,
        Some(carrier_entity),
        Loadout::default(),
    );
    net.run_ticks(32);
    net.teleport(2, Vec2::new(3000.0, 3000.0), 0.0);
    net.run_ticks(8);
    net.client_send_self_destruct(1);
    assert!(net.run_until(256, |net| net.server_ship(2).is_none()));
    assert!(net.run_until(
        sim::RESPAWN_DELAY_TICKS as usize + 256,
        |net| net.server_ship_hull(2) == Some(HullKind::Bomber)
    ));

    // Torpedo the enemy (team 1) mothership: damage lands through the DR.
    let anchor1 = sim::team_anchor(team1);
    net.teleport(2, anchor1 + Vec2::new(400.0, 0.0), core::f32::consts::PI);
    net.run_ticks(64);
    net.set_input(1, fire());
    assert!(
        net.run_until(1024, |net| {
            net.server_mothership_health(team1)
                .is_some_and(|(current, max)| current < max)
        }),
        "torpedoes must damage the mothership"
    );
    net.set_input(1, Some(ShipInput::default()));

    // Bring it to the brink, land the killing torpedo, and watch the flow.
    net.set_mothership_health(team1, 1);
    net.set_input(1, fire());
    assert!(
        net.run_until(1024, |net| net
            .server_mothership_health(team1)
            .is_some_and(|(current, _)| current == 0)),
        "the killing blow never landed"
    );
    net.set_input(1, Some(ShipInput::default()));

    // Everyone hears who won.
    for idx in [0, 1, 2, 3] {
        assert!(
            net.run_until(512, |net| net.client_last_match_result(idx) == Some(team2)),
            "client {idx} never heard the result"
        );
    }

    // The world resets: fresh motherships, cleared ledgers, everyone dead.
    assert!(
        net.run_until(
            sim::MATCH_RESET_TICKS as usize + 256,
            |net| net.server_mothership_health(team1)
                == Some((sim::MOTHERSHIP_HEALTH, sim::MOTHERSHIP_HEALTH))
        ),
        "mothership never came back at full health"
    );
    assert_eq!(net.server_bank(2), 0, "banks must reset");
    assert_eq!(net.server_points(2), 0, "points must reset");

    // And the next match begins: players redeploy through the spawn screen.
    assert!(
        net.run_until(
            sim::RESPAWN_DELAY_TICKS as usize + 512,
            |net| net.server_ships().len() == 4
        ),
        "players never redeployed after the reset (auto-confirm clients)"
    );
    let blue = net
        .server_mothership_health(Team::Blue)
        .expect("blue mothership");
    let red = net
        .server_mothership_health(Team::Red)
        .expect("red mothership");
    assert_eq!(blue.0, blue.1);
    assert_eq!(red.0, red.1);
}
