//! Game-feel systems (DESIGN §4.2): camera look-ahead + speed zoom, capped
//! screenshake, motion trails, own-ship thruster flare, hit flashes, kill
//! rings, and placeholder synthesized SFX. Everything here is cosmetic,
//! client-only, and driven off replicated/predicted state — it must never
//! touch the simulation.

use avian2d::prelude::{LinearVelocity, Position, Rotation};
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use homage_shared::protocol::*;
use homage_shared::sim;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;
use std::collections::{HashMap, VecDeque};

const CAMERA_LEAD_SECS: f32 = 0.35;
const CAMERA_LEAD_MAX: f32 = 160.0;
/// 1/s; higher = stiffer camera.
const CAMERA_STIFFNESS: f32 = 5.0;
const ZOOM_STIFFNESS: f32 = 2.5;
/// Extra zoom-out at max speed (fraction of base scale).
const ZOOM_AT_MAX_SPEED: f32 = 0.30;

const SHAKE_MAX_OFFSET: f32 = 9.0;
const SHAKE_DECAY_PER_SEC: f32 = 1.4;

const TRAIL_MAX_AGE: f32 = 0.45;
const TRAIL_MIN_SPEED: f32 = 60.0;

const FLASH_SECS: f32 = 0.12;
const KILL_RING_SECS: f32 = 0.5;
const KILL_RING_RADIUS: f32 = 60.0;

/// How far away hits/kills still shake the camera / make noise.
const FEEDBACK_RADIUS: f32 = 900.0;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenShake>();
        app.init_resource::<CameraRig>();
        app.init_resource::<ShipPosCache>();
        app.init_resource::<KillRings>();
        app.add_systems(Startup, setup_sfx);
        app.add_systems(
            Update,
            (
                (detect_kills, cache_ship_positions).chain(),
                detect_damage,
                detect_own_fire,
                ensure_trails,
                update_trails,
                draw_trails,
                draw_thrust_flare,
                draw_kill_rings,
                decay_shake,
                camera_follow,
            ),
        );
    }
}

// Screenshake

/// Trauma-based shake (Vlambeer-school): effects add trauma, offset scales
/// with trauma², and the offset is hard-capped so even a barrage never
/// disorients.
#[derive(Resource, Default)]
pub struct ScreenShake {
    trauma: f32,
}

impl ScreenShake {
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    fn offset(&self, t: f32) -> Vec2 {
        let s = self.trauma * self.trauma;
        // Cheap smooth pseudo-noise from incommensurate sines.
        let x = ((t * 47.0).sin() + (t * 13.7).sin()) * 0.5;
        let y = ((t * 59.0).cos() + (t * 17.3).sin()) * 0.5;
        Vec2::new(x, y) * SHAKE_MAX_OFFSET * s
    }
}

fn decay_shake(time: Res<Time>, mut shake: ResMut<ScreenShake>) {
    shake.trauma = (shake.trauma - SHAKE_DECAY_PER_SEC * time.delta_secs()).max(0.0);
}

/// 1 at distance 0, 0 at `FEEDBACK_RADIUS`, quadratic falloff.
fn falloff(distance: f32) -> f32 {
    let x = 1.0 - (distance / FEEDBACK_RADIUS).min(1.0);
    x * x
}

// Camera

/// Source-of-truth camera state; the Transform is rig + shake, so shake never
/// feeds back into the smoothing.
#[derive(Resource)]
struct CameraRig {
    pos: Vec2,
    zoom: f32,
    initialized: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            zoom: 1.0,
            initialized: false,
        }
    }
}

