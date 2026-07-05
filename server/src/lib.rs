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
use homage_shared::{fittings, hulls, sim, SharedPlugin};
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

/// Server-local respawn state: the countdown is the EARLIEST spawn moment;
/// the actual spawn waits for the player's SpawnConfirm (the map click on
/// the spawn screen). Early confirms are remembered.
#[derive(Component)]
struct RespawnTask {
    client_id: PeerId,
    link: Entity,
    ticks_remaining: i32,
    confirmed: bool,
}

/// Which team each known player is on. Assignments persist through death and
/// disconnect, so rejoining players keep their side.
#[derive(Resource, Default)]
struct TeamAssignments(std::collections::HashMap<PeerId, Team>);

/// The authoritative store of deposited resources per player. Lives outside
/// the ship entity so it survives death (DESIGN §3: deposited resources are
/// never lost); mirrored onto each ship's `Bank` component for replication.
#[derive(Resource, Default)]
pub struct Banks(pub std::collections::HashMap<PeerId, u32>);

/// What each player wants to fly on their next spawn, and where (from
/// SpawnOrder messages). Costs and facility eligibility are checked at
/// spawn time.
#[derive(Resource, Default)]
struct SpawnChoices(std::collections::HashMap<PeerId, SpawnOrder>);

/// The authoritative points ledger (DESIGN §5), keyed by player so it
/// survives death like the banks; mirrored onto each ship's `Points`
/// component by `sync_points`.
#[derive(Resource, Default)]
pub struct PointsStore(pub std::collections::HashMap<PeerId, u32>);

impl PointsStore {
    fn award(&mut self, player: PeerId, amount: u32) {
        *self.0.entry(player).or_insert(0) += amount;
    }
}

/// Ships stowed at a facility for refits (DESIGN §6 docking). The ship
/// entity is despawned; undocking (SpawnConfirm while docked) restores it
/// in place with hull, health, and cargo intact — no cost, no delay.
#[derive(Resource, Default)]
pub struct DockedStates(pub std::collections::HashMap<PeerId, DockedShip>);

#[derive(Clone, Copy, Debug)]
pub struct DockedShip {
    pub hull: HullKind,
    pub health: u16,
    pub cargo: u16,
    pub facility: Entity,
    pub link: Entity,
    pub team: Team,
}

/// Intermission countdown after a mothership falls; the world resets when
/// it hits zero.
#[derive(Resource, Default)]
pub struct MatchState {
    pub ending: Option<(Team, i32)>,
}

/// Kill/death tallies per player (scoreboard data; survives death).
#[derive(Resource, Default)]
pub struct KdStore(pub std::collections::HashMap<PeerId, (u32, u32)>);

impl KdStore {
    fn kill(&mut self, player: PeerId) {
        self.0.entry(player).or_default().0 += 1;
    }
    fn death(&mut self, player: PeerId) {
        self.0.entry(player).or_default().1 += 1;
    }
}

/// One replicated roster entity per known player: team, K/D, points. The
/// scoreboard reads these — ships die, the roster doesn't.
fn sync_roster(
    mut commands: Commands,
    mut index: Local<std::collections::HashMap<PeerId, Entity>>,
    teams: Res<TeamAssignments>,
    kd: Res<KdStore>,
    points: Res<PointsStore>,
    mut entries: Query<(&mut Team, &mut Kills, &mut Deaths, &mut Points), With<RosterEntry>>,
) {
    for (player, team) in teams.0.iter() {
        let (kills, deaths) = kd.0.get(player).copied().unwrap_or((0, 0));
        let score = points.0.get(player).copied().unwrap_or(0);
        match index.get(player).copied() {
            Some(entity) => {
                if let Ok((mut t, mut k, mut d, mut p)) = entries.get_mut(entity) {
                    if *t != *team {
                        *t = *team;
                    }
                    if k.0 != kills {
                        k.0 = kills;
                    }
                    if d.0 != deaths {
                        d.0 = deaths;
                    }
                    if p.0 != score {
                        p.0 = score;
                    }
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        RosterEntry(*player),
                        *team,
                        Kills(kills),
                        Deaths(deaths),
                        Points(score),
                        Name::from("RosterEntry"),
                        Replicate::to_clients(NetworkTarget::All),
                    ))
                    .id();
                index.insert(*player, entity);
            }
        }
    }
}

/// Match-permanent fitting unlocks per player (DESIGN §5: points are spent
/// on the unlock; nothing is re-bought per life).
#[derive(Resource, Default)]
pub struct Unlocks(pub std::collections::HashMap<PeerId, std::collections::HashSet<FittingId>>);

impl Unlocks {
    pub fn has(&self, player: PeerId, fitting: FittingId) -> bool {
        fittings::def(fitting).cost == 0
            || self.0.get(&player).is_some_and(|set| set.contains(&fitting))
    }
}

