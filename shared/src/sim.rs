//! The shared simulation: ship movement, firing, and bullet lifetime. These
//! systems run in `FixedUpdate` on both the server and the client (for
//! predicted entities), so they must behave identically on both sides.

use crate::protocol::*;
use avian2d::prelude::*;
use bevy::prelude::*;
use core::f32::consts::TAU;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;

/// Seconds per simulation tick, as a constant so prediction rollbacks
/// re-simulate identically.
pub const TICK_DT: f32 = 1.0 / crate::FIXED_TIMESTEP_HZ as f32;

pub const SHIP_LENGTH: f32 = 32.0;
pub const SHIP_WIDTH: f32 = 19.0;
pub const SHIP_HEALTH: u16 = 3;
// Feel bar (DESIGN §4.2): ~1.5s from rest to max speed, and reversing course
// noticeably faster than accelerating. With damping 0.4 and accel 360, a
// fighter hits the 420 cap in ~1.6s; thrusting against full speed stops it
// in ~0.8s; braking is in between.
pub const THRUST_ACCEL: f32 = 360.0; // units/s^2
pub const BRAKE_DECEL: f32 = 480.0; // units/s^2, opposes velocity
pub const TURN_SPEED: f32 = 3.5; // rad/s
pub const SHIP_DAMPING: f32 = 0.4; // avian LinearDamping
pub const MAX_SPEED: f32 = 420.0;

pub const BULLET_SIZE: f32 = 2.0;
pub const BULLET_SPEED: f32 = 500.0;
pub const BULLET_LIFETIME_TICKS: i32 = 128; // 2s at 64Hz
pub const FIRE_COOLDOWN_TICKS: u16 = 16; // 4 shots/s
/// How long a fire press stays buffered waiting for the cooldown (125ms).
pub const FIRE_BUFFER_TICKS: i32 = 8;

pub const SPAWN_RING_RADIUS: f32 = 200.0;
pub const RESPAWN_DELAY_TICKS: i32 = 192; // 3s at 64Hz

/// Ships face +X at zero rotation.
fn ship_collider() -> Collider {
    Collider::convex_hull(vec![
        Vec2::new(SHIP_LENGTH / 2.0, 0.0),
        Vec2::new(-SHIP_LENGTH / 2.0, SHIP_WIDTH / 2.0),
        Vec2::new(-SHIP_LENGTH / 2.0, -SHIP_WIDTH / 2.0),
    ])
    .expect("ship collider hull")
}

/// Deterministic spawn spot: spread around a ring, facing outward.
pub fn spawn_pose(client_id: PeerId) -> (Position, Rotation) {
    let angle = (client_id.to_bits() % 16) as f32 / 16.0 * TAU;
    (
        Position(Vec2::from_angle(angle) * SPAWN_RING_RADIUS),
        Rotation::radians(angle),
    )
}

/// Everything a ship needs on the server; replication/prediction targets are
/// added separately by the server.
pub fn ship_bundle(client_id: PeerId) -> impl Bundle {
    let (position, rotation) = spawn_pose(client_id);
    (
        PlayerId(client_id),
        PlayerColor(color_from_id(client_id)),
        Health::new(SHIP_HEALTH),
        Weapon::new(FIRE_COOLDOWN_TICKS, BULLET_SPEED),
        position,
        rotation,
        RigidBody::Dynamic,
        ship_collider(),
        ColliderDensity(1.0),
        LinearDamping(SHIP_DAMPING),
        Name::from("Ship"),
    )
}

