//! Server app construction and systems: accepts connections, spawns one ship
//! per client, simulates the world from replicated inputs, and performs
//! lag-compensated hit detection (rewinding targets to what the shooter saw).
//!
//! Lag compensation is hand-rolled (Valve-style): every tick we record each
//! ship's post-physics position in a short history buffer; when validating a
//! bullet we rewind targets by the shooter's interpolation delay (carried on
//! every input message) and test the bullet's swept path against the rewound
//! hit circle. We don't use lightyear_avian's LagCompensationPlugin because
//! its history component is declared as a bevy Resource, which under bevy
//! 0.19's resource-uniqueness rules breaks with more than one tracked entity
//! (and its envelope child colliders fight the solver on dynamic bodies).

use avian2d::prelude::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use core::net::SocketAddr;
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::{sim, SharedPlugin};
use homage_shared::{FIXED_TIMESTEP_HZ, PRIVATE_KEY, PROTOCOL_ID, SEND_INTERVAL};
use lightyear::connection::client::Connected;
use lightyear::connection::client_of::ClientOf;
use lightyear::netcode::server_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use std::collections::VecDeque;

/// How many ticks of ship positions to keep for lag compensation.
/// 35 ticks is ~550ms at 64Hz — enough to cover typical interpolation delays.
const LAG_COMP_HISTORY_TICKS: usize = 35;
/// Ships are hit-tested as circles for rewind purposes (the visual is a
/// 32x19 triangle; a 12-unit circle is a fair middle ground).
const SHIP_HIT_RADIUS: f32 = 12.0;

/// Ring buffer of recent post-physics ship positions, for rewinding.
#[derive(Component, Default)]
struct ShipPoseHistory {
    poses: VecDeque<(Tick, Vec2)>,
}

impl ShipPoseHistory {
    fn record(&mut self, tick: Tick, position: Vec2) {
        self.poses.push_back((tick, position));
        while self.poses.len() > LAG_COMP_HISTORY_TICKS {
            self.poses.pop_front();
        }
    }

    /// Position of this ship as the shooter saw it: interpolated between
    /// `tick` and `tick + 1` by `overstep`.
    fn sample(&self, tick: Tick, overstep: f32) -> Option<Vec2> {
        let idx = self.poses.iter().position(|(t, _)| *t == tick)?;
        let (_, start) = self.poses[idx];
        let end = self.poses.get(idx + 1).map_or(start, |(_, p)| *p);
        Some(start.lerp(end, overstep))
    }
}

/// Server-local countdown to bring a dead player's ship back.
#[derive(Component)]
struct RespawnTask {
    client_id: PeerId,
    link: Entity,
    ticks_remaining: i32,
}

/// Which team each known player is on. Assignments persist through death and
/// disconnect, so rejoining players keep their side.
#[derive(Resource, Default)]
struct TeamAssignments(std::collections::HashMap<PeerId, Team>);

impl TeamAssignments {
    /// Existing assignment, or the team with fewer assigned players.
    fn assign(&mut self, client_id: PeerId) -> Team {
        if let Some(team) = self.0.get(&client_id) {
            return *team;
        }
        let blue = self.0.values().filter(|t| **t == Team::Blue).count();
        let red = self.0.len() - blue;
        let team = if blue <= red { Team::Blue } else { Team::Red };
        self.0.insert(client_id, team);
        team
    }
}

/// Address the server should listen on; read by the startup system.
#[derive(Resource)]
struct ListenAddr(SocketAddr);

