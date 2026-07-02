//! Windowed client: connects to the server, predicts its own ship and
//! bullets, and interpolates everyone else's.
//!
//! Run with `cargo run -p homage_client -- <client_id>`. The id must be unique
//! per connected client; it defaults to the process id so that launching
//! several clients without arguments still works. Pass `bot` as a second
//! argument for a self-driving client (constant thrust + turn + fire).

use avian2d::prelude::{Position, Rotation};
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::sim;
use homage_shared::SharedPlugin;
use homage_shared::{FIXED_TIMESTEP_HZ, PRIVATE_KEY, PROTOCOL_ID, SERVER_ADDR};
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::input::*;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;

/// When true, the client ignores the keyboard and constantly thrusts, turns,
/// and fires — a self-driving client for testing without a human.
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
    app.add_plugins(SharedPlugin);
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
    app.add_systems(FixedUpdate, log_ships);
    app.add_systems(
        Update,
        (draw_grid, draw_ships, draw_bullets, camera_follow),
    );
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
        input.fire = true;
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
    if keypress.pressed(KeyCode::Space) {
        input.fire = true;
    }
    action_state.0 = Inputs(input);
}

/// Periodic snapshot of the visual entities, for verifying replication
/// without eyes on the window.
fn log_ships(
    mut ticks: Local<u32>,
    ships: Query<
        (Entity, &Position, &Health, Has<Predicted>, Has<Interpolated>),
        With<PlayerId>,
    >,
    bullets: Query<(), With<BulletMarker>>,
) {
    *ticks += 1;
    if *ticks % 320 != 0 {
        return;
    }
    info!("{} bullet entities", bullets.iter().count());
    for (entity, position, health, predicted, interpolated) in &ships {
        let kind = if predicted {
            "predicted"
        } else if interpolated {
            "interpolated"
        } else {
            "confirmed"
        };
        info!(
            "{kind} ship {entity:?} pos ({:.1}, {:.1}) hp {}/{}",
            position.0.x, position.0.y, health.current, health.max
        );
    }
}

/// Draw each ship as a triangle plus a health bar. Predicted (our ship) and
/// Interpolated (everyone else) entities are the visual ones; the raw
/// Confirmed copies have no visual.
fn draw_ships(
    mut gizmos: Gizmos,
    ships: Query<
        (&Position, &Rotation, &PlayerColor, Option<&Health>),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    for (position, rotation, color, health) in &ships {
        let pos = position.0;
        let nose = pos + *rotation * Vec2::new(sim::SHIP_LENGTH / 2.0, 0.0);
        let left = pos + *rotation * Vec2::new(-sim::SHIP_LENGTH / 2.0, sim::SHIP_WIDTH / 2.0);
        let right = pos + *rotation * Vec2::new(-sim::SHIP_LENGTH / 2.0, -sim::SHIP_WIDTH / 2.0);
        gizmos.linestrip_2d([nose, left, right, nose], color.0);

        if let Some(health) = health {
            let fraction = health.current as f32 / health.max as f32;
            let half_width = sim::SHIP_LENGTH / 2.0;
            let y = sim::SHIP_LENGTH / 2.0 + 8.0;
            gizmos.line_2d(
                pos + Vec2::new(-half_width, y),
                pos + Vec2::new(-half_width + sim::SHIP_LENGTH * fraction, y),
                Color::srgb(0.2, 1.0, 0.2),
            );
        }
    }
}

/// Bullets: our own are predicted (PreSpawned before the server confirms),
/// everyone else's are interpolated.
fn draw_bullets(
    mut gizmos: Gizmos,
    bullets: Query<
        (&Position, &PlayerColor),
        (
            With<BulletMarker>,
            Or<(With<Predicted>, With<Interpolated>, With<PreSpawned>)>,
        ),
    >,
) {
    for (position, color) in &bullets {
        gizmos.circle_2d(
            Isometry2d::from_translation(position.0),
            sim::BULLET_SIZE,
            color.0,
        );
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
    ship: Query<&Position, (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let (Ok(position), Ok(mut transform)) = (ship.single(), camera.single_mut()) else {
        return;
    };
    transform.translation.x = position.0.x;
    transform.translation.y = position.0.y;
}
