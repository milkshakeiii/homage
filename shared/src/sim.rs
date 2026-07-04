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

pub const RESPAWN_DELAY_TICKS: i32 = 192; // 3s at 64Hz

// Economy (DESIGN §3).
pub const ASTEROID_MIN_RADIUS: f32 = 28.0;
pub const ASTEROID_MAX_RADIUS: f32 = 70.0;
pub const FRAGMENT_VALUE: u16 = 1;
pub const FRAGMENT_SPEED: f32 = 55.0;
pub const FRAGMENT_TTL_TICKS: i32 = 3840; // 60s
/// Ship-to-fragment distance that counts as a scoop.
pub const SCOOP_RADIUS: f32 = 36.0;
pub const FIGHTER_CARGO_CAPACITY: u16 = 5;
/// Fraction of thrust lost at a full hold — hauling home is a piloting
/// problem (DESIGN §3), and "one more rock" is a real gamble.
pub const CARGO_ACCEL_PENALTY: f32 = 0.5;
/// Fraction of max speed lost at a full hold.
pub const CARGO_SPEED_PENALTY: f32 = 0.35;

/// Asteroid durability scales gently with size.
pub fn asteroid_health(radius: f32) -> u16 {
    2 + (radius / 25.0) as u16
}

/// Fragments ejected when an asteroid cracks.
pub fn asteroid_fragment_count(radius: f32) -> u16 {
    ((radius / 8.0) as u16).clamp(4, 12)
}

// Map (DESIGN §8): symmetric bounded arena, motherships at opposite ends.
pub const MAP_HALF_WIDTH: f32 = 6000.0;
pub const MAP_HALF_HEIGHT: f32 = 4000.0;
/// Depth over which the soft boundary ramps to full strength.
pub const BOUNDARY_MARGIN: f32 = 300.0;
pub const BOUNDARY_PUSH: f32 = 900.0; // units/s^2 at full depth

pub const MOTHERSHIP_RADIUS: f32 = 120.0;
pub const MOTHERSHIP_HEALTH: u16 = 1000;
/// Ship-to-dropoff distance that counts as depositing (lenient: guidepost 5).
pub const DEPOSIT_RADIUS: f32 = MOTHERSHIP_RADIUS + 130.0;
/// Deposit radius around a friendly resource controller (the mobile dropoff).
pub const CONTROLLER_DEPOSIT_RADIUS: f32 = 170.0;
/// One ore unit transfers per this many ticks — a full fighter hold takes
/// ~1s of hovering, so depositing is a deliberate, vulnerable moment.
pub const DEPOSIT_INTERVAL_TICKS: u16 = 12;

// Points (DESIGN §5): awarded automatically for team-positive actions.
pub const POINTS_PER_HIT: u32 = 1;
pub const POINTS_PER_ORE_DEPOSITED: u32 = 1;

/// Cracking an asteroid pays proportionally to the ore it releases.
pub fn asteroid_crack_points(radius: f32) -> u32 {
    (asteroid_fragment_count(radius) as u32 / 2).max(1)
}
/// Ships take spawn on a ring around their mothership.
pub const SPAWN_RING_RADIUS: f32 = MOTHERSHIP_RADIUS + 120.0;

pub fn team_anchor(team: Team) -> Vec2 {
    match team {
        Team::Blue => Vec2::new(-(MAP_HALF_WIDTH - 800.0), 0.0),
        Team::Red => Vec2::new(MAP_HALF_WIDTH - 800.0, 0.0),
    }
}

/// Ships face +X at zero rotation. Captain hulls are round: facing is
/// meaningless for them.
fn ship_collider(kind: HullKind) -> Collider {
    let stats = crate::hulls::stats(kind);
    if stats.archetype == crate::hulls::Archetype::Captain {
        return Collider::circle(stats.width / 2.0 * 0.9);
    }
    Collider::convex_hull(vec![
        Vec2::new(stats.length / 2.0, 0.0),
        Vec2::new(-stats.length / 2.0, stats.width / 2.0),
        Vec2::new(-stats.length / 2.0, -stats.width / 2.0),
    ])
    .expect("ship collider hull")
}

