//! End-to-end test harness: one real server App plus N real client Apps in a
//! single process, connected over UDP on loopback, with time stepped manually
//! so a tick of simulation takes a microsecond of wall clock instead of 15ms.
//!
//! Each test must use a unique port (tests in one binary run in parallel).

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::time::Duration;
use homage_client::{build_client_app, ClientConfig, InputOverride};
use homage_server::{build_server_app, AsteroidFieldConfig};
use homage_shared::protocol::*;
use homage_shared::{sim, FIXED_TIMESTEP_HZ};
use lightyear::prelude::*;

/// One simulation tick of virtual time.
pub const TICK: Duration = Duration::from_micros((1_000_000.0 / FIXED_TIMESTEP_HZ) as u64);

pub struct TestNet {
    pub server: App,
    pub clients: Vec<App>,
    /// Netcode client ids, parallel to `clients`.
    pub client_ids: Vec<u64>,
}

impl TestNet {
    /// Start a server on `port` and connect one headless client per id.
    /// Set `TEST_LOG=1` to get server logs while debugging a test.
    pub fn new(port: u16, client_ids: &[u64]) -> Self {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let mut server = build_server_app(addr);
        // No random asteroid field in tests: they teleport ships around and
        // need empty, predictable space. Economy tests spawn precise rocks
        // via `spawn_asteroid` / `spawn_fragment`.
        server.insert_resource(AsteroidFieldConfig {
            enabled: false,
            seed: 0,
        });
        if std::env::var("TEST_LOG").is_ok() {
            server.add_plugins(bevy::log::LogPlugin::default());
        }
        prepare(&mut server);

        let clients = client_ids
            .iter()
            .map(|&id| {
                let mut app = build_client_app(ClientConfig::headless(id, addr));
                prepare(&mut app);
                app
            })
            .collect();

        Self {
            server,
            clients,
            client_ids: client_ids.to_vec(),
        }
    }

    /// Advance every app by one simulation tick.
    pub fn tick(&mut self) {
        self.server.update();
        for client in &mut self.clients {
            client.update();
        }
    }

    pub fn run_ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Tick until `condition` holds, up to `max_ticks`. Returns whether the
    /// condition was ever met.
    #[must_use]
    pub fn run_until(
        &mut self,
        max_ticks: usize,
        mut condition: impl FnMut(&mut TestNet) -> bool,
    ) -> bool {
        for _ in 0..max_ticks {
            self.tick();
            if condition(self) {
                return true;
            }
        }
        false
    }

    /// Script the given client's input (`None` returns control to bot/keys).
    pub fn set_input(&mut self, client_idx: usize, input: Option<ShipInput>) {
        self.clients[client_idx]
            .world_mut()
            .resource_mut::<InputOverride>()
            .0 = input;
    }

    /// Server-side view of all ships: (owner id, position, health).
    pub fn server_ships(&mut self) -> Vec<(PeerId, Vec2, u16)> {
        self.server
            .world_mut()
            .query::<(&PlayerId, &Position, &Health)>()
            .iter(self.server.world())
            .map(|(id, pos, health)| (id.0, pos.0, health.current))
            .collect()
    }

    /// The server-side ship of one client, if alive.
    pub fn server_ship(&mut self, client_id: u64) -> Option<(Vec2, u16)> {
        self.server_ships()
            .into_iter()
            .find(|(id, _, _)| *id == PeerId::Netcode(client_id))
            .map(|(_, pos, health)| (pos, health))
    }