/// Build the full server app, listening on `addr`. The binary adds a
/// LogPlugin and calls `run()`; tests drive the returned app with manual
/// `update()` calls instead.
pub fn build_server_app(addr: SocketAddr) -> App {
    let mut app = App::new();
    // A dedicated server must survive ECS command failures (e.g. lightyear's
    // disconnect cleanup racing against ControlledBy despawns); log instead
    // of panicking.
    app.set_error_handler(bevy::ecs::error::warn);
    // Throttle the headless main loop; the 64Hz fixed timestep accumulates
    // real time, so simulation speed is unaffected.
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 256.0,
        ))),
        StatesPlugin,
        TransformPlugin,
    ));
    app.add_plugins(ServerPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
    });
    app.add_plugins(SharedPlugin);
    app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
    app.insert_resource(ListenAddr(addr));
    app.init_resource::<TeamAssignments>();
    app.add_systems(Startup, (start_server, spawn_motherships));
    app.add_systems(FixedUpdate, (hit_detection, respawn_ships, log_ships).chain());
    // Record ship positions after physics has moved them, so the history
    // matches what gets replicated (and therefore what shooters see).
    app.add_systems(
        FixedPostUpdate,
        record_pose_history.after(PhysicsSystems::StepSimulation),
    );
    app.add_observer(handle_new_client);
    app.add_observer(handle_connected);
    app
}

fn start_server(mut commands: Commands, addr: Res<ListenAddr>) {
    let server = commands
        .spawn((
            NetcodeServer::new(NetcodeConfig {
                protocol_id: PROTOCOL_ID,
                private_key: PRIVATE_KEY,
                ..Default::default()
            }),
            LocalAddr(addr.0),
            ServerUdpIo::default(),
            Name::from("Server"),
        ))
        .id();
    commands.trigger(Start { entity: server });
    info!("Server listening on {}", addr.0);
}

/// A new link entity is created when a client starts connecting; give it a
/// `ReplicationSender` so we can replicate entities to that client.
fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender, Name::from("ClientLink")));
}

