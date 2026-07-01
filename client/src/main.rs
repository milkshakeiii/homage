//! Windowed client: connects to the server, predicts its own ship, and
//! interpolates everyone else's.
//!
//! Run with `cargo run -p homage_client -- <client_id>`. The id must be unique
//! per connected client; it defaults to the process id so that launching
//! several clients without arguments still works.

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::ship;
use homage_shared::{FIXED_TIMESTEP_HZ, PRIVATE_KEY, PROTOCOL_ID, SERVER_ADDR};
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::input::*;
use lightyear::prelude::client::{InputDelayConfig, InputTimelineConfig};
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;

/// When true, the client ignores the keyboard and constantly thrusts while
/// turning — a self-driving client for testing replication without a human.
#[derive(Resource)]
struct BotMode(bool);

fn main() {
    let client_id: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or_else(|| std::process::id() as u64);
    let bot = std::env::args().nth(2).is_some_and(|arg| arg == "bot");

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("homage - client {client_id}"),
            resolution: (1024, 768).into(),
            ..default()
        }),
        ..default()
    }));
    // Keep updating at full rate when the window loses focus, so running two
    // clients side by side stays smooth.
    app.insert_resource(WinitSettings::continuous());
    app.add_plugins(ClientPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
    });
    app.add_plugins(ProtocolPlugin);
    app.insert_resource(BotMode(bot));

    let auth = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id,
        private_key: PRIVATE_KEY,
        protocol_id: PROTOCOL_ID,
    };
    let netcode_config = NetcodeConfig {
        client_timeout_secs: 3,
        token_expire_secs: -1,
        ..Default::default()
    };
    app.world_mut().spawn((
        Client::default(),
        Link::new(None),
        LocalAddr(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)),
        PeerAddr(SERVER_ADDR),
        PredictionManager::default(),
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
        NetcodeClient::new(auth, netcode_config).expect("failed to build netcode client"),
        UdpIo::default(),
        Name::from("Client"),
    ));

    app.add_systems(Startup, (setup_scene, connect));
    app.add_systems(
        FixedPreUpdate,
        buffer_input.in_set(InputSystems::WriteClientInputs),
    );
    app.add_systems(FixedUpdate, (predicted_movement, log_ships).chain());
    app.add_systems(Update, (draw_grid, draw_ships, camera_follow));
    app.add_observer(handle_controlled_spawn);
    app.run();
}

fn setup_scene(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn connect(mut commands: Commands, client: Single<Entity, With<Client>>) {
    commands.trigger(Connect {
        entity: client.into_inner(),
    });
}

/// When the server gives us control of an entity, attach an `InputMarker` so
/// our buffered inputs drive it.
fn handle_controlled_spawn(
    trigger: On<Add, Controlled>,
    players: Query<(), (With<PlayerId>, Without<InputMarker<Inputs>>)>,
    mut commands: Commands,
) {
    if players.get(trigger.entity).is_ok() {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<Inputs>::default());
    }
}

/// Sample the keyboard once per tick into the lightyear input buffer.
fn buffer_input(
    mut query: Query<&mut ActionState<Inputs>, With<InputMarker<Inputs>>>,
    keypress: Res<ButtonInput<KeyCode>>,
    bot: Res<BotMode>,
) {
    let Ok(mut action_state) = query.single_mut() else {
        return;
    };
    let mut input = ShipInput::default();
    if bot.0 {
        input.thrust = true;
        input.turn_left = true;
        action_state.0 = Inputs(input);
        return;
    }
    if keypress.pressed(KeyCode::KeyW) || keypress.pressed(KeyCode::ArrowUp) {
        input.thrust = true;
    }
    if keypress.pressed(KeyCode::KeyA) || keypress.pressed(KeyCode::ArrowLeft) {
        input.turn_left = true;
    }
    if keypress.pressed(KeyCode::KeyD) || keypress.pressed(KeyCode::ArrowRight) {
        input.turn_right = true;
    }
    action_state.0 = Inputs(input);
}

/// Apply inputs to our predicted ship. The same `apply_ship_input` runs on the
/// server; lightyear rolls back and re-simulates on mispredictions.
fn predicted_movement(
    synced_client: Query<(), (With<Client>, With<IsSynced<InputTimeline>>)>,
    mut query: Query<
        (
            &mut ShipPosition,
            &mut ShipHeading,
            &mut ShipVelocity,
            &ActionState<Inputs>,
        ),
        With<Predicted>,
    >,
) {
    if synced_client.is_empty() {
        return;
    }
    for (position, heading, velocity, action_state) in &mut query {
        let Inputs(input) = &action_state.0;
        ship::apply_ship_input(position, heading, velocity, input);
    }
}

/// Periodic snapshot of the visual (predicted/interpolated) ships, for
/// verifying replication without eyes on the window.
fn log_ships(
    mut ticks: Local<u32>,
    query: Query<
        (Entity, &ShipPosition, Has<Predicted>, Has<Interpolated>),
        Or<(With<Predicted>, With<Interpolated>)>,
    >,
) {
    *ticks += 1;
    if *ticks % 320 != 0 {
        return;
    }
    for (entity, position, predicted, interpolated) in &query {
        let kind = if predicted {
            "predicted"
        } else if interpolated {
            "interpolated"
        } else {
            "?"
        };
        info!(
            "{kind} ship {entity:?} pos ({:.1}, {:.1})",
            position.0.x, position.0.y
        );
    }
}

/// Draw each ship as a triangle. Predicted (our ship) and Interpolated
/// (everyone else) entities are the visual ones; the raw Confirmed copies
/// have no visual.
fn draw_ships(
    mut gizmos: Gizmos,
    ships: Query<
        (&ShipPosition, &ShipHeading, &PlayerColor),
        Or<(With<Predicted>, With<Interpolated>)>,
    >,
) {
    for (position, heading, color) in &ships {
        let nose = position.0 + Vec2::from_angle(heading.0) * 16.0;
        let left = position.0 + Vec2::from_angle(heading.0 + 2.5) * 12.0;
        let right = position.0 + Vec2::from_angle(heading.0 - 2.5) * 12.0;
        gizmos.linestrip_2d([nose, left, right, nose], color.0);
    }
}

/// A faint grid so motion is visible against empty space.
fn draw_grid(mut gizmos: Gizmos) {
    let color = Color::srgba(1.0, 1.0, 1.0, 0.08);
    let extent = 2000.0;
    let step = 200.0;
    let n = (extent / step) as i32;
    for i in -n..=n {
        let offset = i as f32 * step;
        gizmos.line_2d(
            Vec2::new(offset, -extent),
            Vec2::new(offset, extent),
            color,
        );
        gizmos.line_2d(
            Vec2::new(-extent, offset),
            Vec2::new(extent, offset),
            color,
        );
    }
}

/// Keep the camera centered on our predicted ship.
fn camera_follow(
    ship: Query<&ShipPosition, (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let (Ok(position), Ok(mut transform)) = (ship.single(), camera.single_mut()) else {
        return;
    };
    transform.translation.x = position.0.x;
    transform.translation.y = position.0.y;
}
