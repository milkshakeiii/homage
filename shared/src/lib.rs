//! Code shared between the homage client and server: the network protocol,
//! the avian physics setup, and the ship/bullet simulation that must run
//! identically on both sides for prediction to work.

pub mod fittings;
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

/// Bump this whenever the wire format changes (protocol.rs: components,
/// messages, channels, input struct). It feeds PROTOCOL_ID, so a stale
/// server and a new client refuse to connect outright instead of silently
/// dropping the messages one side doesn't know (which reads as "the feature
/// doesn't work" in playtests).
pub const PROTOCOL_VERSION: u64 = 10;
pub const PROTOCOL_ID: u64 = 0x484f_4d41_4745 ^ (PROTOCOL_VERSION << 48); // "HOMAGE" + version
pub const PRIVATE_KEY: [u8; 32] = [0; 32];

/// Log the net-id tables both sides use on the wire. Separately-built
/// binaries MUST agree on these; when they don't, messages silently decode
/// as the wrong type (debugging aid for protocol drift).
pub fn log_protocol_tables(
    messages: bevy::ecs::system::Res<lightyear::prelude::MessageRegistry>,
    channels: bevy::ecs::system::Res<lightyear::prelude::ChannelRegistry>,
) {
    for net_id in 0..32u16 {
        if let Some(kind) = messages.kind_map.kind(net_id) {
            let name = messages.kind_map.name(kind).unwrap_or("?");
            bevy::log::info!("PROTOCOL message net_id {net_id} = {name}");
        }
    }
    let channel_map = channels.kind_map();
    for net_id in 0..32u16 {
        if let Some(kind) = channel_map.kind(net_id) {
            let name = channel_map.name(kind).unwrap_or("?");
            bevy::log::info!("PROTOCOL channel net_id {net_id} = {name}");
        }
    }
}

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

        app.add_systems(Startup, log_protocol_tables);
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
