//! Client app construction: connects to the server, predicts its own ship
//! and bullets, and interpolates everyone else's.
//!
//! The windowed binary and the headless integration tests build the same app
//! through `build_client_app`; headless mode swaps DefaultPlugins for
//! MinimalPlugins and skips the rendering systems.

pub mod juice;
pub mod spawn_screen;

use avian2d::prelude::{Position, Rotation};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::winit::WinitSettings;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use homage_shared::protocol::*;
use homage_shared::{hulls, sim};
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
    /// Confirm respawns automatically instead of via the spawn screen
    /// (bots and headless test clients).
    pub auto_spawn: bool,
}

impl ClientConfig {
    pub fn headless(client_id: u64, server_addr: SocketAddr) -> Self {
        Self {
            client_id,
            server_addr,
            bot: false,
            headless: true,
            auto_spawn: true,
        }
    }
}

/// See ClientConfig::auto_spawn; a resource so tests can flip it.
#[derive(Resource)]
pub struct AutoSpawn(pub bool);

/// Last-known wealth and unlocks. Mirrored from ship components while alive
/// and from authoritative WealthUpdate messages (which also work while dead,
/// when the ship components don't exist).
#[derive(Resource, Default)]
pub struct WealthCache {
    pub bank: u32,
    pub points: u32,
    pub unlocked: std::collections::HashSet<FittingId>,
}

impl WealthCache {
    pub fn has_unlock(&self, fitting: FittingId) -> bool {
        homage_shared::fittings::def(fitting).cost == 0 || self.unlocked.contains(&fitting)
    }
}

/// The facility we're currently docked at, if any (from DockedNotice;
/// cleared when our ship exists again).
#[derive(Resource, Default)]
pub struct DockedAt(pub Option<Entity>);

fn receive_docked_notices(
    mut receivers: Query<&mut MessageReceiver<DockedNotice>, With<Client>>,
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut docked: ResMut<DockedAt>,
) {
    for mut receiver in &mut receivers {
        for notice in receiver.receive() {
            docked.0 = Some(notice.facility);
        }
    }
    if !alive.is_empty() {
        docked.0 = None;
    }
}

/// The last match result heard from the server (cleared implicitly when the
/// next match's gameplay resumes; the banner reads it).
#[derive(Resource, Default)]
pub struct LastMatchResult(pub Option<(Team, f32)>);

/// Consume match results. Runs in Update in both modes: receivers are
/// cleared every frame.
fn receive_match_results(
    time: Res<Time>,
    mut receivers: Query<&mut MessageReceiver<MatchResult>, With<Client>>,
    mut last: ResMut<LastMatchResult>,
) {
    for mut receiver in &mut receivers {
        for result in receiver.receive() {
            last.0 = Some((result.winner, time.elapsed_secs()));
        }
    }
}