/// Spend points to unlock a fitting; idempotent and refuses on insufficient
/// points.
fn receive_unlock_orders(
    mut receivers: Query<
        (
            &RemoteId,
            &mut MessageReceiver<UnlockOrder>,
            &mut MessageSender<WealthUpdate>,
        ),
        With<ClientOf>,
    >,
    mut unlocks: ResMut<Unlocks>,
    mut points: ResMut<PointsStore>,
    banks: Res<Banks>,
) {
    for (client_id, mut receiver, mut sender) in &mut receivers {
        let mut respond = false;
        for order in receiver.receive() {
            respond = true;
            let def = fittings::def(order.fitting);
            if unlocks.has(client_id.0, order.fitting) {
                continue;
            }
            let balance = points.0.entry(client_id.0).or_insert(0);
            if *balance < def.cost {
                info!(
                    "{:?} can't afford {:?} ({} pts, has {})",
                    client_id.0, order.fitting, def.cost, balance
                );
                continue;
            }
            *balance -= def.cost;
            unlocks.0.entry(client_id.0).or_default().insert(order.fitting);
            info!("{:?} unlocked {:?} for {} pts", client_id.0, order.fitting, def.cost);
        }
        // Answer with the authoritative snapshot: the ship components that
        // normally mirror wealth are dead exactly when unlocks happen.
        if respond {
            let mut unlocked: Vec<FittingId> = unlocks
                .0
                .get(&client_id.0)
                .map(|set| set.iter().copied().collect())
                .unwrap_or_default();
            unlocked.sort();
            sender.send::<OrdersChannel>(WealthUpdate {
                bank: banks.0.get(&client_id.0).copied().unwrap_or(0),
                points: points.0.get(&client_id.0).copied().unwrap_or(0),
                unlocked,
            });
        }
    }
}

/// Mirror unlocks onto ship components for the spawn-screen UI.
fn sync_unlocks(
    unlocks: Res<Unlocks>,
    mut ships: Query<(&PlayerId, &mut UnlockedFittings)>,
) {
    for (player, mut component) in &mut ships {
        let mut list: Vec<FittingId> = unlocks
            .0
            .get(&player.0)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        list.sort();
        if component.0 != list {
            component.0 = list;
        }
    }
}

/// Mirror the points ledger onto ship components for replication. Runs
/// unconditionally: respawned ships arrive with Points(0) and need their
/// persisted total restored even when the ledger itself didn't change.
fn sync_points(store: Res<PointsStore>, mut ships: Query<(&PlayerId, &mut Points)>) {
    for (player, mut points) in &mut ships {
        let total = store.0.get(&player.0).copied().unwrap_or(0);
        if points.0 != total {
            points.0 = total;
        }
    }
}

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

/// Asteroid field layout. Tests disable it (`enabled: false`) and spawn
/// precisely-placed rocks instead.
#[derive(Resource)]
pub struct AsteroidFieldConfig {
    pub enabled: bool,
    pub seed: u64,
}

impl Default for AsteroidFieldConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed: 0xA57E_401D,
        }
    }
}

/// Tiny deterministic PRNG for world generation (no external rand dep).
struct Lcg(u64);

impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

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
    app.init_resource::<Banks>();
    app.init_resource::<SpawnChoices>();
    app.init_resource::<PointsStore>();
    app.init_resource::<Unlocks>();
    app.init_resource::<KdStore>();
    app.init_resource::<MatchState>();
    app.init_resource::<DockedStates>();
    app.init_resource::<AsteroidFieldConfig>();
    app.add_systems(Startup, (start_server, spawn_motherships, spawn_asteroid_field));
    // Message drains MUST run in Update: lightyear clears MessageReceiver
    // buffers every render frame (in Last), and FixedUpdate doesn't run
    // every frame — draining there silently drops most messages (the e2e
    // harness can't catch this: manual stepping runs exactly one tick per
    // frame, masking the race).
    app.add_systems(
        Update,
        (
            receive_spawn_orders,
            receive_spawn_confirms,
            receive_dock_requests,
            receive_unlock_orders,
            receive_self_destructs,
            receive_cheats,
        ),
    );
    app.add_systems(
        FixedUpdate,
        (
            hit_detection,
            mothership_hit_detection,
            asteroid_hit_detection,
            scoop_fragments,
            deposit_cargo,
            sync_points,
            sync_unlocks,
            sync_roster,
            run_match_reset,
            respawn_ships,
            log_ships,
        )
            .chain(),
    );
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

/// Generate the asteroid field (DESIGN §8): a dense contested belt in the
/// middle, plus a safer thin belt near each mothership for bootstrap
/// harvesting. Deterministic from the config seed.
fn spawn_asteroid_field(config: Res<AsteroidFieldConfig>, mut commands: Commands) {
    generate_asteroid_field(&config, &mut commands);
}