/// The physics components a simulated ship needs. Used by the server when
/// spawning, and by the client for its *predicted* ship copy: physics
/// components aren't replicated, and without a RigidBody avian never
/// integrates the predicted ship's position — prediction degrades to
/// snapping to server updates.
pub fn ship_physics(kind: HullKind) -> impl Bundle {
    (
        RigidBody::Dynamic,
        ship_collider(kind),
        ColliderDensity(1.0),
        LinearDamping(SHIP_DAMPING),
    )
}

/// Which way a team's ships face at spawn: toward the enemy.
pub fn team_facing(team: Team) -> f32 {
    match team {
        Team::Blue => 0.0,
        Team::Red => core::f32::consts::PI,
    }
}

/// Deterministic spawn spot: a ring around the team's mothership, facing the
/// enemy's side of the map.
pub fn spawn_pose(client_id: PeerId, team: Team) -> (Position, Rotation) {
    spawn_pose_at(client_id, team, team_anchor(team), SPAWN_RING_RADIUS)
}

/// Spawn on a ring around an arbitrary facility (e.g. a strike carrier).
pub fn spawn_pose_at(
    client_id: PeerId,
    team: Team,
    center: Vec2,
    ring_radius: f32,
) -> (Position, Rotation) {
    let angle = (client_id.to_bits() % 16) as f32 / 16.0 * TAU;
    let position = center + Vec2::from_angle(angle) * ring_radius;
    (Position(position), Rotation::radians(team_facing(team)))
}

/// Everything a ship needs on the server; replication/prediction targets are
/// added separately by the server. The loadout must already be validated
/// (unlocks + facility stocking).
pub fn ship_bundle(
    client_id: PeerId,
    team: Team,
    kind: HullKind,
    loadout: Loadout,
    pose: (Position, Rotation),
) -> impl Bundle {
    let stats = crate::hulls::stats(kind);
    let mods = crate::fittings::hull_mod_effects(loadout.hull_mod);
    let weapon = crate::fittings::weapon_profile(loadout.weapon, kind);
    let (cooldown, bullet_speed) = weapon
        .map(|w| (w.cooldown_ticks, w.bullet_speed))
        .unwrap_or((u16::MAX, 0.0));
    let health = (stats.health as i32 + mods.health_bonus).max(1) as u16;
    let cargo = (stats.cargo_capacity as f32 * mods.cargo_mult) as u16;
    let (position, rotation) = pose;
    (
        PlayerId(client_id),
        team,
        kind,
        PlayerColor(color_from_id(client_id, team)),
        Health::new(health),
        Weapon::new(cooldown, bullet_speed),
        Equipped(loadout),
        UtilityState::default(),
        CargoHold::empty(cargo),
        TurretAim(0.0),
        position,
        rotation,
        ship_physics(kind),
        Name::from(crate::hulls::display_name(kind)),
    )
}

/// The server-side mothership: an unpiloted team structure (DESIGN §2) —
/// dropoff and build site. Not damageable yet (win condition is M4).
pub fn mothership_bundle(team: Team) -> impl Bundle {
    (
        Mothership,
        team,
        Health::new(MOTHERSHIP_HEALTH),
        Position(team_anchor(team)),
        Rotation::default(),
        RigidBody::Static,
        Collider::circle(MOTHERSHIP_RADIUS),
        Name::from("Mothership"),
    )
}

/// A minable rock (server-side; static in M1).
pub fn asteroid_bundle(position: Vec2, radius: f32, seed: u16) -> impl Bundle {
    (
        Asteroid { radius, seed },
        Health::new(asteroid_health(radius)),
        Position(position),
        Rotation::default(),
        RigidBody::Static,
        Collider::circle(radius * 0.9),
        Name::from("Asteroid"),
    )
}

/// A drifting ore chunk (server-side). Kinematic: constant ballistic drift,
/// scooped by proximity, no collisions.
pub fn fragment_bundle(position: Vec2, velocity: Vec2, tick: Tick) -> impl Bundle {
    (
        OreFragment {
            value: FRAGMENT_VALUE,
        },
        Position(position),
        LinearVelocity(velocity),
        RigidBody::Kinematic,
        Expires {
            origin_tick: tick,
            lifetime_ticks: FRAGMENT_TTL_TICKS,
        },
        Name::from("Ore"),
    )
}