/// Follow the predicted ship with velocity look-ahead and speed zoom: the
/// camera shows where you're *going*, and going fast widens the view.
fn camera_follow(
    time: Res<Time>,
    shake: Res<ScreenShake>,
    mut rig: ResMut<CameraRig>,
    ship: Query<(&Position, &LinearVelocity), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let (Ok((position, velocity)), Ok((mut transform, mut projection))) =
        (ship.single(), camera.single_mut())
    else {
        return;
    };
    let lead = (velocity.0 * CAMERA_LEAD_SECS).clamp_length_max(CAMERA_LEAD_MAX);
    let target = position.0 + lead;
    if !rig.initialized {
        rig.pos = target;
        rig.initialized = true;
    }
    let dt = time.delta_secs();
    let alpha = 1.0 - (-CAMERA_STIFFNESS * dt).exp();
    rig.pos = rig.pos.lerp(target, alpha);

    let speed_frac = (velocity.0.length() / sim::MAX_SPEED).clamp(0.0, 1.2);
    let zoom_target = 1.0 + ZOOM_AT_MAX_SPEED * speed_frac;
    let zoom_alpha = 1.0 - (-ZOOM_STIFFNESS * dt).exp();
    rig.zoom += (zoom_target - rig.zoom) * zoom_alpha;

    let with_shake = rig.pos + shake.offset(time.elapsed_secs());
    transform.translation.x = with_shake.x;
    transform.translation.y = with_shake.y;
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = rig.zoom;
    }
}

// Trails & thrust flare

/// Recent tail positions of a ship, for motion streaks.
#[derive(Component, Default)]
struct Trail {
    points: VecDeque<(Vec2, f32)>,
}

fn ensure_trails(
    mut commands: Commands,
    ships: Query<
        Entity,
        (
            With<PlayerId>,
            Or<(With<Predicted>, With<Interpolated>)>,
            Without<Trail>,
        ),
    >,
) {
    for entity in &ships {
        commands.entity(entity).try_insert(Trail::default());
    }
}

/// Emit trail points while a ship is moving fast enough for streaks to read
/// as motion (speed-keyed rather than thrust-keyed: remote ships don't
/// replicate their inputs, and speed is honest data).
fn update_trails(
    time: Res<Time>,
    mut ships: Query<(&Position, &Rotation, &LinearVelocity, &mut Trail)>,
) {
    let now = time.elapsed_secs();
    for (position, rotation, velocity, mut trail) in &mut ships {
        if velocity.0.length() > TRAIL_MIN_SPEED {
            let tail = position.0 - (*rotation * Vec2::X) * (sim::SHIP_LENGTH * 0.5);
            trail.points.push_back((tail, now));
        }
        while trail
            .points
            .front()
            .is_some_and(|(_, t)| now - *t > TRAIL_MAX_AGE)
        {
            trail.points.pop_front();
        }
    }
}

fn draw_trails(mut gizmos: Gizmos, time: Res<Time>, ships: Query<(&Trail, &PlayerColor)>) {
    let now = time.elapsed_secs();
    for (trail, color) in &ships {
        for pair in trail.points.iter().collect::<Vec<_>>().windows(2) {
            let (a, ta) = *pair[0];
            let (b, _) = *pair[1];
            let age = now - ta;
            let alpha = (1.0 - age / TRAIL_MAX_AGE).max(0.0) * 0.5;
            gizmos.line_2d(a, b, color.0.with_alpha(alpha));
        }
    }
}

/// Flickering exhaust triangle on the local ship while thrusting — instant
/// cosmetic response to the player's own input (feel guidepost 1).
fn draw_thrust_flare(
    mut gizmos: Gizmos,
    time: Res<Time>,
    ship: Query<
        (&Position, &Rotation, &ActionState<Inputs>),
        (With<Predicted>, With<InputMarker<Inputs>>),
    >,
) {
    let Ok((position, rotation, action)) = ship.single() else {
        return;
    };
    if !action.0 .0.thrust {
        return;
    }
    let facing = *rotation * Vec2::X;
    let side = facing.perp();
    let tail = position.0 - facing * (sim::SHIP_LENGTH * 0.5);
    let flicker = (time.elapsed_secs() * 40.0).sin() * 3.0;
    let apex = tail - facing * (12.0 + flicker);
    let half = sim::SHIP_WIDTH * 0.25;
    let color = Color::srgb(1.0, 0.7, 0.25);
    gizmos.linestrip_2d(
        [tail + side * half, apex, tail - side * half],
        color,
    );
}