/// Each team's mothership, replicated to everyone. No InterpolationTarget:
/// interpolation only applies values once it has two updates to blend, and a
/// static structure never updates after spawn — clients read the confirmed
/// entity directly.
fn spawn_motherships(mut commands: Commands) {
    for team in [Team::Blue, Team::Red] {
        commands.spawn((
            sim::mothership_bundle(team),
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
}

/// Once a client is confirmed as connected, put it on the smaller team and
/// spawn its ship.
fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut teams: ResMut<TeamAssignments>,
    mut commands: Commands,
) {
    let Ok(client_id) = query.get(trigger.entity) else {
        return;
    };
    let team = teams.assign(client_id.0);
    spawn_ship(&mut commands, client_id.0, team, trigger.entity);
}

fn spawn_ship(commands: &mut Commands, client_id: PeerId, team: Team, link: Entity) {
    let entity = commands
        .spawn((
            sim::ship_bundle(client_id, team),
            ShipPoseHistory::default(),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
            ControlledBy {
                owner: link,
                lifetime: Default::default(),
            },
        ))
        .id();
    info!("Spawned ship {entity:?} for client {client_id:?} on {team:?}");
}

fn record_pose_history(
    timeline: Res<LocalTimeline>,
    mut ships: Query<(&Position, &mut ShipPoseHistory)>,
) {
    let tick = timeline.tick();
    for (position, mut history) in &mut ships {
        history.record(tick, position.0);
    }
}

fn segment_hits_circle(a: Vec2, b: Vec2, center: Vec2, radius: f32) -> bool {
    let ab = b - a;
    let len_sq = ab.length_squared();
    let t = if len_sq < 1e-6 {
        0.0
    } else {
        ((center - a).dot(ab) / len_sq).clamp(0.0, 1.0)
    };
    let closest = a + ab * t;
    closest.distance_squared(center) <= radius * radius
}

/// Industry-standard lag compensation: each bullet sweeps one tick's travel
/// as a segment, tested against target hit-circles rewound to the
/// interpolated state the shooter was seeing when they fired.
fn hit_detection(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    bullets: Query<(Entity, &Position, &LinearVelocity, &BulletMarker)>,
    shooters: Query<(&PlayerId, &Team, &ControlledBy)>,
    mut targets: Query<(Entity, &PlayerId, &Team, &ShipPoseHistory, &mut Health)>,
    delays: Query<&InterpolationDelay, With<ClientOf>>,
) {
    let tick = timeline.tick();
    for (bullet_entity, position, velocity, marker) in &bullets {
        // Rewind by the *shooter's* interpolation delay (sent with inputs).
        let Some((shooter_team, link)) = shooters
            .iter()
            .find(|(id, _, _)| id.0 == marker.owner)
            .map(|(_, team, controlled_by)| (*team, controlled_by.owner))
        else {
            // Shooter's ship is gone (died with bullets in flight): without
            // their delay we can't rewind fairly, so the bullet just flies on.
            continue;
        };
        let Ok(delay) = delays.get(link) else {
            continue;
        };
        let (rewind_tick, overstep) = delay.tick_and_overstep(tick);

        let seg_start = position.0;
        let seg_end = position.0 + velocity.0 * sim::TICK_DT;

        // No friendly fire: bullets only connect with the other team.
        let hit_target = targets.iter().find_map(|(entity, id, team, history, _)| {
            if id.0 == marker.owner || *team == shooter_team {
                return None;
            }
            let center = history.sample(rewind_tick, overstep)?;
            segment_hits_circle(seg_start, seg_end, center, SHIP_HIT_RADIUS + sim::BULLET_SIZE)
                .then_some(entity)
        });
        let Some(target) = hit_target else {
            continue;
        };

        commands.entity(bullet_entity).try_despawn();
        let Ok((_, target_id, _, _, mut health)) = targets.get_mut(target) else {
            continue;
        };
        let target_id = target_id.0;
        health.current = health.current.saturating_sub(1);
        info!(
            "Hit: {:?} shot {target:?} (health now {}/{})",
            marker.owner, health.current, health.max
        );
        if health.current == 0 {
            let Some(link) = shooters
                .iter()
                .find(|(id, _, _)| id.0 == target_id)
                .map(|(_, _, controlled_by)| controlled_by.owner)
            else {
                continue;
            };
            info!("Kill: {:?} destroyed {:?}", marker.owner, target_id);
            commands.entity(target).try_despawn();
            commands.spawn(RespawnTask {
                client_id: target_id,
                link,
                ticks_remaining: sim::RESPAWN_DELAY_TICKS,
            });
        }
    }
}

fn respawn_ships(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RespawnTask)>,
    links: Query<(), With<ClientOf>>,
    mut teams: ResMut<TeamAssignments>,
) {
    for (entity, mut task) in &mut tasks {
        task.ticks_remaining -= 1;
        if task.ticks_remaining > 0 {
            continue;
        }
        commands.entity(entity).despawn();
        // Skip the respawn if the client disconnected while dead.
        if links.get(task.link).is_ok() {
            info!("Respawning ship for {:?}", task.client_id);
            let team = teams.assign(task.client_id);
            spawn_ship(&mut commands, task.client_id, team, task.link);
        }
    }
}

/// Periodic snapshot of world state, for observability while the game has no
/// other diagnostics.
fn log_ships(
    mut ticks: Local<u32>,
    ships: Query<(
        Entity,
        &PlayerId,
        &Position,
        &Rotation,
        &LinearVelocity,
        &AngularVelocity,
        &Health,
    )>,
    bullets: Query<(), With<BulletMarker>>,
) {
    *ticks += 1;
    if *ticks % 320 != 0 {
        return;
    }
    info!("{} bullets in flight", bullets.iter().count());
    for (entity, id, position, rotation, linvel, angvel, health) in &ships {
        info!(
            "ship {entity:?} owner {:?} pos ({:.1}, {:.1}) rot {:.2} linvel ({:.1}, {:.1}) angvel {:.2} hp {}/{}",
            id.0,
            position.0.x,
            position.0.y,
            rotation.as_radians(),
            linvel.0.x,
            linvel.0.y,
            angvel.0,
            health.current,
            health.max
        );
    }
}