/// A bullet is uniquely identified by its owner and the tick it was fired on;
/// the client's prespawned bullet and the server's authoritative bullet
/// compute the same hash and get matched by lightyear.
pub fn bullet_prespawn_hash(owner: PeerId, tick: Tick) -> u64 {
    let mut x = owner.to_bits() ^ ((tick.0 as u64) << 32) ^ tick.0 as u64;
    // SplitMix64 finalizer.
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Apply inputs to ships by writing velocities directly. Deterministic manual
/// integration (rather than avian's force accumulators) keeps prediction
/// rollbacks exact; avian still integrates positions and resolves collisions.
///
/// On the server this runs for every ship (using each client's replicated
/// inputs); on the client only the local predicted ship has an `ActionState`,
/// so no extra filtering is needed. (Notably, do NOT filter on
/// `Without<Interpolated>` here: lightyear 0.28 leaves `Interpolated` present
/// on server-side entities that have an `InterpolationTarget`, which silently
/// empties the query.)
pub fn player_movement(
    mut query: Query<
        (
            &ActionState<Inputs>,
            &Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<PlayerId>,
    >,
) {
    for (action_state, rotation, mut linvel, mut angvel) in &mut query {
        let input = &action_state.0 .0;
        if input.thrust {
            linvel.0 += *rotation * Vec2::X * THRUST_ACCEL * TICK_DT;
        }
        // Brake opposes the velocity vector regardless of facing: a recovery
        // tool (skill floor), while reversing by turn-and-thrust stays the
        // faster, skillful option (skill ceiling).
        if input.brake {
            let speed = linvel.0.length();
            let decel = BRAKE_DECEL * TICK_DT;
            linvel.0 = if decel >= speed {
                Vec2::ZERO
            } else {
                linvel.0 - linvel.0 * (decel / speed)
            };
        }
        let desired_ang_vel = if input.turn_left {
            TURN_SPEED
        } else if input.turn_right {
            -TURN_SPEED
        } else {
            0.0
        };
        if angvel.0 != desired_ang_vel {
            angvel.0 = desired_ang_vel;
        }
    }
}

/// Asteroids-style speed cap on top of damping.
pub fn clamp_ship_speed(mut query: Query<&mut LinearVelocity, With<PlayerId>>) {
    for mut velocity in &mut query {
        if velocity.0.length_squared() > MAX_SPEED * MAX_SPEED {
            velocity.0 = velocity.0.normalize() * MAX_SPEED;
        }
    }
}

/// Fire bullets. Runs on both sides: the server spawns the authoritative
/// (replicated) bullet, the client prespawns a predicted copy that lightyear
/// matches to the server's via the prespawn hash.
pub fn shared_player_firing(
    mut query: Query<(
        &Position,
        &Rotation,
        &LinearVelocity,
        &PlayerColor,
        &PlayerId,
        &ActionState<Inputs>,
        &mut Weapon,
        Has<Predicted>,
        Has<Interpolated>,
        Option<&ControlledBy>,
    )>,
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    synced_client: Query<(), (With<Client>, With<IsSynced<InputTimeline>>)>,
    server: Query<(), With<Server>>,
) {
    let client_is_synced = !synced_client.is_empty();
    let is_server = !server.is_empty();
    let current_tick = timeline.tick();

    for (
        position,
        rotation,
        velocity,
        color,
        player_id,
        action_state,
        mut weapon,
        is_predicted,
        is_interpolated,
        controlled_by,
    ) in &mut query
    {
        if is_server {
            if controlled_by.is_none() {
                continue;
            }
        } else if !client_is_synced || !is_predicted || is_interpolated {
            continue;
        }
        let Inputs(input) = &action_state.0;
        // Input buffering: remember the most recent fire press and honor it
        // on the first tick the cooldown allows, so a press during cooldown
        // is never eaten. All in wrapped tick arithmetic, and all part of the
        // predicted Weapon component so rollbacks replay it identically.
        if input.fire {
            weapon.fire_requested = Some(current_tick);
        }
        let Some(requested) = weapon.fire_requested else {
            continue;
        };
        if (current_tick - requested) > FIRE_BUFFER_TICKS {
            weapon.fire_requested = None;
            continue;
        }
        let ticks_since_fire = current_tick - weapon.last_fire_tick;
        if ticks_since_fire.abs() <= weapon.cooldown_ticks as i32 {
            continue;
        }
        weapon.last_fire_tick = current_tick;
        weapon.fire_requested = None;

        // The bullet spawns off the nose and inherits the ship's velocity.
        let forward = *rotation * Vec2::X;
        let origin = position.0 + forward * (SHIP_LENGTH / 2.0 + BULLET_SIZE + 2.0);
        let bullet_velocity = forward * weapon.bullet_speed + velocity.0;

        let bullet = commands
            .spawn((
                Position(origin),
                LinearVelocity(bullet_velocity),
                RigidBody::Kinematic,
                BulletMarker { owner: player_id.0 },
                PlayerColor(color.0),
                BulletLifetime {
                    origin_tick: current_tick,
                    lifetime_ticks: BULLET_LIFETIME_TICKS,
                },
                PreSpawned::new(bullet_prespawn_hash(player_id.0, current_tick)),
                Name::from("Bullet"),
            ))
            .id();

        if is_server {
            commands.entity(bullet).insert((
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::Single(player_id.0)),
                InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(player_id.0)),
                controlled_by.unwrap().clone(),
            ));
        }
    }
}

/// Despawn bullets after their lifetime expires (both sides; the predicted
/// copy despawns locally, the authoritative one despawns via replication).
pub fn lifetime_despawner(
    query: Query<(Entity, &BulletLifetime)>,
    timeline: Res<LocalTimeline>,
    mut commands: Commands,
) {
    let tick = timeline.tick();
    for (entity, lifetime) in &query {
        if (tick - lifetime.origin_tick) > lifetime.lifetime_ticks as i32 {
            commands.entity(entity).try_despawn();
        }
    }
}
