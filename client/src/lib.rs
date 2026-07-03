//! Client app construction: connects to the server, predicts its own ship
//! and bullets, and interpolates everyone else's.
//!
//! The windowed binary and the headless integration tests build the same app
//! through `build_client_app`; headless mode swaps DefaultPlugins for
//! MinimalPlugins and skips the rendering systems.

pub mod juice;

use avian2d::prelude::{Position, Rotation};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::winit::WinitSettings;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::sim;
use homage_shared::SharedPlugin;
use homage_shared::{FIXED_TIMESTEP_HZ, PRIVATE_KEY, PROTOCOL_ID};
use lightyear::frame_interpolation::prelude::*;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::input::*;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;

pub struct ClientConfig {
    pub client_id: u64,
    pub server_addr: SocketAddr,
    /// Ignore the keyboard and constantly thrust, turn, and fire.
    pub bot: bool,
    /// No window, no rendering; driven by manual `App::update()` calls.
    pub headless: bool,
}

impl ClientConfig {
    pub fn headless(client_id: u64, server_addr: SocketAddr) -> Self {
        Self {
            client_id,
            server_addr,
            bot: false,
            headless: true,
        }
    }
}

/// When true, the client ignores the keyboard and constantly thrusts, turns,
/// and fires — a self-driving client for testing without a human.
#[derive(Resource)]
struct BotMode(bool);

/// Scripted input for tests: when `Some`, it wins over both the bot and the
/// keyboard. Mutate this resource from a test to steer the ship.
#[derive(Resource, Default)]
pub struct InputOverride(pub Option<ShipInput>);

/// Presses caught at render rate so a tap shorter than a simulation tick
/// still registers (feel guidepost: no input is ever eaten). Set by
/// `accumulate_taps` in `Update`, consumed by `buffer_input` on the next
/// fixed tick.
#[derive(Resource, Default)]
struct TapBuffer {
    fire: bool,
}

pub fn build_client_app(config: ClientConfig) -> App {
    let mut app = App::new();
    if config.headless {
        app.add_plugins((MinimalPlugins, StatesPlugin, TransformPlugin));
    } else {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("homage - client {}", config.client_id),
                resolution: (1024, 768).into(),
                ..default()
            }),
            ..default()
        }));
        // Keep updating at full rate when the window loses focus, so running
        // two clients side by side stays smooth.
        app.insert_resource(WinitSettings::continuous());
    }
    app.add_plugins(ClientPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ),
    });
    app.add_plugins(SharedPlugin);
    app.insert_resource(BotMode(config.bot));
    app.init_resource::<InputOverride>();
    app.init_resource::<TapBuffer>();

    let auth = Authentication::Manual {
        server_addr: config.server_addr,
        client_id: config.client_id,
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
        PeerAddr(config.server_addr),
        PredictionManager::default(),
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
        NetcodeClient::new(auth, netcode_config).expect("failed to build netcode client"),
        UdpIo::default(),
        Name::from("Client"),
    ));

    app.add_systems(Startup, connect);
    app.add_systems(
        FixedPreUpdate,
        buffer_input.in_set(InputSystems::WriteClientInputs),
    );
    app.add_systems(FixedUpdate, log_ships);
    app.add_observer(handle_controlled_spawn);

    // The simulation steps at 64Hz but the display renders faster, so
    // predicted entities (which only move on fixed ticks) would judder.
    // Frame interpolation writes a between-ticks visual value into
    // Position/Rotation during PostUpdate; every draw/camera system runs
    // after it (see JuicePlugin) so gizmos see the smooth values.
    // Enabled in headless mode too so the e2e harness exercises it.
    app.add_plugins((
        FrameInterpolationPlugin::<Position>::default(),
        FrameInterpolationPlugin::<Rotation>::default(),
    ));
    // lightyear 0.28 bug workaround: FrameInterpolationPlugin puts its
    // history-capture system in FixedPostUpdate but configures its set's
    // ordering in FixedLast, leaving the capture UNORDERED against avian's
    // position integration (also FixedPostUpdate). If it captures before
    // physics runs, the captured "current" value is the pre-physics one that
    // Restore just wrote, so previous == current forever: interpolation
    // becomes a no-op AND Restore keeps stomping the physics result — the
    // predicted ship only visibly moves on server-forced rollbacks (~20Hz
    // stutter, input response delayed by a round trip). Ordering the capture
    // after the physics step fixes both.
    app.configure_sets(
        FixedPostUpdate,
        lightyear::frame_interpolation::FrameInterpolationSystems::Update
            .after(avian2d::prelude::PhysicsSystems::StepSimulation),
    );
    app.add_observer(add_frame_interpolation::<Predicted>);
    app.add_observer(add_frame_interpolation::<PreSpawned>);
    // Physics components aren't replicated, so the predicted ship arrives
    // without a RigidBody — and avian silently ignores it: no local
    // integration, prediction degrades to 20Hz snap-to-server. Give every
    // predicted ship its physics locally. (Remote ships stay non-physical:
    // interpolation owns their pose. Ship-ship contact prediction is a
    // known gap for later.)
    app.add_systems(Update, add_predicted_ship_physics);
    // Static structures get client-side colliders so the predicted ship
    // bounces off them locally instead of waiting for a server correction.
    app.add_systems(Update, add_structure_colliders);

    if !config.headless {
        app.add_plugins(juice::JuicePlugin);
        app.add_systems(Startup, setup_scene);
        app.add_systems(Update, accumulate_taps);
        app.add_systems(
            PostUpdate,
            (draw_grid, draw_ships, draw_bullets, draw_motherships)
                .after(FrameInterpolationSystems::Interpolate),
        );
        // HOMAGE_MOTION_DEBUG=1: log the per-frame movement of the ship as
        // the renderer sees it, to quantify visual stutter (a healthy smooth
        // ship never shows delta=0 frames while under thrust).
        if std::env::var("HOMAGE_MOTION_DEBUG").is_ok() {
            app.add_systems(
                PostUpdate,
                log_motion.after(FrameInterpolationSystems::Interpolate),
            );
        }
    }
    app
}

