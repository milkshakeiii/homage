//! M1: teams and motherships — assignment balance, team spawn anchors, and
//! mothership replication.

use homage_e2e::TestNet;
use homage_shared::protocol::Team;
use homage_shared::sim;

const CONNECT_TICKS: usize = 1024;

#[test]
fn teams_balance_and_ships_spawn_at_their_anchor() {
    let mut net = TestNet::new(6401, &[1, 2]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ships().len() == 2),
        "both clients never spawned"
    );

    // Two players end up on opposite teams.
    let team1 = net.server_ship_team(1).expect("ship 1 has a team");
    let team2 = net.server_ship_team(2).expect("ship 2 has a team");
    assert_eq!(team1, team2.opponent(), "two players should split teams");

    // Each spawns on the ring around their own mothership.
    for id in [1, 2] {
        let team = net.server_ship_team(id).unwrap();
        let (pos, _) = net.server_ship(id).unwrap();
        let dist = pos.distance(sim::team_anchor(team));
        assert!(
            (dist - sim::SPAWN_RING_RADIUS).abs() < 50.0,
            "ship {id} spawned {dist:.0} from its anchor; expected ~{}",
            sim::SPAWN_RING_RADIUS
        );
    }
}

#[test]
fn motherships_replicate_to_clients() {
    let mut net = TestNet::new(6402, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.client_motherships(0).len() == 2),
        "client never saw both motherships; sees {:?}",
        net.client_motherships(0)
    );
    let motherships = net.client_motherships(0);
    for team in [Team::Blue, Team::Red] {
        let found = motherships
            .iter()
            .find(|(t, _)| *t == team)
            .unwrap_or_else(|| panic!("no {team:?} mothership"));
        assert!(
            found.1.distance(sim::team_anchor(team)) < 1.0,
            "{team:?} mothership at {:?}, expected {:?}",
            found.1,
            sim::team_anchor(team)
        );
    }
}

/// The soft boundary turns a runaway ship around instead of letting it leave
/// the map.
#[test]
fn soft_boundary_turns_ships_around() {
    let mut net = TestNet::new(6403, &[1]);
    assert!(
        net.run_until(CONNECT_TICKS, |net| net.server_ship(1).is_some()),
        "client never connected"
    );

    // Fling the ship straight at the right edge from just inside it.
    net.teleport(
        1,
        bevy::prelude::Vec2::new(sim::MAP_HALF_WIDTH - 50.0, 0.0),
        0.0,
    );
    net.run_ticks(8);
    net.set_input(
        0,
        Some(homage_shared::protocol::ShipInput {
            thrust: true,
            ..Default::default()
        }),
    );

    // It may overshoot into the margin, but must come back inside and end up
    // heading away from the edge (vx < 0 is not required while thrusting
    // outward — but x must stay bounded).
    let mut max_x: f32 = 0.0;
    for _ in 0..512 {
        net.tick();
        let (pos, _) = net.server_ship(1).unwrap();
        max_x = max_x.max(pos.x);
    }
    assert!(
        max_x < sim::MAP_HALF_WIDTH + sim::BOUNDARY_MARGIN + 200.0,
        "ship escaped the soft boundary: reached x={max_x:.0}"
    );
    let (pos, _) = net.server_ship(1).unwrap();
    assert!(
        pos.x < sim::MAP_HALF_WIDTH + sim::BOUNDARY_MARGIN,
        "ship still outside the boundary after 8s: x={:.0}",
        pos.x
    );
}