    /// The server-side velocity of one client's ship, if alive.
    pub fn server_ship_velocity(&mut self, client_id: u64) -> Option<Vec2> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &LinearVelocity)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, vel)| vel.0)
    }

    /// Server-side count of bullets in flight.
    pub fn server_bullet_count(&mut self) -> usize {
        let world = self.server.world_mut();
        let mut query = world.query_filtered::<(), With<BulletMarker>>();
        query.iter(world).count()
    }

    /// Spawn a precisely-placed asteroid on the server (tests disable the
    /// random field).
    pub fn spawn_asteroid(&mut self, position: Vec2, radius: f32) {
        self.server.world_mut().spawn((
            sim::asteroid_bundle(position, radius, 7),
            Replicate::to_clients(NetworkTarget::All),
        ));
    }

    /// Spawn a stationary ore fragment on the server.
    pub fn spawn_fragment(&mut self, position: Vec2) {
        let tick = self
            .server
            .world()
            .resource::<lightyear::prelude::LocalTimeline>()
            .tick();
        self.server.world_mut().spawn((
            sim::fragment_bundle(position, Vec2::ZERO, tick),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ));
    }

    /// Server-side count of asteroids / ore fragments.
    pub fn server_asteroid_count(&mut self) -> usize {
        let world = self.server.world_mut();
        let mut query = world.query_filtered::<(), With<Asteroid>>();
        query.iter(world).count()
    }

    pub fn server_fragment_count(&mut self) -> usize {
        let world = self.server.world_mut();
        let mut query = world.query_filtered::<(), With<OreFragment>>();
        query.iter(world).count()
    }

    /// A client's view of the fragment count (confirmed copies).
    pub fn client_fragment_count(&mut self, client_idx: usize) -> usize {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query_filtered::<(), With<OreFragment>>();
        query.iter(world).count()
    }

    /// Server-side cargo of a client's ship: (current, capacity).
    pub fn server_ship_cargo(&mut self, client_id: u64) -> Option<(u16, u16)> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &CargoHold)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, hold)| (hold.current, hold.capacity))
    }

    /// Overwrite a ship's cargo server-side (for handling tests).
    pub fn set_ship_cargo(&mut self, client_id: u64, current: u16) {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &mut CargoHold)>();
        for (id, mut hold) in query.iter_mut(world) {
            if id.0 == PeerId::Netcode(client_id) {
                hold.current = current.min(hold.capacity);
            }
        }
    }

    /// Send a spawn order (next-hull choice) from a client, as the respawn
    /// menu would.
    pub fn client_send_spawn_order(&mut self, client_idx: usize, hull: HullKind) {
        self.client_send_spawn_order_at(client_idx, hull, None);
    }

    /// Send a spawn order with an explicit facility choice. `spawn_at` must
    /// be an entity in the *client's* world (a confirmed replicated copy).
    pub fn client_send_spawn_order_at(
        &mut self,
        client_idx: usize,
        hull: HullKind,
        spawn_at: Option<Entity>,
    ) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<SpawnOrder>, With<Client>>();
        let mut sent = 0;
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull,
                spawn_at,
                loadout: Loadout::default(),
            });
            sent += 1;
        }
        assert!(sent > 0, "no MessageSender<SpawnOrder> on the client entity");
    }

    /// Send a full spawn order including a fittings loadout.
    pub fn client_send_spawn_order_loadout(
        &mut self,
        client_idx: usize,
        hull: HullKind,
        spawn_at: Option<Entity>,
        loadout: Loadout,
    ) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<SpawnOrder>, With<Client>>();
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull,
                spawn_at,
                loadout,
            });
        }
    }

    /// Spend points on a fitting unlock, as the module tile would.
    pub fn client_send_unlock(&mut self, client_idx: usize, fitting: FittingId) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<UnlockOrder>, With<Client>>();
        let mut sent = 0;
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(UnlockOrder { fitting });
            sent += 1;
        }
        assert!(sent > 0, "no MessageSender<UnlockOrder> on the client entity");
    }

    /// A team's mothership health server-side: (current, max).
    pub fn server_mothership_health(&mut self, team: Team) -> Option<(u16, u16)> {
        let world = self.server.world_mut();
        let mut query = world.query_filtered::<(&Team, &Health), With<Mothership>>();
        query
            .iter(world)
            .find(|(t, _)| **t == team)
            .map(|(_, health)| (health.current, health.max))
    }

    /// Set a mothership's health (test shortcut for the win condition).
    pub fn set_mothership_health(&mut self, team: Team, current: u16) {
        let world = self.server.world_mut();
        let mut query = world.query_filtered::<(&Team, &mut Health), With<Mothership>>();
        for (t, mut health) in query.iter_mut(world) {
            if *t == team {
                health.current = current;
            }
        }
    }

    /// The winner a client last heard about, if any.
    pub fn client_last_match_result(&mut self, client_idx: usize) -> Option<Team> {
        self.clients[client_idx]
            .world()
            .resource::<homage_client::LastMatchResult>()
            .0
            .map(|(winner, _)| winner)
    }

    /// Roster entries as replicated to a client: (player id, team, kills,
    /// deaths, points), sorted by player id.
    pub fn client_roster(&mut self, client_idx: usize) -> Vec<(u64, Team, u32, u32, u32)> {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query::<(&RosterEntry, &Team, &Kills, &Deaths, &Points)>();
        let mut entries: Vec<_> = query
            .iter(world)
            .map(|(entry, team, kills, deaths, points)| {
                (entry.0.to_bits(), *team, kills.0, deaths.0, points.0)
            })
            .collect();
        entries.sort_by_key(|(id, ..)| *id);
        entries
    }

    /// The client's last-known wealth cache (fed by ship components while
    /// alive and WealthUpdate messages while dead).
    pub fn client_wealth(&mut self, client_idx: usize) -> (u32, u32, Vec<FittingId>) {
        let cache = self.clients[client_idx]
            .world()
            .resource::<homage_client::WealthCache>();
        let mut unlocked: Vec<FittingId> = cache.unlocked.iter().copied().collect();
        unlocked.sort();
        (cache.bank, cache.points, unlocked)
    }

    /// What a client's server-side ship actually spawned with.
    pub fn server_ship_equipped(&mut self, client_id: u64) -> Option<Loadout> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &Equipped)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, equipped)| equipped.0)
    }

    /// Max health of a client's server-side ship.
    pub fn server_ship_max_health(&mut self, client_id: u64) -> Option<u16> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &Health)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, health)| health.max)
    }

    /// Toggle a client's automatic spawn confirmation (headless clients
    /// default to true so kill/respawn flows behave like the old
    /// auto-respawn).
    pub fn set_auto_spawn(&mut self, client_idx: usize, enabled: bool) {
        self.clients[client_idx]
            .world_mut()
            .resource_mut::<homage_client::AutoSpawn>()
            .0 = enabled;
    }

    /// Send an explicit spawn confirmation (the map click).
    pub fn client_send_spawn_confirm(&mut self, client_idx: usize) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<SpawnConfirm>, With<Client>>();
        let mut sent = 0;
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(SpawnConfirm);
            sent += 1;
        }
        assert!(sent > 0, "no MessageSender<SpawnConfirm> on the client entity");
    }

    /// Send a dev cheat from a client, as the F-keys would.
    pub fn client_send_cheat(&mut self, client_idx: usize, cheat: CheatOrder) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<CheatOrder>, With<Client>>();
        let mut sent = 0;
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(cheat.clone());
            sent += 1;
        }
        assert!(sent > 0, "no MessageSender<CheatOrder> on the client entity");
    }

    /// The client's replicated entity for another player's ship (lightyear
    /// 0.28 uses a single-entity model: the interpolated entity IS the
    /// replicated one the server's mapper understands).
    pub fn client_find_ship(&mut self, client_idx: usize, owner_id: u64) -> Option<Entity> {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query::<(Entity, &PlayerId)>();
        query
            .iter(world)
            .find(|(_, id)| id.0 == PeerId::Netcode(owner_id))
            .map(|(entity, _)| entity)
    }

    /// A client's view of a mothership entity by team.
    pub fn client_find_mothership(&mut self, client_idx: usize, team: Team) -> Option<Entity> {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<(Entity, &Team), With<Mothership>>();
        query
            .iter(world)
            .find(|(_, t)| **t == team)
            .map(|(entity, _)| entity)
    }

    /// Send a self-destruct order from a client, as holding Backspace would.
    pub fn client_send_self_destruct(&mut self, client_idx: usize) {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&mut MessageSender<SelfDestruct>, With<Client>>();
        let mut sent = 0;
        for mut sender in query.iter_mut(world) {
            sender.send::<OrdersChannel>(SelfDestruct);
            sent += 1;
        }
        assert!(sent > 0, "no MessageSender<SelfDestruct> on the client entity");
    }

    /// Which hull a client's server-side ship currently is.
    pub fn server_ship_hull(&mut self, client_id: u64) -> Option<HullKind> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &HullKind)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, kind)| *kind)
    }

    /// Overwrite a player's bank server-side.
    pub fn set_bank(&mut self, client_id: u64, amount: u32) {
        self.server
            .world_mut()
            .resource_mut::<homage_server::Banks>()
            .0
            .insert(PeerId::Netcode(client_id), amount);
    }

    /// Server-authoritative points for a client.
    pub fn server_points(&mut self, client_id: u64) -> u32 {
        self.server
            .world()
            .resource::<homage_server::PointsStore>()
            .0
            .get(&PeerId::Netcode(client_id))
            .copied()
            .unwrap_or(0)
    }

    /// The points value replicated onto a client's own ship.
    pub fn client_points(&mut self, client_idx: usize) -> Option<u32> {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query_filtered::<&Points, With<Predicted>>();
        query.iter(world).next().map(|points| points.0)
    }

    /// Server-authoritative bank balance for a client.
    pub fn server_bank(&mut self, client_id: u64) -> u32 {
        self.server
            .world()
            .resource::<homage_server::Banks>()
            .0
            .get(&PeerId::Netcode(client_id))
            .copied()
            .unwrap_or(0)
    }

    /// The bank value replicated onto a client's own ship.
    pub fn client_bank(&mut self, client_idx: usize) -> Option<u32> {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query_filtered::<&Bank, With<Predicted>>();
        query.iter(world).next().map(|bank| bank.0)
    }

    /// Which team the server put a client's ship on.
    pub fn server_ship_team(&mut self, client_id: u64) -> Option<Team> {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &Team)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, team)| *team)
    }

    /// Motherships visible to a client: (team, position).
    pub fn client_motherships(&mut self, client_idx: usize) -> Vec<(Team, Vec2)> {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query_filtered::<(&Team, &Position), With<Mothership>>();
        query.iter(world).map(|(team, pos)| (*team, pos.0)).collect()
    }

    /// The server's current view of a client's input (for diagnosing input
    /// transmission in tests).
    pub fn server_input(&mut self, client_id: u64) -> Option<ShipInput> {
        use lightyear::prelude::input::native::ActionState;
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &ActionState<Inputs>)>();
        query
            .iter(world)
            .find(|(id, _)| id.0 == PeerId::Netcode(client_id))
            .map(|(_, action)| action.0 .0.clone())
    }

    /// Point a client's server-side ship in a direction without touching its
    /// velocity (unlike `teleport`).
    pub fn set_rotation(&mut self, client_id: u64, angle: f32) {
        let world = self.server.world_mut();
        let mut query = world.query::<(&PlayerId, &mut Rotation)>();
        for (id, mut rot) in query.iter_mut(world) {
            if id.0 == PeerId::Netcode(client_id) {
                *rot = Rotation::radians(angle);
            }
        }
    }

    /// Teleport a client's server-side ship and zero its velocity. Lag
    /// compensation samples position history, so wait ~35 ticks after this
    /// before relying on hits at the new location.
    pub fn teleport(&mut self, client_id: u64, position: Vec2, angle: f32) {
        let world = self.server.world_mut();
        let mut query = world.query::<(
            &PlayerId,
            &mut Position,
            &mut Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
        )>();
        for (id, mut pos, mut rot, mut linvel, mut angvel) in query.iter_mut(world) {
            if id.0 == PeerId::Netcode(client_id) {
                pos.0 = position;
                *rot = Rotation::radians(angle);
                linvel.0 = Vec2::ZERO;
                angvel.0 = 0.0;
            }
        }
    }

    /// Count of a client's visual ship entities by kind:
    /// (predicted, interpolated).
    pub fn client_ship_kinds(&mut self, client_idx: usize) -> (usize, usize) {
        let world = self.clients[client_idx].world_mut();
        let mut query = world.query_filtered::<(Has<Predicted>, Has<Interpolated>), With<PlayerId>>();
        let mut predicted = 0;
        let mut interpolated = 0;
        for (is_predicted, is_interpolated) in query.iter(world) {
            if is_predicted {
                predicted += 1;
            }
            if is_interpolated {
                interpolated += 1;
            }
        }
        (predicted, interpolated)
    }

    /// Position of a client's own (predicted) ship.
    pub fn predicted_ship_pos(&mut self, client_idx: usize) -> Option<Vec2> {
        let world = self.clients[client_idx].world_mut();
        let mut query =
            world.query_filtered::<&Position, (With<PlayerId>, With<Predicted>)>();
        query.iter(world).next().map(|p| p.0)
    }
}

/// Finish plugin setup so the app can be driven by manual `update()` calls,
/// and pin virtual time to exactly one tick per update.
fn prepare(app: &mut App) {
    let plugins = app.plugins_state();
    if plugins != bevy::app::PluginsState::Cleaned {
        while app.plugins_state() == bevy::app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(TICK));
}