/// A bullet is uniquely identified by its owner and the tick it was fired on;
/// the client's prespawned bullet and the server's authoritative bullet
/// compute the same hash and get matched by lightyear.
pub fn bullet_prespawn_hash(owner: PeerId, tick: Tick, pellet: u8) -> u64 {
    let mut x = owner.to_bits() ^ ((tick.0 as u64) << 32) ^ tick.0 as u64;
    x ^= (pellet as u64 + 1) << 48;
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
    timeline: Res<LocalTimeline>,
    mut query: Query<
        (
            &ActionState<Inputs>,
            &Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
            Option<&CargoHold>,
            Option<&HullKind>,
            Option<&Equipped>,
            Option<&mut UtilityState>,
        ),
        With<PlayerId>,
    >,
) {
    let tick = timeline.tick();
    for (action_state, rotation, mut linvel, mut angvel, cargo, kind, equipped, utility) in
        &mut query
    {
        let stats = crate::hulls::stats(kind.copied().unwrap_or(HullKind::Fighter));
        let input = &action_state.0 .0;
        let load = cargo.map_or(0.0, CargoHold::load_fraction);
        let loadout = equipped.map(|e| e.0).unwrap_or_default();
        let mods = crate::fittings::hull_mod_effects(loadout.hull_mod);
        // Afterburner: hold the ability key for more thrust (guidepost 4's
        // heat cost is a follow-up).
        let burner = if input.ability && loadout.utility == Some(FittingId::Afterburner) {
            crate::fittings::AFTERBURNER_ACCEL_MULT
        } else {
            1.0
        };
        // Blink: an instant impulse along the nose on a fixed cooldown, all
        // in the predicted UtilityState so rollbacks replay it.
        if input.ability && loadout.utility == Some(FittingId::BlinkThruster) {
            if let Some(mut state) = utility {
                if (tick - state.ready_at) >= 0 {
                    linvel.0 += *rotation * Vec2::X * crate::fittings::BLINK_IMPULSE;
                    state.ready_at = tick + Tick(crate::fittings::BLINK_COOLDOWN_TICKS as u32);
                }
            }
        }

        // Captain hulls (DESIGN §4.1) drift omnidirectionally: WASD nudges in
        // screen space, no meaningful facing, the mouse is for abilities.
        if stats.archetype == crate::hulls::Archetype::Captain {
            let nudge = Vec2::new(
                (input.turn_right as i8 - input.turn_left as i8) as f32,
                (input.thrust as i8 - input.brake as i8) as f32,
            );
            if nudge != Vec2::ZERO {
                linvel.0 += nudge.normalize() * stats.accel * TICK_DT;
            }
            if angvel.0 != 0.0 {
                angvel.0 = 0.0;
            }
            continue;
        }

        if input.thrust {
            let accel =
                stats.accel * mods.accel_mult * burner * (1.0 - CARGO_ACCEL_PENALTY * load);
            linvel.0 += *rotation * Vec2::X * accel * TICK_DT;
        }
        // Brake opposes the velocity vector regardless of facing: a recovery
        // tool (skill floor), while reversing by turn-and-thrust stays the
        // faster, skillful option (skill ceiling).
        if input.brake {
            let speed = linvel.0.length();
            let decel = stats.brake * TICK_DT;
            linvel.0 = if decel >= speed {
                Vec2::ZERO
            } else {
                linvel.0 - linvel.0 * (decel / speed)
            };
        }
        let turn = stats.turn_speed * mods.turn_mult;
        let desired_ang_vel = if input.turn_left {
            turn
        } else if input.turn_right {
            -turn
        } else {
            0.0
        };
        if angvel.0 != desired_ang_vel {
            angvel.0 = desired_ang_vel;
        }
    }
}