fn generate_asteroid_field(config: &AsteroidFieldConfig, commands: &mut Commands) {
    if !config.enabled {
        return;
    }
    let mut rng = Lcg(config.seed);
    let mut placed: Vec<(Vec2, f32)> = Vec::new();

    let try_place = |placed: &mut Vec<(Vec2, f32)>, pos: Vec2, radius: f32| {
        let clear = placed
            .iter()
            .all(|(p, r)| p.distance(pos) > (r + radius) * 1.6)
            && [Team::Blue, Team::Red].iter().all(|t| {
                sim::team_anchor(*t).distance(pos) > sim::SPAWN_RING_RADIUS + radius + 150.0
            });
        if clear {
            placed.push((pos, radius));
        }
        clear
    };

    // Central contested belt.
    let mut central = 0;
    for _ in 0..400 {
        if central >= 48 {
            break;
        }
        let pos = Vec2::new(rng.range(-1800.0, 1800.0), rng.range(-3600.0, 3600.0));
        let radius = rng.range(sim::ASTEROID_MIN_RADIUS, sim::ASTEROID_MAX_RADIUS);
        if try_place(&mut placed, pos, radius) {
            let seed = (rng.next_f32() * u16::MAX as f32) as u16;
            commands.spawn((
                sim::asteroid_bundle(pos, radius, seed),
                Replicate::to_clients(NetworkTarget::All),
            ));
            central += 1;
        }
    }

    // Home belts: a loose ring around each mothership, outside the spawn ring.
    for team in [Team::Blue, Team::Red] {
        let anchor = sim::team_anchor(team);
        let mut home = 0;
        for _ in 0..200 {
            if home >= 10 {
                break;
            }
            let angle = rng.range(0.0, core::f32::consts::TAU);
            let dist = rng.range(700.0, 1400.0);
            let pos = anchor + Vec2::from_angle(angle) * dist;
            if pos.x.abs() > sim::MAP_HALF_WIDTH - 100.0
                || pos.y.abs() > sim::MAP_HALF_HEIGHT - 100.0
            {
                continue;
            }
            let radius = rng.range(sim::ASTEROID_MIN_RADIUS, sim::ASTEROID_MAX_RADIUS * 0.7);
            if try_place(&mut placed, pos, radius) {
                let seed = (rng.next_f32() * u16::MAX as f32) as u16;
                commands.spawn((
                    sim::asteroid_bundle(pos, radius, seed),
                    Replicate::to_clients(NetworkTarget::All),
                ));
                home += 1;
            }
        }
    }
    info!("Spawned {} asteroids", placed.len());
}

/// Bullets pound motherships: swept segment vs the hull circle, minus flat
/// damage reduction — small arms bounce off (DESIGN §2). Zero health ends
/// the match.
fn mothership_hit_detection(
    mut commands: Commands,
    mut match_state: ResMut<MatchState>,
    bullets: Query<(Entity, &Position, &LinearVelocity, &BulletMarker)>,
    shooters: Query<(&PlayerId, &Team)>,
    mut motherships: Query<(&Position, &Team, &mut Health), With<Mothership>>,
) {
    if match_state.ending.is_some() {
        return;
    }
    for (bullet_entity, position, velocity, marker) in &bullets {
        let Some(shooter_team) = shooters
            .iter()
            .find(|(id, _)| id.0 == marker.owner)
            .map(|(_, team)| *team)
        else {
            continue;
        };
        let seg_start = position.0;
        let seg_end = position.0 + velocity.0 * sim::TICK_DT;
        for (mothership_pos, team, mut health) in &mut motherships {
            if *team == shooter_team
                || !segment_hits_circle(
                    seg_start,
                    seg_end,
                    mothership_pos.0,
                    sim::MOTHERSHIP_RADIUS + sim::BULLET_SIZE,
                )
            {
                continue;
            }
            commands.entity(bullet_entity).try_despawn();
            let damage = marker.damage.saturating_sub(sim::MOTHERSHIP_DAMAGE_REDUCTION);
            if damage == 0 {
                break;
            }
            health.current = health.current.saturating_sub(damage);
            if health.current == 0 {
                info!("MATCH OVER: {team:?} mothership destroyed; {shooter_team:?} wins");
                match_state.ending = Some((shooter_team, sim::MATCH_RESET_TICKS));
            }
            break;
        }
    }
}