/// Consume authoritative wealth snapshots (sent after unlock orders). Runs
/// in Update in both modes: receivers are cleared every frame.
fn receive_wealth_updates(
    mut receivers: Query<&mut MessageReceiver<WealthUpdate>, With<Client>>,
    mut cache: ResMut<WealthCache>,
) {
    for mut receiver in &mut receivers {
        for update in receiver.receive() {
            cache.bank = update.bank;
            cache.points = update.points;
            cache.unlocked = update.unlocked.iter().copied().collect();
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

/// Mouse state in world coordinates, tracked at render rate and sampled into
/// `ShipInput.aim` / `cursor_*` each tick. Gunship turrets and (later)
/// Captain ability targeting read these.
#[derive(Resource, Default)]
struct MouseWorld {
    cursor: Vec2,
    /// World-space angle from the local ship to the cursor.
    aim: f32,
}

/// Seconds Backspace has been held toward a self-destruct.
#[derive(Resource, Default)]
struct SelfDestructHold(f32);

const SELF_DESTRUCT_HOLD_SECS: f32 = 1.0;

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
    app.insert_resource(AutoSpawn(config.auto_spawn || config.bot));
    app.init_resource::<InputOverride>();
    app.init_resource::<TapBuffer>();
    app.init_resource::<MouseWorld>();

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
    app.init_resource::<WealthCache>();
    app.init_resource::<LastMatchResult>();
    app.init_resource::<DockedAt>();
    app.add_systems(
        Update,
        (
            auto_confirm_spawn,
            receive_wealth_updates,
            receive_match_results,
            receive_docked_notices,
        ),
    );
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
        app.add_plugins((juice::JuicePlugin, spawn_screen::SpawnScreenPlugin));
        app.add_systems(Startup, (setup_scene, setup_hud));
        app.init_resource::<SelfDestructHold>();
        app.init_resource::<DockHint>();
        app.add_systems(
            Update,
            (
                accumulate_taps,
                track_mouse,
                self_destruct,
                dock_hold,
                send_cheats,
                update_hud,
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                draw_grid,
                draw_ships,
                draw_turrets,
                draw_bullets,
                draw_motherships,
                draw_asteroids,
                draw_fragments,
            )
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
        // HOMAGE_HUD_DEBUG=1: log the HUD text's computed layout size, to
        // verify UI text actually renders (headless tests can't see it).
        if std::env::var("HOMAGE_HUD_DEBUG").is_ok() {
            app.add_systems(Update, log_hud_layout);
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
        (Entity, &HullKind),
        (
            With<PlayerId>,
            With<Predicted>,
            Without<avian2d::prelude::RigidBody>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, kind) in &ships {
        commands.entity(entity).try_insert(sim::ship_physics(*kind));
    }
}

fn add_structure_colliders(
    motherships: Query<
        Entity,
        (With<Mothership>, Without<avian2d::prelude::RigidBody>),
    >,
    asteroids: Query<
        (Entity, &Asteroid),
        Without<avian2d::prelude::RigidBody>,
    >,
    mut commands: Commands,
) {
    for entity in &motherships {
        commands.entity(entity).try_insert((
            avian2d::prelude::RigidBody::Static,
            avian2d::prelude::Collider::circle(sim::MOTHERSHIP_RADIUS),
        ));
    }
    for (entity, asteroid) in &asteroids {
        commands.entity(entity).try_insert((
            avian2d::prelude::RigidBody::Static,
            avian2d::prelude::Collider::circle(asteroid.radius * 0.9),
        ));
    }
}

fn setup_scene(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Marker for the ore/bank HUD line.
#[derive(Component)]
struct HudText;

fn setup_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new("Banked: 0"),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.85, 0.3)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}


fn log_hud_layout(
    mut ticks: Local<u32>,
    hud: Query<(&Text, &ComputedNode), With<HudText>>,
) {
    *ticks += 1;
    if *ticks % 60 != 0 {
        return;
    }
    match hud.single() {
        Ok((text, node)) => info!("HUD size={:?} text={:?}", node.size, text.0),
        Err(e) => info!("HUD query failed: {e}"),
    }
}

/// Bank and hold of the local ship, top-left. While earning, the banked
/// number grows a "(+N)" accumulator that fades a couple of seconds after
/// the earning pauses.
fn update_hud(
    time: Res<Time>,
    recent: Res<juice::RecentEarnings>,
    recent_points: Res<juice::RecentPoints>,
    destruct: Res<SelfDestructHold>,
    dock_hint: Res<DockHint>,
    ship: Query<
        (Option<&Bank>, Option<&Points>, Option<&CargoHold>),
        (With<Predicted>, With<InputMarker<Inputs>>),
    >,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let Ok((bank, points, cargo)) = ship.single() else {
        // Dead: the spawn screen owns the display; a frozen HUD would keep
        // showing stale warnings (e.g. the self-destruct countdown).
        text.0 = String::new();
        return;
    };
    let bank = bank.map_or(0, |b| b.0);
    let points = points.map_or(0, |p| p.0);
    let (held, capacity) = cargo.map_or((0, 0), |c| (c.current, c.capacity));
    let earned = if recent.visible(time.elapsed_secs()) {
        format!(" (+{})", recent.amount)
    } else {
        String::new()
    };
    let points_earned = if recent_points.visible(time.elapsed_secs()) {
        format!(" (+{})", recent_points.amount)
    } else {
        String::new()
    };
    let warning = if destruct.0 > 0.0 {
        format!(
            "   !! SELF-DESTRUCT IN {:.1} !!",
            (SELF_DESTRUCT_HOLD_SECS - destruct.0).max(0.0)
        )
    } else {
        String::new()
    };
    let dock = dock_hint.0.map(|h| format!("   {h}")).unwrap_or_default();
    text.0 = format!(
        "Banked: {bank}{earned}   Pts: {points}{points_earned}   Hold: {held}/{capacity}{dock}{warning}"
    );
}

/// Bots and headless clients skip the spawn screen: while dead, confirm the
/// respawn about once a second (idempotent server-side).
fn auto_confirm_spawn(
    time: Res<Time>,
    mut last_sent: Local<f32>,
    auto: Res<AutoSpawn>,
    docked: Res<DockedAt>,
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut sender: Query<&mut MessageSender<SpawnConfirm>, With<Client>>,
) {
    if !auto.0 || !alive.is_empty() || docked.0.is_some() {
        return;
    }
    if time.elapsed_secs() - *last_sent < 1.0 {
        return;
    }
    *last_sent = time.elapsed_secs();
    if let Ok(mut sender) = sender.single_mut() {
        sender.send::<OrdersChannel>(SpawnConfirm);
    }
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
fn accumulate_taps(
    mut taps: ResMut<TapBuffer>,
    keypress: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if keypress.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left) {
        taps.fire = true;
    }
}

/// Hold Backspace to scuttle: the only way to change hulls without dying in
/// combat. The hold delay prevents fat-fingered fleet losses; the HUD shows
/// the countdown while held.
fn self_destruct(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut hold: ResMut<SelfDestructHold>,
    mut sender: Query<&mut MessageSender<SelfDestruct>, With<Client>>,
) {
    // HOMAGE_AUTO_SCUTTLE=<secs>: act as if Backspace is held from that many
    // seconds in (drives the full path without a keyboard).
    let auto = std::env::var("HOMAGE_AUTO_SCUTTLE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok().or(Some(6.0)))
        .is_some_and(|delay| time.elapsed_secs() > delay);
    if alive.is_empty() || !(keys.pressed(KeyCode::Backspace) || auto) {
        hold.0 = 0.0;
        return;
    }
    let before = hold.0;
    hold.0 += time.delta_secs();
    if before < SELF_DESTRUCT_HOLD_SECS && hold.0 >= SELF_DESTRUCT_HOLD_SECS {
        match sender.single_mut() {
            Ok(mut sender) => {
                sender.send::<OrdersChannel>(SelfDestruct);
                info!("SelfDestruct sent");
            }
            Err(e) => warn!("SelfDestruct not sent, no sender: {e}"),
        }
    }
}

/// Hold E near a friendly facility to dock for refits (DESIGN §6). The
/// server validates range; this also surfaces a HUD hint via DockHint.
#[derive(Resource, Default)]
struct DockHint(Option<&'static str>);

#[allow(clippy::type_complexity)]
fn dock_hold(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut hold: Local<f32>,
    mut hint: ResMut<DockHint>,
    ship: Query<(&Position, &Team), (With<Predicted>, With<InputMarker<Inputs>>)>,
    motherships: Query<(&Position, &Team), With<Mothership>>,
    facilities: Query<(&Position, &Team, &HullKind), With<PlayerId>>,
    mut sender: Query<&mut MessageSender<DockRequest>, With<Client>>,
) {
    hint.0 = None;
    let Ok((position, team)) = ship.single() else {
        *hold = 0.0;
        return;
    };
    let near_mothership = motherships
        .iter()
        .any(|(pos, t)| t == team && pos.0.distance(position.0) < sim::dock_radius(None));
    let near_facility = facilities.iter().any(|(pos, t, kind)| {
        t == team
            && hulls::is_dockable(*kind)
            && pos.0.distance(position.0) < sim::dock_radius(Some(*kind))
    });
    if !near_mothership && !near_facility {
        *hold = 0.0;
        return;
    }
    hint.0 = Some("[hold E] dock");
    if !keys.pressed(KeyCode::KeyE) {
        *hold = 0.0;
        return;
    }
    let before = *hold;
    *hold += time.delta_secs();
    if before < 0.5 && *hold >= 0.5 {
        if let Ok(mut sender) = sender.single_mut() {
            sender.send::<OrdersChannel>(DockRequest);
        }
    }
}

/// Dev cheats on F-keys (manual testing; see CheatOrder). Position-taking
/// cheats use the mouse cursor's world position.
fn send_cheats(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<MouseWorld>,
    mut sender: Query<&mut MessageSender<CheatOrder>, With<Client>>,
) {
    let cheat = if keys.just_pressed(KeyCode::F1) {
        Some(CheatOrder::GiveOre(50))
    } else if keys.just_pressed(KeyCode::F2) {
        Some(CheatOrder::SpawnAsteroid(mouse.cursor))
    } else if keys.just_pressed(KeyCode::F3) {
        Some(CheatOrder::SpawnFragments(mouse.cursor))
    } else if keys.just_pressed(KeyCode::F4) {
        Some(CheatOrder::SpawnTargetDrone(mouse.cursor))
    } else if keys.just_pressed(KeyCode::F5) {
        Some(CheatOrder::Teleport(mouse.cursor))
    } else if keys.just_pressed(KeyCode::F6) {
        Some(CheatOrder::Heal)
    } else if keys.just_pressed(KeyCode::F7) {
        Some(CheatOrder::GivePoints(50))
    } else {
        None
    };
    if let Some(cheat) = cheat {
        if let Ok(mut sender) = sender.single_mut() {
            info!("cheat: {cheat:?}");
            sender.send::<OrdersChannel>(cheat);
        }
    }
}

/// Track the mouse in world space at render rate: cursor position and the
/// aim angle from the local ship toward it.
fn track_mouse(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    ship: Query<&Position, (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut mouse: ResMut<MouseWorld>,
) {
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), camera.single())
    else {
        return;
    };
    let Some(screen) = window.cursor_position() else {
        return;
    };
    let Ok(world) = camera.viewport_to_world_2d(camera_transform, screen) else {
        return;
    };
    mouse.cursor = world;
    if let Ok(position) = ship.single() {
        let to_cursor = world - position.0;
        if to_cursor.length_squared() > 1.0 {
            mouse.aim = to_cursor.to_angle();
        }
    }
}

/// Sample the input source once per tick into the lightyear input buffer.
/// Priority: scripted override (tests) > bot mode > keyboard. In headless
/// mode there is no `ButtonInput` resource, so the keyboard branch is skipped.
fn buffer_input(
    mut query: Query<&mut ActionState<Inputs>, With<InputMarker<Inputs>>>,
    keypress: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    mouse: Res<MouseWorld>,
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
    if keypress.pressed(KeyCode::ShiftLeft) || keypress.pressed(KeyCode::ShiftRight) {
        input.ability = true;
    }
    let mouse_held = mouse_buttons.is_some_and(|m| m.pressed(MouseButton::Left));
    if keypress.pressed(KeyCode::Space) || mouse_held || taps.fire {
        input.fire = true;
    }
    taps.fire = false;
    // Aim and cursor ride along on every input; hulls that don't use them
    // ignore them (superset protocol, DESIGN §4.3).
    input.set_aim_radians(mouse.aim);
    input.set_cursor_world(mouse.cursor);
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
            Option<&HullKind>,
            Option<&Health>,
            Option<&CargoHold>,
            Option<&juice::FlashUntil>,
        ),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    for (position, rotation, color, kind, health, cargo, flash) in &ships {
        let stats = hulls::stats(kind.copied().unwrap_or(HullKind::Fighter));
        let (length, width) = (stats.length, stats.width);
        let flashing = flash.is_some_and(|f| time.elapsed_secs() < f.0);
        let draw_color = if flashing { Color::WHITE } else { color.0 };
        let pos = position.0;
        if stats.archetype == hulls::Archetype::Captain {
            // Captain hulls are round: facing is meaningless, presence is
            // the point.
            gizmos.circle_2d(Isometry2d::from_translation(pos), width / 2.0, draw_color);
            gizmos.circle_2d(
                Isometry2d::from_translation(pos),
                width * 0.28,
                draw_color.with_alpha(0.5),
            );
            if flashing {
                gizmos.circle_2d(
                    Isometry2d::from_translation(pos),
                    width / 2.0 * 1.2,
                    Color::WHITE.with_alpha(0.6),
                );
            }
        } else {
            let nose = pos + *rotation * Vec2::new(length / 2.0, 0.0);
            let left = pos + *rotation * Vec2::new(-length / 2.0, width / 2.0);
            let right = pos + *rotation * Vec2::new(-length / 2.0, -width / 2.0);
            gizmos.linestrip_2d([nose, left, right, nose], draw_color);
            if flashing {
                // Second, slightly larger outline so the flash pops.
                let grow = 1.35;
                let nose = pos + (nose - pos) * grow;
                let left = pos + (left - pos) * grow;
                let right = pos + (right - pos) * grow;
                gizmos.linestrip_2d([nose, left, right, nose], Color::WHITE.with_alpha(0.6));
            }
        }

        if let Some(health) = health {
            let fraction = health.current as f32 / health.max as f32;
            let half_width = length / 2.0;
            let y = length / 2.0 + 8.0;
            gizmos.line_2d(
                pos + Vec2::new(-half_width, y),
                pos + Vec2::new(-half_width + length * fraction, y),
                Color::srgb(0.2, 1.0, 0.2),
            );
        }
        // Ore aboard: an amber bar under the health bar.
        if let Some(cargo) = cargo {
            if cargo.current > 0 {
                let fraction = cargo.load_fraction();
                let half_width = length / 2.0;
                let y = length / 2.0 + 4.0;
                gizmos.line_2d(
                    pos + Vec2::new(-half_width, y),
                    pos + Vec2::new(-half_width + length * fraction, y),
                    Color::srgb(1.0, 0.85, 0.3),
                );
            }
        }
    }
}

/// Gunship turrets: a hub and barrel drawn over the hull along the aim. The
/// local ship reads its own live input (zero-latency, guidepost 1); remote
/// ships read the replicated TurretAim.
fn draw_turrets(
    mut gizmos: Gizmos,
    ships: Query<
        (
            &Position,
            &PlayerColor,
            &HullKind,
            Option<&TurretAim>,
            Option<&ActionState<Inputs>>,
        ),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    for (position, color, kind, turret, action) in &ships {
        let stats = hulls::stats(*kind);
        if stats.archetype != hulls::Archetype::Gunship {
            continue;
        }
        let aim = action
            .map(|a| a.0 .0.aim_radians())
            .or(turret.map(|t| t.0));
        let Some(aim) = aim else {
            continue;
        };
        let dir = Vec2::from_angle(aim);
        let hub = stats.width * 0.28;
        let base = position.0;
        gizmos.circle_2d(Isometry2d::from_translation(base), hub, color.0);
        gizmos.line_2d(base + dir * hub, base + dir * (hub + 14.0), color.0);
    }
}

/// Bullets: our own are predicted (PreSpawned before the server confirms),
/// everyone else's are interpolated.
fn draw_bullets(
    mut gizmos: Gizmos,
    bullets: Query<
        (&Position, &PlayerColor, &BulletMarker),
        Or<(With<Predicted>, With<Interpolated>, With<PreSpawned>)>,
    >,
) {
    for (position, color, marker) in &bullets {
        // Torpedoes read as ordnance, not tracer fire.
        let radius = if marker.damage > 1 {
            sim::BULLET_SIZE * 3.0
        } else {
            sim::BULLET_SIZE
        };
        gizmos.circle_2d(Isometry2d::from_translation(position.0), radius, color.0);
    }
}

/// Each team's mothership: a big team-colored hull ring with an inner core,
/// plus a health bar once it becomes damageable.
fn draw_motherships(
    mut gizmos: Gizmos,
    time: Res<Time>,
    motherships: Query<(&Position, &Team, Option<&Health>), With<Mothership>>,
) {
    for (position, team, health) in &motherships {
        let color = team_color(*team);
        let pos = position.0;
        gizmos.circle_2d(Isometry2d::from_translation(pos), sim::MOTHERSHIP_RADIUS, color);
        gizmos.circle_2d(
            Isometry2d::from_translation(pos),
            sim::MOTHERSHIP_RADIUS * 0.55,
            color.with_alpha(0.6),
        );
        if let Some(health) = health {
            let fraction = health.current as f32 / health.max.max(1) as f32;
            let width = sim::MOTHERSHIP_RADIUS * 1.6;
            let y = sim::MOTHERSHIP_RADIUS + 16.0;
            gizmos.line_2d(
                pos + Vec2::new(-width / 2.0, y),
                pos + Vec2::new(-width / 2.0 + width * fraction, y),
                Color::srgb(0.2, 1.0, 0.2),
            );
        }
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

/// Rocks as irregular polygons, silhouette stable via the replicated seed;
/// they darken as they take damage.
fn draw_asteroids(
    mut gizmos: Gizmos,
    asteroids: Query<(&Position, &Asteroid, Option<&Health>)>,
) {
    for (position, asteroid, health) in &asteroids {
        let damage = health.map_or(1.0, |h| h.current as f32 / h.max.max(1) as f32);
        let shade = 0.35 + 0.25 * damage;
        let color = Color::srgb(shade, shade * 0.95, shade * 0.85);
        let n = 9;
        let points: Vec<Vec2> = (0..=n)
            .map(|i| {
                let k = i % n;
                // Cheap per-vertex hash off the replicated seed.
                let h = (asteroid.seed as u32)
                    .wrapping_mul(k as u32 + 13)
                    .wrapping_mul(2654435761);
                let wobble = 0.72 + 0.28 * ((h >> 16) & 0xff) as f32 / 255.0;
                let angle = k as f32 / n as f32 * core::f32::consts::TAU;
                position.0 + Vec2::from_angle(angle) * asteroid.radius * wobble
            })
            .collect();
        gizmos.linestrip_2d(points, color);
    }
}

/// Ore fragments: small pulsing diamonds you want to fly through.
fn draw_fragments(
    mut gizmos: Gizmos,
    time: Res<Time>,
    fragments: Query<&Position, (With<OreFragment>, With<Interpolated>)>,
) {
    let pulse = 3.5 + (time.elapsed_secs() * 6.0).sin() * 0.8;
    let color = Color::srgb(1.0, 0.85, 0.3);
    for position in &fragments {
        let p = position.0;
        gizmos.linestrip_2d(
            [
                p + Vec2::new(0.0, pulse),
                p + Vec2::new(pulse, 0.0),
                p + Vec2::new(0.0, -pulse),
                p + Vec2::new(-pulse, 0.0),
                p + Vec2::new(0.0, pulse),
            ],
            color,
        );
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