// Hit flashes

/// Ship renders white-hot until this timestamp (seconds of app time).
#[derive(Component)]
pub struct FlashUntil(pub f32);

#[derive(Component)]
struct LastHealth(u16);

/// Watch replicated Health for drops on visible ships: flash the victim,
/// shake proportional to how close to home the hit was, and play the hit
/// sound. Server-authoritative health means this fires on *confirmed* hits.
fn detect_damage(
    mut commands: Commands,
    time: Res<Time>,
    mut shake: ResMut<ScreenShake>,
    sfx: Option<Res<Sfx>>,
    own: Query<&Position, (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut ships: Query<
        (
            Entity,
            &Health,
            &Position,
            Option<&mut LastHealth>,
            Has<InputMarker<Inputs>>,
        ),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    let own_pos = own.single().map(|p| p.0).ok();
    for (entity, health, position, last, is_own) in &mut ships {
        let Some(mut last) = last else {
            commands.entity(entity).try_insert(LastHealth(health.current));
            continue;
        };
        if health.current < last.0 {
            commands
                .entity(entity)
                .try_insert(FlashUntil(time.elapsed_secs() + FLASH_SECS));
            let intensity = if is_own {
                0.35
            } else {
                own_pos.map_or(0.0, |own| 0.2 * falloff(own.distance(position.0)))
            };
            shake.add_trauma(intensity);
            if let Some(sfx) = &sfx {
                let volume = if is_own {
                    0.5
                } else {
                    own_pos.map_or(0.2, |own| 0.5 * falloff(own.distance(position.0)))
                };
                play(&mut commands, &sfx.hit, volume);
            }
        }
        if health.current != last.0 {
            last.0 = health.current;
        }
    }
}

// Kill rings

#[derive(Resource, Default)]
struct ShipPosCache(HashMap<Entity, (Vec2, Color)>);

#[derive(Resource, Default)]
struct KillRings(Vec<(Vec2, f32, Color)>);

fn cache_ship_positions(
    mut cache: ResMut<ShipPosCache>,
    ships: Query<
        (Entity, &Position, &PlayerColor),
        (With<PlayerId>, Or<(With<Predicted>, With<Interpolated>)>),
    >,
) {
    for (entity, position, color) in &ships {
        cache.0.insert(entity, (position.0, color.0));
    }
}

/// A visible ship despawning is a death: pop an expanding ring at its last
/// known position, shake by proximity, and play the boom.
fn detect_kills(
    mut commands: Commands,
    mut removed: RemovedComponents<PlayerId>,
    mut cache: ResMut<ShipPosCache>,
    mut rings: ResMut<KillRings>,
    mut shake: ResMut<ScreenShake>,
    sfx: Option<Res<Sfx>>,
    time: Res<Time>,
    own: Query<&Position, (With<Predicted>, With<InputMarker<Inputs>>)>,
) {
    let own_pos = own.single().map(|p| p.0).ok();
    for entity in removed.read() {
        let Some((pos, color)) = cache.0.remove(&entity) else {
            continue;
        };
        rings.0.push((pos, time.elapsed_secs(), color));
        let closeness = own_pos.map_or(0.3, |own| falloff(own.distance(pos)));
        shake.add_trauma(0.15 + 0.35 * closeness);
        if let Some(sfx) = &sfx {
            play(&mut commands, &sfx.kill, 0.2 + 0.5 * closeness);
        }
    }
}

fn draw_kill_rings(mut gizmos: Gizmos, mut rings: ResMut<KillRings>, time: Res<Time>) {
    let now = time.elapsed_secs();
    rings.0.retain(|(_, born, _)| now - born < KILL_RING_SECS);
    for (pos, born, color) in &rings.0 {
        let age = (now - born) / KILL_RING_SECS;
        let radius = 8.0 + age * KILL_RING_RADIUS;
        let alpha = (1.0 - age) * 0.8;
        gizmos.circle_2d(
            Isometry2d::from_translation(*pos),
            radius,
            color.with_alpha(alpha),
        );
    }
}

// SFX (synthesized placeholders — no asset files)

#[derive(Resource)]
struct Sfx {
    fire: Handle<AudioSource>,
    hit: Handle<AudioSource>,
    kill: Handle<AudioSource>,
}

fn play(commands: &mut Commands, handle: &Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer::new(handle.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

/// Play the fire blip the instant the *predicted* weapon fires — same tick as
/// the press, no waiting on the server.
fn detect_own_fire(
    mut commands: Commands,
    mut last_seen: Local<Option<Tick>>,
    weapon: Query<&Weapon, (With<Predicted>, With<InputMarker<Inputs>>)>,
    sfx: Option<Res<Sfx>>,
) {
    let Ok(weapon) = weapon.single() else {
        *last_seen = None;
        return;
    };
    let fired = last_seen.is_some_and(|t| t != weapon.last_fire_tick);
    if fired {
        if let Some(sfx) = &sfx {
            play(&mut commands, &sfx.fire, 0.25);
        }
    }
    *last_seen = Some(weapon.last_fire_tick);
}

const SAMPLE_RATE: u32 = 44_100;

fn setup_sfx(mut commands: Commands, mut audio: ResMut<Assets<AudioSource>>) {
    let mut add = |samples: Vec<f32>| {
        audio.add(AudioSource {
            bytes: wav_bytes(&samples).into(),
        })
    };
    commands.insert_resource(Sfx {
        fire: add(synth_fire()),
        hit: add(synth_hit()),
        kill: add(synth_kill()),
    });
}

/// Laser pew: square-ish sweep 700→350 Hz over 60 ms with exponential decay.
fn synth_fire() -> Vec<f32> {
    let n = (SAMPLE_RATE as f32 * 0.06) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let freq = 700.0 - 350.0 * t;
            phase += freq / SAMPLE_RATE as f32;
            let square = if (phase % 1.0) < 0.5 { 1.0 } else { -1.0 };
            square * (-6.0 * t).exp() * 0.8
        })
        .collect()
}

/// Hit thunk: 180 Hz sine plus noise, 90 ms, fast decay.
fn synth_hit() -> Vec<f32> {
    let n = (SAMPLE_RATE as f32 * 0.09) as usize;
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let s = (i as f32 / SAMPLE_RATE as f32 * 180.0 * core::f32::consts::TAU).sin();
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let noise = ((rng >> 33) as f32 / (1u64 << 31) as f32) - 1.0;
            (s * 0.7 + noise * 0.5) * (-7.0 * t).exp()
        })
        .collect()
}

/// Kill boom: low-passed noise sweeping down plus an 80 Hz sub sine, 400 ms.
fn synth_kill() -> Vec<f32> {
    let n = (SAMPLE_RATE as f32 * 0.4) as usize;
    let mut rng = 0x9e37_79b9_7f4a_7c15u64;
    let mut lowpass = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let noise = ((rng >> 33) as f32 / (1u64 << 31) as f32) - 1.0;
            // Filter opens fast then closes as the boom decays.
            let cutoff = 0.4 * (1.0 - t) + 0.02;
            lowpass += (noise - lowpass) * cutoff;
            let sub = (i as f32 / SAMPLE_RATE as f32 * 80.0 * core::f32::consts::TAU).sin();
            (lowpass * 1.2 + sub * 0.4) * (-3.5 * t).exp()
        })
        .collect()
}

/// Minimal 16-bit mono PCM WAV in memory.
fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let s = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
