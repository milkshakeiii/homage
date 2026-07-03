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
use homage_server::build_server_app;
use homage_shared::protocol::*;
use homage_shared::FIXED_TIMESTEP_HZ;
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