/// Asteroids-style speed cap on top of damping; a loaded hold lowers the
/// cap, hull mods and a lit afterburner move it.
pub fn clamp_ship_speed(
    mut query: Query<
        (
            &mut LinearVelocity,
            Option<&CargoHold>,
            Option<&HullKind>,
            Option<&Equipped>,
            Option<&ActionState<Inputs>>,
        ),
        With<PlayerId>,
    >,
) {
    for (mut velocity, cargo, kind, equipped, action) in &mut query {
        let stats = crate::hulls::stats(kind.copied().unwrap_or(HullKind::Fighter));
        let load = cargo.map_or(0.0, CargoHold::load_fraction);
        let loadout = equipped.map(|e| e.0).unwrap_or_default();
        let mods = crate::fittings::hull_mod_effects(loadout.hull_mod);
        let burner = if loadout.utility == Some(FittingId::Afterburner)
            && action.is_some_and(|a| a.0 .0.ability)
        {
            crate::fittings::AFTERBURNER_SPEED_MULT
        } else {
            1.0
        };
        let max =
            stats.max_speed * mods.max_speed_mult * burner * (1.0 - CARGO_SPEED_PENALTY * load);
        if velocity.0.length_squared() > max * max {
            velocity.0 = velocity.0.normalize() * max;
        }
    }
}

/// Gunship turrets track the owner's aim input (server-side for all ships,
/// from replicated inputs). Cosmetic for remote rendering; firing reads the
/// aim input directly so this never affects hit registration.
pub fn update_turrets(
    mut query: Query<(&ActionState<Inputs>, &HullKind, &mut TurretAim), With<PlayerId>>,
) {
    for (action_state, kind, mut turret) in &mut query {
        if crate::hulls::stats(*kind).archetype == crate::hulls::Archetype::Gunship {
            let aim = action_state.0 .0.aim_radians();
            if turret.0 != aim {
                turret.0 = aim;
            }
        }
    }
}

/// Soft map boundary (DESIGN §8): a push-back force that ramps up over
/// `BOUNDARY_MARGIN` outside the play area instead of a hard wall. Runs in
/// the shared sim so the predicted ship feels it with zero latency.
pub fn soft_boundary(mut query: Query<(&Position, &mut LinearVelocity), With<PlayerId>>) {
    for (position, mut velocity) in &mut query {
        let p = position.0;
        let overshoot = Vec2::new(
            (p.x.abs() - MAP_HALF_WIDTH).max(0.0) * -p.x.signum(),
            (p.y.abs() - MAP_HALF_HEIGHT).max(0.0) * -p.y.signum(),
        );
        if overshoot != Vec2::ZERO {
            let strength = (overshoot.length() / BOUNDARY_MARGIN).min(1.0);
            velocity.0 += overshoot.normalize() * BOUNDARY_PUSH * strength * TICK_DT;
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
        Option<&HullKind>,
        Option<&Equipped>,
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
        kind,
        equipped,
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

        // Pilot hulls fire down the nose; Gunship hulls fire along the
        // mouse-aimed turret. Both spawn bullets clear of the hull and
        // inherit the ship's velocity (guidepost 3). The equipped weapon
        // fitting sets pellet count, spread, speed, and reach.
        let hull = kind.copied().unwrap_or(HullKind::Fighter);
        let stats = crate::hulls::stats(hull);
        let loadout = equipped.map(|e| e.0).unwrap_or_default();
        let Some(profile) = crate::fittings::weapon_profile(loadout.weapon, hull) else {
            continue;
        };
        let forward = match stats.archetype {
            crate::hulls::Archetype::Gunship => Vec2::from_angle(input.aim_radians()),
            _ => *rotation * Vec2::X,
        };
        let lifetime = (BULLET_LIFETIME_TICKS as f32 * profile.lifetime_mult) as i32;
        for pellet in 0..profile.pellets {
            let offset = pellet as f32 - (profile.pellets as f32 - 1.0) / 2.0;
            let dir = Vec2::from_angle(forward.to_angle() + offset * profile.spread);
            let origin = position.0 + dir * (stats.length / 2.0 + BULLET_SIZE + 2.0);
            let bullet_velocity = dir * weapon.bullet_speed + velocity.0;

            let bullet = commands
                .spawn((
                    Position(origin),
                    LinearVelocity(bullet_velocity),
                    RigidBody::Kinematic,
                    BulletMarker { owner: player_id.0 },
                    PlayerColor(color.0),
                    Expires {
                        origin_tick: current_tick,
                        lifetime_ticks: lifetime,
                    },
                    PreSpawned::new(bullet_prespawn_hash(player_id.0, current_tick, pellet)),
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
}

/// Despawn bullets after their lifetime expires (both sides; the predicted
/// copy despawns locally, the authoritative one despawns via replication).
pub fn lifetime_despawner(
    query: Query<(Entity, &Expires)>,
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