/// Run the intermission: announce the winner once, count down, then reset
/// the world for a fresh match (ledgers cleared, field regenerated, everyone
/// back through the spawn screen).
#[allow(clippy::too_many_arguments)]
fn run_match_reset(
    mut commands: Commands,
    mut match_state: ResMut<MatchState>,
    mut announced: Local<bool>,
    mut senders: Query<&mut MessageSender<MatchResult>, With<ClientOf>>,
    links: Query<(Entity, &RemoteId), With<ClientOf>>,
    world_entities: Query<
        Entity,
        Or<(
            With<PlayerId>,
            With<BulletMarker>,
            With<OreFragment>,
            With<Asteroid>,
            With<Mothership>,
            With<RespawnTask>,
        )>,
    >,
    mut banks: ResMut<Banks>,
    mut points: ResMut<PointsStore>,
    mut unlocks: ResMut<Unlocks>,
    mut kd: ResMut<KdStore>,
    mut docked: ResMut<DockedStates>,
    config: Res<AsteroidFieldConfig>,
) {
    let Some((winner, ticks_remaining)) = match_state.ending else {
        *announced = false;
        return;
    };
    if !*announced {
        *announced = true;
        for mut sender in &mut senders {
            sender.send::<OrdersChannel>(MatchResult { winner });
        }
    }
    if ticks_remaining > 0 {
        match_state.ending = Some((winner, ticks_remaining - 1));
        return;
    }
    info!("Resetting the world for a new match");
    match_state.ending = None;
    for entity in &world_entities {
        commands.entity(entity).try_despawn();
    }
    banks.0.clear();
    points.0.clear();
    unlocks.0.clear();
    kd.0.clear();
    docked.0.clear();
    for team in [Team::Blue, Team::Red] {
        commands.spawn((
            sim::mothership_bundle(team),
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
    generate_asteroid_field(&config, &mut commands);
    // Everyone re-enters through the spawn screen.
    for (link, client_id) in &links {
        commands.spawn(RespawnTask {
            client_id: client_id.0,
            link,
            ticks_remaining: sim::RESPAWN_DELAY_TICKS,
            confirmed: false,
        });
    }
}

/// Bullets crack asteroids: swept segment vs the rock's circle (no lag
/// compensation — the rocks don't move). A cracked asteroid ejects ore
/// fragments in a deterministic fan.
fn asteroid_hit_detection(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    bullets: Query<(Entity, &Position, &LinearVelocity, &BulletMarker)>,
    mut asteroids: Query<(Entity, &Position, &Asteroid, &mut Health)>,
    mut points: ResMut<PointsStore>,
) {
    let tick = timeline.tick();
    for (bullet_entity, position, velocity, marker) in &bullets {
        let seg_start = position.0;
        let seg_end = position.0 + velocity.0 * sim::TICK_DT;
        for (asteroid_entity, apos, asteroid, mut health) in &mut asteroids {
            if !segment_hits_circle(
                seg_start,
                seg_end,
                apos.0,
                asteroid.radius * 0.9 + sim::BULLET_SIZE,
            ) {
                continue;
            }
            commands.entity(bullet_entity).try_despawn();
            health.current = health.current.saturating_sub(marker.damage);
            if health.current == 0 {
                commands.entity(asteroid_entity).try_despawn();
                let count = sim::asteroid_fragment_count(asteroid.radius);
                for i in 0..count {
                    let angle = i as f32 / count as f32 * core::f32::consts::TAU
                        + asteroid.seed as f32;
                    let dir = Vec2::from_angle(angle);
                    let speed = sim::FRAGMENT_SPEED * (0.6 + 0.4 * ((i * 7 % 5) as f32 / 4.0));
                    commands.spawn((
                        sim::fragment_bundle(
                            apos.0 + dir * asteroid.radius * 0.5,
                            dir * speed,
                            tick,
                        ),
                        Replicate::to_clients(NetworkTarget::All),
                        InterpolationTarget::to_clients(NetworkTarget::All),
                    ));
                }
                points.award(marker.owner, sim::asteroid_crack_points(asteroid.radius));
                info!("Asteroid {asteroid_entity:?} cracked into {count} fragments");
            }
            break;
        }
    }
}

/// Hovering inside a friendly dropoff's radius transfers ore from the hold
/// into the player's bank, one unit per DEPOSIT_INTERVAL_TICKS: deposits are
/// a deliberate, vulnerable pause rather than a drive-by (DESIGN §3).
/// Dropoffs: the mothership, plus any friendly resource controller (the
/// mobile forward dropoff, DESIGN §5).
fn deposit_cargo(
    timeline: Res<LocalTimeline>,
    mut banks: ResMut<Banks>,
    motherships: Query<(&Position, &Team), With<Mothership>>,
    controllers: Query<(&Position, &Team, &HullKind), With<PlayerId>>,
    mut ships: Query<(&PlayerId, &Team, &Position, &mut CargoHold, &mut Bank)>,
    mut points: ResMut<PointsStore>,
) {
    if timeline.tick().0 % sim::DEPOSIT_INTERVAL_TICKS as u32 != 0 {
        return;
    }
    let dropoffs: Vec<(Vec2, Team, f32)> = motherships
        .iter()
        .map(|(pos, team)| (pos.0, *team, sim::DEPOSIT_RADIUS))
        .chain(controllers.iter().filter_map(|(pos, team, kind)| match kind {
            HullKind::ResourceController => {
                Some((pos.0, *team, sim::CONTROLLER_DEPOSIT_RADIUS))
            }
            HullKind::FleetCarrier => Some((
                pos.0,
                *team,
                hulls::stats(HullKind::FleetCarrier).width / 2.0 + 110.0,
            )),
            _ => None,
        }))
        .collect();
    for (player, team, position, mut hold, mut bank) in &mut ships {
        if hold.current == 0 {
            continue;
        }
        let at_dropoff = dropoffs.iter().any(|(dpos, dteam, radius)| {
            dteam == team && dpos.distance_squared(position.0) < radius * radius
        });
        if !at_dropoff {
            continue;
        }
        hold.current -= 1;
        let total = banks.0.entry(player.0).or_insert(0);
        *total += 1;
        bank.0 = *total;
        points.award(player.0, sim::POINTS_PER_ORE_DEPOSITED);
    }
}

/// Flying over a fragment scoops it into the hold — if it fits. Full holds
/// leave ore floating: go deposit.
fn scoop_fragments(
    mut commands: Commands,
    fragments: Query<(Entity, &Position, &OreFragment)>,
    mut ships: Query<(&Position, &mut CargoHold), With<PlayerId>>,
) {
    for (fragment_entity, fragment_pos, fragment) in &fragments {
        for (ship_pos, mut hold) in &mut ships {
            if ship_pos.0.distance_squared(fragment_pos.0)
                > sim::SCOOP_RADIUS * sim::SCOOP_RADIUS
            {
                continue;
            }
            if hold.current + fragment.value > hold.capacity {
                continue;
            }
            hold.current += fragment.value;
            commands.entity(fragment_entity).try_despawn();
            break;
        }
    }
}

/// Once a client is confirmed as connected, put it on the smaller team and
/// spawn its ship.
fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut teams: ResMut<TeamAssignments>,
    banks: Res<Banks>,
    mut commands: Commands,
) {
    let Ok(client_id) = query.get(trigger.entity) else {
        return;
    };
    let team = teams.assign(client_id.0);
    let bank = banks.0.get(&client_id.0).copied().unwrap_or(0);
    // First spawn is always the free fighter; purchases apply on respawn.
    let pose = sim::spawn_pose(client_id.0, team);
    spawn_ship(
        &mut commands,
        client_id.0,
        team,
        HullKind::Fighter,
        Loadout::default(),
        pose,
        bank,
        trigger.entity,
    );
}

/// Drain SpawnOrder messages into each player's standing choice.
fn receive_spawn_orders(
    mut receivers: Query<(&RemoteId, &mut MessageReceiver<SpawnOrder>), With<ClientOf>>,
    mut choices: ResMut<SpawnChoices>,
) {
    for (client_id, mut receiver) in &mut receivers {
        for order in receiver.receive() {
            info!(
                "{:?} wants to spawn as {:?} at {:?}",
                client_id.0, order.hull, order.spawn_at
            );
            choices.0.insert(client_id.0, order);
        }
    }
}

/// Eject a dead ship's undeposited ore as scoopable fragments (DESIGN §3).
fn scatter_cargo(commands: &mut Commands, position: Vec2, amount: u16, tick: Tick) {
    for i in 0..amount {
        let angle = i as f32 / amount.max(1) as f32 * core::f32::consts::TAU;
        let dir = Vec2::from_angle(angle);
        commands.spawn((
            sim::fragment_bundle(position + dir * 20.0, dir * sim::FRAGMENT_SPEED * 0.7, tick),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ));
    }
}

/// Reduce a requested loadout to what's unlocked and stocked at the spawn
/// facility (DESIGN §5): invalid slots fall back to defaults rather than
/// blocking the spawn.
fn validate_loadout(
    requested: Loadout,
    facility: fittings::SpawnFacility,
    player: PeerId,
    unlocks: &Unlocks,
) -> Loadout {
    let ok = |fitting: FittingId, slot: fittings::Slot| {
        let def = fittings::def(fitting);
        def.slot == slot
            && unlocks.has(player, fitting)
            && fittings::stocked_at(def.stocking, facility)
    };
    Loadout {
        weapon: if ok(requested.weapon, fittings::Slot::Weapon) {
            requested.weapon
        } else {
            FittingId::PulseCannon
        },
        utility: requested.utility.filter(|f| ok(*f, fittings::Slot::Utility)),
        hull_mod: requested.hull_mod.filter(|f| ok(*f, fittings::Slot::HullMod)),
    }
}

/// Mark a dead player's respawn as confirmed (their map click). Spawning
/// happens in respawn_ships once the delay has also elapsed.
#[allow(clippy::too_many_arguments)]
fn receive_spawn_confirms(
    mut commands: Commands,
    mut receivers: Query<(&RemoteId, &mut MessageReceiver<SpawnConfirm>), With<ClientOf>>,
    mut tasks: Query<&mut RespawnTask>,
    mut docked: ResMut<DockedStates>,
    choices: Res<SpawnChoices>,
    unlocks: Res<Unlocks>,
    banks: Res<Banks>,
    motherships: Query<(Entity, &Team, &Position), With<Mothership>>,
    facilities: Query<(Entity, &Team, &Position, &HullKind), With<PlayerId>>,
) {
    for (client_id, mut receiver) in &mut receivers {
        for _ in receiver.receive() {
            // Docked: this is an undock. Restore the stowed ship in place
            // with a loadout revalidated against the facility's stock.
            if let Some(stowed) = docked.0.remove(&client_id.0) {
                let facility_kind =
                    facility_stock_kind(stowed.facility, &motherships, &facilities);
                let (center, ring, stock) = match facility_kind {
                    Some(kind) => {
                        let pos = motherships
                            .get(stowed.facility)
                            .map(|(_, _, p)| p.0)
                            .or_else(|_| facilities.get(stowed.facility).map(|(_, _, p, _)| p.0))
                            .unwrap_or(sim::team_anchor(stowed.team));
                        let ring = match kind {
                            fittings::SpawnFacility::Mothership => sim::SPAWN_RING_RADIUS,
                            _ => facilities
                                .get(stowed.facility)
                                .map(|(.., k)| hulls::stats(*k).width / 2.0 + 90.0)
                                .unwrap_or(200.0),
                        };
                        (pos, ring, kind)
                    }
                    // Facility died while we were docked: bail out at home.
                    None => (
                        sim::team_anchor(stowed.team),
                        sim::SPAWN_RING_RADIUS,
                        fittings::SpawnFacility::Mothership,
                    ),
                };
                let requested = choices
                    .0
                    .get(&client_id.0)
                    .map(|order| order.loadout)
                    .unwrap_or_default();
                let loadout = validate_loadout(requested, stock, client_id.0, &unlocks);
                let pose = sim::spawn_pose_at(client_id.0, stowed.team, center, ring);
                let bank = banks.0.get(&client_id.0).copied().unwrap_or(0);
                spawn_ship(
                    &mut commands,
                    client_id.0,
                    stowed.team,
                    stowed.hull,
                    loadout,
                    pose,
                    bank,
                    stowed.link,
                );
                commands.queue(RestoreShipState {
                    player: client_id.0,
                    health: stowed.health,
                    cargo: stowed.cargo,
                });
                info!("{:?} undocked as {:?}", client_id.0, stowed.hull);
                continue;
            }
            for mut task in &mut tasks {
                if task.client_id == client_id.0 {
                    task.confirmed = true;
                }
            }
        }
    }
}

/// Deferred: clamp the just-undocked ship's health/cargo back to its stowed
/// values (the spawn bundle starts it fresh).
struct RestoreShipState {
    player: PeerId,
    health: u16,
    cargo: u16,
}

impl bevy::ecs::system::Command for RestoreShipState {
    type Out = ();

    fn apply(self, world: &mut World) {
        let mut ships = world.query::<(&PlayerId, &mut Health, &mut CargoHold)>();
        for (id, mut health, mut hold) in ships.iter_mut(world) {
            if id.0 == self.player {
                health.current = self.health.min(health.max);
                hold.current = self.cargo.min(hold.capacity);
            }
        }
    }
}

/// Dev cheats (manual-testing aids; strip or gate before public builds).
#[allow(clippy::too_many_arguments)]
fn receive_cheats(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut receivers: Query<(&RemoteId, &mut MessageReceiver<CheatOrder>), With<ClientOf>>,
    mut banks: ResMut<Banks>,
    mut points_store: ResMut<PointsStore>,
    mut ships: Query<(
        &PlayerId,
        &Team,
        &mut Position,
        &mut LinearVelocity,
        &mut Health,
        &mut Bank,
    )>,
    mut drone_counter: Local<u64>,
) {
    let tick = timeline.tick();
    for (client_id, mut receiver) in &mut receivers {
        for cheat in receiver.receive() {
            info!("CHEAT from {:?}: {cheat:?}", client_id.0);
            match cheat {
                CheatOrder::GivePoints(amount) => {
                    points_store.award(client_id.0, amount);
                }
                CheatOrder::GiveOre(amount) => {
                    let total = banks.0.entry(client_id.0).or_insert(0);
                    *total += amount;
                    if let Some((.., mut bank)) =
                        ships.iter_mut().find(|(id, ..)| id.0 == client_id.0)
                    {
                        bank.0 = *total;
                    }
                }
                CheatOrder::SpawnAsteroid(pos) => {
                    commands.spawn((
                        sim::asteroid_bundle(pos, 45.0, tick.0 as u16),
                        Replicate::to_clients(NetworkTarget::All),
                    ));
                }
                CheatOrder::SpawnFragments(pos) => {
                    scatter_cargo(&mut commands, pos, 6, tick);
                }
                CheatOrder::SpawnTargetDrone(pos) => {
                    let Some(team) = ships
                        .iter()
                        .find(|(id, ..)| id.0 == client_id.0)
                        .map(|(_, team, ..)| *team)
                    else {
                        continue;
                    };
                    *drone_counter += 1;
                    let drone_id = PeerId::Netcode(90_000 + *drone_counter);
                    commands.spawn((
                        sim::ship_bundle(
                            drone_id,
                            team.opponent(),
                            HullKind::Fighter,
                            Loadout::default(),
                            (Position(pos), Rotation::default()),
                        ),
                        Bank(0),
                        Points(0),
                        ShipPoseHistory::default(),
                        Replicate::to_clients(NetworkTarget::All),
                        InterpolationTarget::to_clients(NetworkTarget::All),
                    ));
                }
                CheatOrder::Teleport(pos) => {
                    if let Some((_, _, mut position, mut velocity, ..)) =
                        ships.iter_mut().find(|(id, ..)| id.0 == client_id.0)
                    {
                        position.0 = pos;
                        velocity.0 = Vec2::ZERO;
                    }
                }
                CheatOrder::Heal => {
                    if let Some((.., mut health, _)) =
                        ships.iter_mut().find(|(id, ..)| id.0 == client_id.0)
                    {
                        health.current = health.max;
                    }
                }
            }
        }
    }
}

/// Resolve a facility entity to its stocking kind (None = not dockable).
fn facility_stock_kind(
    entity: Entity,
    motherships: &Query<(Entity, &Team, &Position), With<Mothership>>,
    ships: &Query<(Entity, &Team, &Position, &HullKind), With<PlayerId>>,
) -> Option<fittings::SpawnFacility> {
    if motherships.get(entity).is_ok() {
        return Some(fittings::SpawnFacility::Mothership);
    }
    match ships.get(entity).map(|(_, _, _, kind)| *kind) {
        Ok(HullKind::StrikeCarrier) => Some(fittings::SpawnFacility::StrikeCarrier),
        Ok(HullKind::FleetCarrier) => Some(fittings::SpawnFacility::FleetCarrier),
        Ok(HullKind::Outfitter) => Some(fittings::SpawnFacility::Outfitter),
        _ => None,
    }
}

/// Stow a ship at a nearby friendly facility: despawn it, remember its
/// state, deposit its hold if the facility is a dropoff, and tell the
/// client it's docked.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn receive_dock_requests(
    mut commands: Commands,
    mut receivers: Query<
        (
            &RemoteId,
            &mut MessageReceiver<DockRequest>,
            &mut MessageSender<DockedNotice>,
        ),
        With<ClientOf>,
    >,
    ships: Query<
        (
            Entity,
            &PlayerId,
            &Team,
            &Position,
            &HullKind,
            &Health,
            Option<&CargoHold>,
            &ControlledBy,
        ),
        Without<Mothership>,
    >,
    motherships: Query<(Entity, &Team, &Position), With<Mothership>>,
    mut docked: ResMut<DockedStates>,
    mut banks: ResMut<Banks>,
    mut points: ResMut<PointsStore>,
) {
    for (client_id, mut receiver, mut sender) in &mut receivers {
        for _ in receiver.receive() {
            let Some((entity, _, team, position, hull, health, cargo, controlled_by)) = ships
                .iter()
                .find(|(_, id, ..)| id.0 == client_id.0)
            else {
                continue;
            };
            // Nearest friendly dockable facility in range.
            let mothership_target = motherships
                .iter()
                .find(|(_, t, pos)| {
                    **t == *team && pos.0.distance(position.0) < sim::dock_radius(None)
                })
                .map(|(e, ..)| (e, true));
            let Some((facility, facility_is_dropoff)) = mothership_target.or_else(|| {
                ships
                    .iter()
                    .find(|(other, _, t, pos, kind, ..)| {
                        *other != entity
                            && **t == *team
                            && hulls::is_dockable(**kind)
                            && pos.0.distance(position.0) < sim::dock_radius(Some(**kind))
                    })
                    .map(|(e, _, _, _, kind, ..)| {
                        (e, matches!(kind, HullKind::FleetCarrier))
                    })
            }) else {
                continue;
            };
            // Docking at a dropoff deposits the hold while you shop.
            let mut cargo_left = cargo.map_or(0, |hold| hold.current);
            if facility_is_dropoff && cargo_left > 0 {
                let total = banks.0.entry(client_id.0).or_insert(0);
                *total += cargo_left as u32;
                points.award(
                    client_id.0,
                    cargo_left as u32 * sim::POINTS_PER_ORE_DEPOSITED,
                );
                cargo_left = 0;
            }
            docked.0.insert(
                client_id.0,
                DockedShip {
                    hull: *hull,
                    health: health.current,
                    cargo: cargo_left,
                    facility,
                    link: controlled_by.owner,
                    team: *team,
                },
            );
            info!("{:?} docked at {facility:?}", client_id.0);
            commands.entity(entity).try_despawn();
            sender.send::<OrdersChannel>(DockedNotice { facility });
        }
    }
}

/// Scuttle on request: the only way to swap hulls without an enemy's help.
/// Same consequences as any death (cargo scatters, normal respawn delay).
fn receive_self_destructs(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut receivers: Query<(&RemoteId, &mut MessageReceiver<SelfDestruct>), With<ClientOf>>,
    ships: Query<(Entity, &PlayerId, &Position, Option<&CargoHold>, &ControlledBy)>,
    mut kd: ResMut<KdStore>,
) {
    let tick = timeline.tick();
    for (client_id, mut receiver) in &mut receivers {
        for _ in receiver.receive() {
            let Some((entity, _, position, cargo, controlled_by)) = ships
                .iter()
                .find(|(_, id, ..)| id.0 == client_id.0)
            else {
                continue;
            };
            info!("{:?} self-destructed", client_id.0);
            kd.death(client_id.0);
            scatter_cargo(
                &mut commands,
                position.0,
                cargo.map_or(0, |hold| hold.current),
                tick,
            );
            commands.entity(entity).try_despawn();
            commands.spawn(RespawnTask {
                client_id: client_id.0,
                link: controlled_by.owner,
                ticks_remaining: sim::RESPAWN_DELAY_TICKS,
                confirmed: false,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_ship(
    commands: &mut Commands,
    client_id: PeerId,
    team: Team,
    kind: HullKind,
    loadout: Loadout,
    pose: (Position, Rotation),
    bank: u32,
    link: Entity,
) {
    let entity = commands
        .spawn((
            sim::ship_bundle(client_id, team, kind, loadout, pose),
            Bank(bank),
            Points(0),
            UnlockedFittings::default(),
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
    info!("Spawned {kind:?} {entity:?} for client {client_id:?} on {team:?}");
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
    mut targets: Query<(
        Entity,
        &PlayerId,
        &Team,
        &Position,
        &ShipPoseHistory,
        Option<&HullKind>,
        Option<&CargoHold>,
        &mut Health,
    )>,
    delays: Query<&InterpolationDelay, With<ClientOf>>,
    mut points: ResMut<PointsStore>,
    mut kd: ResMut<KdStore>,
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

        // No friendly fire: bullets only connect with the other team. Hit
        // circles are per-hull (a harvester is a much fatter target than a
        // fighter).
        let hit_target = targets
            .iter()
            .find_map(|(entity, id, team, _, history, kind, _, _)| {
                if id.0 == marker.owner || *team == shooter_team {
                    return None;
                }
                let radius = hulls::stats(kind.copied().unwrap_or(HullKind::Fighter)).hit_radius;
                let center = history.sample(rewind_tick, overstep)?;
                segment_hits_circle(seg_start, seg_end, center, radius + sim::BULLET_SIZE)
                    .then_some(entity)
            });
        let Some(target) = hit_target else {
            continue;
        };

        commands.entity(bullet_entity).try_despawn();
        let Ok((_, target_id, _, target_pos, _, kind, cargo, mut health)) = targets.get_mut(target)
        else {
            continue;
        };
        let target_id = target_id.0;
        health.current = health.current.saturating_sub(marker.damage);
        points.award(marker.owner, sim::POINTS_PER_HIT);
        info!(
            "Hit: {:?} shot {target:?} (health now {}/{})",
            marker.owner, health.current, health.max
        );
        if health.current == 0 {
            info!("Kill: {:?} destroyed {:?}", marker.owner, target_id);
            let victim_hull = kind.copied().unwrap_or(HullKind::Fighter);
            points.award(marker.owner, hulls::kill_bounty(victim_hull));
            kd.kill(marker.owner);
            kd.death(target_id);
            // Undeposited ore scatters as scoopable fragments — recoverable
            // by the victim's team, or stolen by the killer's (DESIGN §3).
            scatter_cargo(
                &mut commands,
                target_pos.0,
                cargo.map_or(0, |hold| hold.current),
                tick,
            );
            commands.entity(target).try_despawn();
            // Only player ships respawn; linkless ships (target drones)
            // just die.
            if let Some(link) = shooters
                .iter()
                .find(|(id, _, _)| id.0 == target_id)
                .map(|(_, _, controlled_by)| controlled_by.owner)
            {
                commands.spawn(RespawnTask {
                    client_id: target_id,
                    link,
                    ticks_remaining: sim::RESPAWN_DELAY_TICKS,
                    confirmed: false,
                });
            }
        }
    }
}

fn respawn_ships(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RespawnTask)>,
    links: Query<(), With<ClientOf>>,
    carriers: Query<(Entity, &Team, &Position, &HullKind), With<PlayerId>>,
    motherships: Query<(Entity, &Team, &Position), With<Mothership>>,
    mut teams: ResMut<TeamAssignments>,
    mut banks: ResMut<Banks>,
    choices: Res<SpawnChoices>,
    unlocks: Res<Unlocks>,
) {
    for (entity, mut task) in &mut tasks {
        task.ticks_remaining -= 1;
        if task.ticks_remaining > 0 || !task.confirmed {
            continue;
        }
        commands.entity(entity).despawn();
        // Skip the respawn if the client disconnected while dead.
        if links.get(task.link).is_ok() {
            let team = teams.assign(task.client_id);
            let order = choices.0.get(&task.client_id).cloned().unwrap_or(SpawnOrder {
                hull: HullKind::Fighter,
                spawn_at: None,
                loadout: Loadout::default(),
            });
            let desired = order.hull;

            // Resolve the requested spawn facility, if it's still alive,
            // friendly, and eligible for the hull class (DESIGN §2/§6):
            // economy hulls spawn at the mothership or any friendly carrier,
            // combat hulls require a carrier, carrier-types build at the
            // mothership.
            let requested_carrier = order.spawn_at.and_then(|e| {
                carriers
                    .get(e)
                    .ok()
                    .filter(|(_, t, _, k)| **t == team && hulls::is_spawn_carrier(**k))
                    .map(|(_, _, pos, k)| (pos.0, *k))
            });
            let requested_mothership = order.spawn_at.and_then(|e| {
                motherships
                    .get(e)
                    .ok()
                    .filter(|(_, t, _)| **t == team)
                    .map(|(_, _, pos)| pos.0)
            });
            let any_carrier = carriers
                .iter()
                .find(|(_, t, _, k)| **t == team && hulls::is_spawn_carrier(**k))
                .map(|(_, _, pos, k)| (pos.0, *k));

            let any_fleet = carriers
                .iter()
                .find(|(_, t, _, k)| **t == team && **k == HullKind::FleetCarrier)
                .map(|(_, _, pos, k)| (pos.0, *k));
            let carrier_spawn = match hulls::class(desired) {
                hulls::HullClass::CarrierType => None,
                hulls::HullClass::SubCarrier => requested_carrier
                    .filter(|(_, k)| *k == HullKind::FleetCarrier)
                    .or(any_fleet),
                hulls::HullClass::Combat => requested_carrier.or(any_carrier),
                hulls::HullClass::Economy => {
                    // An explicit mothership request (or no request) means
                    // the mothership; otherwise honor the carrier choice.
                    if requested_mothership.is_some() {
                        None
                    } else {
                        requested_carrier
                    }
                }
            };
            let allowed = match hulls::class(desired) {
                hulls::HullClass::Combat | hulls::HullClass::SubCarrier => {
                    carrier_spawn.is_some()
                }
                _ => true,
            };

            // Buy the requested hull if allowed and the bank covers it;
            // hulls are lost on death, so every non-free spawn is a fresh
            // purchase (DESIGN §6).
            let bank = banks.0.entry(task.client_id).or_insert(0);
            let cost = hulls::stats(desired).cost;
            let kind = if allowed && *bank >= cost {
                *bank -= cost;
                desired
            } else {
                HullKind::Fighter
            };

            let pose = match (hulls::class(kind), carrier_spawn) {
                (hulls::HullClass::CarrierType, _) | (_, None) => {
                    sim::spawn_pose(task.client_id, team)
                }
                (_, Some((center, carrier_kind))) => sim::spawn_pose_at(
                    task.client_id,
                    team,
                    center,
                    hulls::stats(carrier_kind).width / 2.0 + 90.0,
                ),
            };
            let facility = match (hulls::class(kind), carrier_spawn) {
                (hulls::HullClass::CarrierType, _) | (_, None) => {
                    fittings::SpawnFacility::Mothership
                }
                (_, Some((_, HullKind::FleetCarrier))) => fittings::SpawnFacility::FleetCarrier,
                (_, Some(_)) => fittings::SpawnFacility::StrikeCarrier,
            };
            let loadout = validate_loadout(order.loadout, facility, task.client_id, &unlocks);
            info!("Respawning {:?} as {kind:?} with {loadout:?}", task.client_id);
            spawn_ship(
                &mut commands,
                task.client_id,
                team,
                kind,
                loadout,
                pose,
                *bank,
                task.link,
            );
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
