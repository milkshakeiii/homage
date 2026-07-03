//! Code shared between the homage client and server: the network protocol,
//! the avian physics setup, and the ship/bullet simulation that must run
//! identically on both sides for prediction to work.

pub mod hulls;
pub mod protocol;
pub mod sim;

use avian2d::physics_transform::PhysicsTransformConfig;
use avian2d::prelude::*;
use bevy::prelude::*;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::time::Duration;
use lightyear::avian2d::plugin::{AvianReplicationMode, LightyearAvianPlugin};

pub const FIXED_TIMESTEP_HZ: f64 = 64.0;
pub const SERVER_PORT: u16 = 5888;
// Bind/connect on loopback for now; switch to 0.0.0.0 when hosting for LAN/WAN.
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), SERVER_PORT);

/// How often the server sends replication updates to clients.
pub const SEND_INTERVAL: Duration = Duration::from_millis(50);

pub const PROTOCOL_ID: u64 = 0x484f_4d41_4745; // "HOMAGE"
pub const PRIVATE_KEY: [u8; 32] = [0; 32];

/// Protocol + physics + shared simulation systems, added by both binaries.
#[derive(Clone)]
pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(protocol::ProtocolPlugin);

        // Physics. Both sides need it: the server is authoritative, the client
        // simulates predicted entities (and rolls back on mispredictions).
        app.insert_resource(PhysicsTransformConfig {
            transform_to_position: false,
            ..default()
        });
        app.add_plugins(LightyearAvianPlugin {
            replication_mode: AvianReplicationMode::Position,
            ..default()
        });
        app.add_plugins(
            PhysicsPlugins::default()
                .build()
                // position<->transform syncs are handled by lightyear_avian
                .disable::<PhysicsTransformPlugin>()
                .disable::<PhysicsInterpolationPlugin>()
                // island *sleeping* is not compatible with rollbacks; the
                // IslandPlugin itself must stay: avian 0.7's solver integrates
                // dynamic bodies per-island, and without it their velocities
                // are never applied.
                .disable::<IslandSleepingPlugin>(),
        );
        app.insert_resource(Gravity(Vec2::ZERO));

        app.add_systems(
            FixedUpdate,
            (
                sim::player_movement,
                sim::update_turrets,
                sim::soft_boundary,
                sim::clamp_ship_speed,
                sim::shared_player_firing,
                sim::lifetime_despawner,
            )
                .chain(),
        );
    }
}
