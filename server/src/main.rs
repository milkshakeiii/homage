//! Dedicated headless server: accepts connections, spawns one ship per
//! client, simulates ship movement from replicated inputs, and replicates the
//! world back to all clients.

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use core::f32::consts::TAU;
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::ship;
use homage_shared::{FIXED_TIMESTEP_HZ, PRIVATE_KEY, PROTOCOL_ID, SEND_INTERVAL, SERVER_ADDR};
use lightyear::connection::client::Connected;
use lightyear::netcode::server_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::input::native::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

fn main() {
    let mut app = App::new();
    // Throttle the headless main loop; the 64Hz fixed timestep accumulates
    // real time, so simulation speed is unaffected.
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 256.0,
        ))),
        LogPlugin::default(),
        StatesPlugin,
    ));
    app.add_plugins(ServerPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
    });
    app.add_plugins(ProtocolPlugin);
    app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
    app.add_systems(Startup, start_server);
    app.add_systems(FixedUpdate, (movement, log_ships).chain());
    app.add_observer(handle_new_client);
    app.add_observer(handle_connected);
    app.run();
}

fn start_server(mut commands: Commands) {
    let server = commands
        .spawn((
            NetcodeServer::new(NetcodeConfig {
                protocol_id: PROTOCOL_ID,
                private_key: PRIVATE_KEY,
                ..Default::default()
            }),
            LocalAddr(SERVER_ADDR),
            ServerUdpIo::default(),
            Name::from("Server"),
        ))
        .id();
    commands.trigger(Start { entity: server });
    info!("Server listening on {SERVER_ADDR}");
}

/// A new link entity is created when a client starts connecting; give it a
/// `ReplicationSender` so we can replicate entities to that client.
fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender, Name::from("ClientLink")));
}

/// Once a client is confirmed as connected, spawn its ship.
fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    let Ok(client_id) = query.get(trigger.entity) else {
        return;
    };
    let client_id = client_id.0;

    // Spread spawn points around a ring so players don't stack.
    let angle = (client_id.to_bits() % 16) as f32 / 16.0 * TAU;
    let position = Vec2::from_angle(angle) * 200.0;

    let entity = commands
        .spawn((
            ShipBundle::new(client_id, position),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
            ControlledBy {
                owner: trigger.entity,
                lifetime: Default::default(),
            },
        ))
        .id();
    info!("Spawned ship {entity:?} for client {client_id:?}");
}

/// Periodic snapshot of ship state, for observability while the game has no
/// other diagnostics.
fn log_ships(
    mut ticks: Local<u32>,
    query: Query<(Entity, &PlayerId, &ShipPosition, &ShipVelocity)>,
) {
    *ticks += 1;
    if *ticks % 320 != 0 {
        return;
    }
    for (entity, id, position, velocity) in &query {
        info!(
            "ship {entity:?} owner {:?} pos ({:.1}, {:.1}) vel ({:.1}, {:.1})",
            id.0, position.0.x, position.0.y, velocity.0.x, velocity.0.y
        );
    }
}

/// Advance every ship using the latest input received from its owner.
fn movement(
    mut query: Query<(
        &mut ShipPosition,
        &mut ShipHeading,
        &mut ShipVelocity,
        &ActionState<Inputs>,
    )>,
) {
    for (position, heading, velocity, action_state) in &mut query {
        let Inputs(input) = &action_state.0;
        ship::apply_ship_input(position, heading, velocity, input);
    }
}