fn log_motion(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    mut last: Local<Option<Vec2>>,
    ship: Query<
        (&Position, &avian2d::prelude::LinearVelocity),
        (With<Predicted>, With<InputMarker<Inputs>>),
    >,
) {
    let Ok((position, velocity)) = ship.single() else {
        return;
    };
    if let Some(prev) = *last {
        info!(
            "MOTION dt={:.5} delta={:.4} overstep={:.3} pos=({:.2},{:.2}) vel=({:.1},{:.1})",
            time.delta_secs(),
            position.0.distance(prev),
            fixed.overstep_fraction(),
            position.0.x,
            position.0.y,
            velocity.0.x,
            velocity.0.y,
        );
    }
    *last = Some(position.0);
}

/// Locally-simulated entities (our predicted ship, prespawned bullets) step
/// at 64Hz; mark their pose for between-ticks visual interpolation.
fn add_frame_interpolation<M: Component>(trigger: On<Add, M>, mut commands: Commands) {
    commands.entity(trigger.entity).try_insert((
        FrameInterpolate::<Position>::default(),
        FrameInterpolate::<Rotation>::default(),
    ));
}

fn add_predicted_ship_physics(
    ships: Query<
        Entity,
        (
            With<PlayerId>,
            With<Predicted>,
            Without<avian2d::prelude::RigidBody>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &ships {
        commands.entity(entity).try_insert(sim::ship_physics());
    }
}

fn add_structure_colliders(
    structures: Query<
        Entity,
        (With<Mothership>, Without<avian2d::prelude::RigidBody>),
    >,
    mut commands: Commands,
) {
    for entity in &structures {
        commands.entity(entity).try_insert((
            avian2d::prelude::RigidBody::Static,
            avian2d::prelude::Collider::circle(sim::MOTHERSHIP_RADIUS),
        ));
    }
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

/// Catch `just_pressed` edges at render rate (there can be many frames per
/// simulation tick) so quick taps survive until the next fixed tick samples
/// them.
fn accumulate_taps(mut taps: ResMut<TapBuffer>, keypress: Res<ButtonInput<KeyCode>>) {
    if keypress.just_pressed(KeyCode::Space) {
        taps.fire = true;
    }
}

/// Sample the input source once per tick into the lightyear input buffer.
/// Priority: scripted override (tests) > bot mode > keyboard. In headless
/// mode there is no `ButtonInput` resource, so the keyboard branch is skipped.
fn buffer_input(
    mut query: Query<&mut ActionState<Inputs>, With<InputMarker<Inputs>>>,
    keypress: Option<Res<ButtonInput<KeyCode>>>,
    bot: Res<BotMode>,
    scripted: Res<InputOverride>,
    mut taps: ResMut<TapBuffer>,
) {
    let Ok(mut action_state) = query.single_mut() else {
        return;
    };
    if let Some(input) = &scripted.0 {
        action_state.0 = Inputs(input.clone());
        return;
    }
    let mut input = ShipInput::default();
    if bot.0 {
        input.thrust = true;
        input.turn_left = true;
        input.fire = true;
        action_state.0 = Inputs(input);
        return;
    }
    let Some(keypress) = keypress else {
        action_state.0 = Inputs(input);
        return;
    };
    if keypress.pressed(KeyCode::KeyW) || keypress.pressed(KeyCode::ArrowUp) {
        input.thrust = true;
    }
    if keypress.pressed(KeyCode::KeyS) || keypress.pressed(KeyCode::ArrowDown) {
        input.brake = true;
    }
    if keypress.pressed(KeyCode::KeyA) || keypress.pressed(KeyCode::ArrowLeft) {
        input.turn_left = true;
    }
    if keypress.pressed(KeyCode::KeyD) || keypress.pressed(KeyCode::ArrowRight) {
        input.turn_right = true;
    }
    if keypress.pressed(KeyCode::Space) || taps.fire {
        input.fire = true;
    }
    taps.fire = false;
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
/// Confirmed copies have no visual. Ships flash white while a recent hit's
/// `FlashUntil` is active.
fn draw_ships(
    mut gizmos: Gizmos,
    time: Res<Time>,
    ships: Query<
        (
            &Position,
            &Rotation,
            &PlayerColor,
            Option<&Health>,
            Option<&juice::FlashUntil>,
        ),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    for (position, rotation, color, health, flash) in &ships {
        let flashing = flash.is_some_and(|f| time.elapsed_secs() < f.0);
        let draw_color = if flashing { Color::WHITE } else { color.0 };
        let pos = position.0;
        let nose = pos + *rotation * Vec2::new(sim::SHIP_LENGTH / 2.0, 0.0);
        let left = pos + *rotation * Vec2::new(-sim::SHIP_LENGTH / 2.0, sim::SHIP_WIDTH / 2.0);
        let right = pos + *rotation * Vec2::new(-sim::SHIP_LENGTH / 2.0, -sim::SHIP_WIDTH / 2.0);
        gizmos.linestrip_2d([nose, left, right, nose], draw_color);
        if flashing {
            // Second, slightly larger outline so the flash pops at a glance.
            let grow = 1.35;
            let nose = pos + (nose - pos) * grow;
            let left = pos + (left - pos) * grow;
            let right = pos + (right - pos) * grow;
            gizmos.linestrip_2d([nose, left, right, nose], Color::WHITE.with_alpha(0.6));
        }

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

/// Each team's mothership: a big team-colored hull ring with an inner core,
/// plus a health bar once it becomes damageable.
fn draw_motherships(
    mut gizmos: Gizmos,
    time: Res<Time>,
    motherships: Query<(&Position, &Team), With<Mothership>>,
) {
    for (position, team) in &motherships {
        let color = team_color(*team);
        let pos = position.0;
        gizmos.circle_2d(Isometry2d::from_translation(pos), sim::MOTHERSHIP_RADIUS, color);
        gizmos.circle_2d(
            Isometry2d::from_translation(pos),
            sim::MOTHERSHIP_RADIUS * 0.55,
            color.with_alpha(0.6),
        );
        // Slowly rotating docking spokes, so the structure reads as alive.
        let spin = time.elapsed_secs() * 0.2;
        for i in 0..3 {
            let angle = spin + i as f32 * core::f32::consts::TAU / 3.0;
            let dir = Vec2::from_angle(angle);
            gizmos.line_2d(
                pos + dir * sim::MOTHERSHIP_RADIUS * 0.55,
                pos + dir * sim::MOTHERSHIP_RADIUS,
                color.with_alpha(0.4),
            );
        }
    }
}

fn team_color(team: Team) -> Color {
    match team {
        Team::Blue => Color::srgb(0.35, 0.55, 1.0),
        Team::Red => Color::srgb(1.0, 0.35, 0.35),
    }
}

/// A faint grid over the play area so motion is visible against empty space,
/// plus the soft-boundary rectangle.
fn draw_grid(mut gizmos: Gizmos) {
    let color = Color::srgba(1.0, 1.0, 1.0, 0.08);
    let step = 400.0;
    let (w, h) = (sim::MAP_HALF_WIDTH, sim::MAP_HALF_HEIGHT);
    let nx = (w / step) as i32;
    let ny = (h / step) as i32;
    for i in -nx..=nx {
        let x = i as f32 * step;
        gizmos.line_2d(Vec2::new(x, -h), Vec2::new(x, h), color);
    }
    for i in -ny..=ny {
        let y = i as f32 * step;
        gizmos.line_2d(Vec2::new(-w, y), Vec2::new(w, y), color);
    }
    let boundary = Color::srgba(1.0, 0.4, 0.3, 0.35);
    gizmos.rect_2d(Isometry2d::IDENTITY, Vec2::new(w * 2.0, h * 2.0), boundary);
}
