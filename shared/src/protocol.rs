//! The network protocol: replicated components, player inputs, and their
//! registration with lightyear.

use avian2d::prelude::*;
use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use lightyear::input::config::InputConfig;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

// Components

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub PeerId);

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Blue,
    Red,
}

impl Team {
    pub fn opponent(self) -> Team {
        match self {
            Team::Blue => Team::Red,
            Team::Red => Team::Blue,
        }
    }
}

/// The team's central structure: only place carrier-type hulls are built,
/// default resource dropoff, and (eventually) the win condition.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mothership;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerColor(pub Color);

/// Server-authoritative hit points. Never predicted: the server is the only
/// authority on damage, so clients just render whatever they last heard.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Health {
    pub current: u16,
    pub max: u16,
}

impl Health {
    pub fn new(max: u16) -> Self {
        Self { current: max, max }
    }
}

/// Fire-rate limiter: after firing on `last_fire_tick` you must wait
/// `cooldown_ticks` before firing again. Predicted so that the client's
/// firing decisions roll back identically to the server's.
///
/// `fire_requested` implements input buffering (feel guidepost: a fire press
/// during cooldown fires on the first legal tick instead of being eaten).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Weapon {
    pub last_fire_tick: Tick,
    pub cooldown_ticks: u16,
    pub bullet_speed: f32,
    pub fire_requested: Option<Tick>,
}

impl Weapon {
    pub fn new(cooldown_ticks: u16, bullet_speed: f32) -> Self {
        Self {
            last_fire_tick: Tick(0),
            cooldown_ticks,
            bullet_speed,
            fire_requested: None,
        }
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BulletMarker {
    pub owner: PeerId,
    /// Hit damage. Small arms are 1; a bomber torpedo is heavy enough to
    /// punch through mothership damage reduction.
    pub damage: u16,
}

/// Despawns `lifetime_ticks` after `origin_tick` (bullets, ore fragments).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Expires {
    pub origin_tick: Tick,
    pub lifetime_ticks: i32,
}

/// A minable rock. `seed` gives the client a stable irregular silhouette.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Asteroid {
    pub radius: f32,
    pub seed: u16,
}

/// Scoopable ore chunk, ejected by cracking an asteroid (or by dying with
/// cargo aboard).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OreFragment {
    pub value: u16,
}

/// Which hull a ship is. Stats live in `crate::hulls`; this replicates so
/// both sides simulate and render the right hull.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HullKind {
    Fighter,
    Harvester,
    Corvette,
    Bomber,
    ResourceController,
    StrikeCarrier,
}

/// Where a Gunship hull's turret points (world-space radians). Written from
/// the owner's aim input in the shared sim; replicated so *other* clients
/// can render the turret. The owning client renders from its own live input
/// instead (zero latency).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TurretAim(pub f32);

/// Client → server: what to fly on the next (re)spawn, and where. Applied
/// when the respawn happens; costs are deducted then (hulls are lost on
/// death, DESIGN §6). `spawn_at` is a replicated facility entity (mothership
/// or friendly strike carrier); the server validates team and hull-class
/// eligibility and falls back to the rules when it's missing or invalid.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SpawnOrder {
    pub hull: HullKind,
    pub spawn_at: Option<Entity>,
    pub loadout: Loadout,
}

impl MapEntities for SpawnOrder {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        if let Some(entity) = &mut self.spawn_at {
            *entity = entity_mapper.get_mapped(*entity);
        }
    }
}

/// Client → server: scuttle my ship. The only way to swap hulls without an
/// enemy's help; drops cargo like any other death.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SelfDestruct;

/// Client → server: deploy me now (the map-click on the spawn screen). The
/// server spawns the standing SpawnOrder once the respawn delay has elapsed;
/// confirms sent early are remembered. Without a confirm the player stays on
/// the spawn screen indefinitely (Savage-style).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SpawnConfirm;

/// Client → server: dev cheats for manual testing (F-keys on the client).
/// Always-on during development; must be gated or stripped before any
/// public-facing build.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum CheatOrder {
    /// Add ore to the bank without harvesting.
    GiveOre(u32),
    /// Add points without earning them.
    GivePoints(u32),
    /// Spawn an asteroid at a world position (usually the cursor).
    SpawnAsteroid(Vec2),
    /// Scatter a ring of ore fragments at a world position.
    SpawnFragments(Vec2),
    /// Spawn a stationary enemy target drone to shoot at.
    SpawnTargetDrone(Vec2),
    /// Teleport my ship to a world position.
    Teleport(Vec2),
    /// Restore my ship to full health.
    Heal,
}

/// Reliable channel for player orders (spawn requests, later: build orders).
pub struct OrdersChannel;

/// Undeposited ore aboard a ship. Server-authoritative; carried mass
/// degrades handling (see shared sim).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CargoHold {
    pub current: u16,
    pub capacity: u16,
}

impl CargoHold {
    pub fn empty(capacity: u16) -> Self {
        Self {
            current: 0,
            capacity,
        }
    }

    pub fn load_fraction(&self) -> f32 {
        if self.capacity == 0 {
            0.0
        } else {
            self.current as f32 / self.capacity as f32
        }
    }
}

/// Deposited resources (the player's personal bank, DESIGN §3). The
/// authoritative store lives in a server-side map keyed by player so it
/// survives death; this component mirrors it onto the ship for display.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bank(pub u32);

/// Points earned for team-positive actions (DESIGN §5): damage, kills,
/// deposits. Like the bank, the authoritative store is server-side and
/// persists through death; points buy fitting unlocks (M3).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Points(pub u32);

/// A persistent per-player scoreboard entry (replicated to everyone).
/// Deliberately NOT PlayerId: ship systems query With<PlayerId> and roster
/// entities must never look like ships. Lives on its own entity so stats
/// stay visible while the player is dead.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct RosterEntry(pub PeerId);

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Kills(pub u32);

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Deaths(pub u32);

/// Every fitting in the game. Catalog data lives in `crate::fittings`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FittingId {
    PulseCannon,
    ScatterGun,
    LongLance,
    Afterburner,
    BlinkThruster,
    GyroTuning,
    ArmorPlate,
    LightweightFrame,
    CompactedHold,
}

/// A ship's chosen fittings, one per slot (DESIGN §5).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Loadout {
    pub weapon: FittingId,
    pub utility: Option<FittingId>,
    pub hull_mod: Option<FittingId>,
}

impl Default for Loadout {
    fn default() -> Self {
        Self {
            weapon: FittingId::PulseCannon,
            utility: None,
            hull_mod: None,
        }
    }
}

/// What a ship actually spawned with (post-validation). Replicated so both
/// the owner's sim (movement/firing effects) and other clients agree.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Equipped(pub Loadout);

/// Cooldown state for the utility slot (blink). Predicted like Weapon so
/// rollbacks replay it identically.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UtilityState {
    pub ready_at: Tick,
}

impl Default for UtilityState {
    fn default() -> Self {
        Self { ready_at: Tick(0) }
    }
}

/// The player's match-permanent unlocks, mirrored onto the ship for the
/// spawn-screen UI (sorted for deterministic replication).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct UnlockedFittings(pub Vec<FittingId>);

/// Client → server: spend points to permanently unlock a fitting.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnlockOrder {
    pub fitting: FittingId,
}

/// Server → client: a mothership fell; the match is over. The world resets
/// shortly after.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MatchResult {
    pub winner: Team,
}

/// Server → client: authoritative wealth + unlocks snapshot. Sent after
/// unlock orders (and on demand) because the ship components that normally
/// mirror these die with the ship — and unlocking happens on the death
/// screen.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WealthUpdate {
    pub bank: u32,
    pub points: u32,
    pub unlocked: Vec<FittingId>,
}

/// Per-player color within the team's hue band: friend-or-foe is readable at
/// a glance, individuals still distinguishable.
pub fn color_from_id(client_id: PeerId, team: Team) -> Color {
    let base = match team {
        Team::Blue => 210.0,
        Team::Red => 350.0,
    };
    let spread = ((client_id.to_bits().wrapping_mul(47)) % 80) as f32 - 40.0;
    Color::hsl((base + spread).rem_euclid(360.0), 0.8, 0.55)
}

// Inputs

/// One tick of input, a superset across all control archetypes (DESIGN §4.1):
/// Pilot hulls read the buttons, Gunship hulls will read `aim`, Captain hulls
/// will read `cursor_*`. Unused fields stay zero, so the wire format and
/// prediction rollback are archetype-agnostic. Analog values are quantized to
/// integers to keep `Eq` (and input-delta compression) exact.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, Clone, Reflect)]
pub struct ShipInput {
    pub thrust: bool,
    pub brake: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub fire: bool,
    pub ability: bool,
    /// Turret aim as a quantized world-space angle: `TAU * aim / 65536`.
    pub aim: u16,
    /// Ability-target cursor in world space, quantized to 0.5-unit steps.
    pub cursor_x: i16,
    pub cursor_y: i16,
}

/// World units per cursor quantization step.
pub const CURSOR_STEP: f32 = 0.5;

impl ShipInput {
    pub fn aim_radians(&self) -> f32 {
        self.aim as f32 / 65536.0 * core::f32::consts::TAU
    }

    pub fn set_aim_radians(&mut self, radians: f32) {
        let turns = radians / core::f32::consts::TAU;
        self.aim = (turns.rem_euclid(1.0) * 65536.0) as u16;
    }

    pub fn cursor_world(&self) -> Vec2 {
        Vec2::new(self.cursor_x as f32, self.cursor_y as f32) * CURSOR_STEP
    }

    pub fn set_cursor_world(&mut self, world: Vec2) {
        let q = (world / CURSOR_STEP).round();
        self.cursor_x = q.x.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        self.cursor_y = q.y.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, Reflect)]
pub struct Inputs(pub ShipInput);

impl MapEntities for Inputs {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

// Protocol registration

#[derive(Clone)]
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(input::native::InputPlugin::<Inputs> {
            config: InputConfig::<Inputs> {
                // Input messages carry the client's interpolation delay so the
                // server can rewind targets for lag-compensated hit detection.
                lag_compensation: true,
                ..default()
            },
        });

        // Both the channel and the message need the direction: the channel
        // direction wires the channel into each link's Transport (without it,
        // sends fail with ChannelNotFound); the message direction adds the
        // MessageSender/MessageReceiver components to the link entities.
        app.add_channel::<OrdersChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
        app.register_message::<SpawnOrder>()
            .add_direction(NetworkDirection::ClientToServer)
            .add_map_entities();
        app.register_message::<SelfDestruct>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SpawnConfirm>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<UnlockOrder>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<WealthUpdate>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<MatchResult>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<CheatOrder>()
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<Name>().replicate();
        app.component::<PlayerId>().replicate();
        app.component::<Team>().replicate();
        app.component::<HullKind>().replicate();
        app.component::<TurretAim>().replicate();
        app.component::<Mothership>().replicate();
        app.component::<PlayerColor>().replicate();
        app.component::<Health>().replicate();
        app.component::<BulletMarker>().replicate();
        app.component::<Expires>().replicate();
        app.component::<Asteroid>().replicate();
        app.component::<OreFragment>().replicate();
        app.component::<CargoHold>().replicate();
        app.component::<Bank>().replicate();
        app.component::<Points>().replicate();
        app.component::<Equipped>().replicate();
        app.component::<RosterEntry>().replicate();
        app.component::<Kills>().replicate();
        app.component::<Deaths>().replicate();
        app.component::<UnlockedFittings>().replicate();

        app.component::<Weapon>().replicate().predict();
        app.component::<UtilityState>().replicate().predict();

        // Avian physics state. Position/Rotation are the visual components, so
        // they get interpolation (remote entities) and a correction function
        // (smears rollback snaps over a few frames). Velocities are simulation
        // state only: predicted, but no visual smoothing needed.
        app.component::<Position>()
            .replicate()
            .predict()
            .with_rollback_condition(position_should_rollback)
            .add_linear_interpolation()
            .add_linear_correction_fn();

        app.component::<Rotation>()
            .replicate()
            .predict()
            .with_rollback_condition(rotation_should_rollback)
            .add_linear_interpolation()
            .add_linear_correction_fn();

        app.component::<LinearVelocity>()
            .replicate()
            .predict()
            .with_rollback_condition(linear_velocity_should_rollback);

        app.component::<AngularVelocity>()
            .replicate()
            .predict()
            .with_rollback_condition(angular_velocity_should_rollback);
    }
}

// Rollback only when the divergence is above a small epsilon, so that benign
// floating-point noise doesn't trigger constant re-simulation.

fn position_should_rollback(this: &Position, that: &Position) -> bool {
    (this.0 - that.0).length() >= 0.01
}

fn rotation_should_rollback(this: &Rotation, that: &Rotation) -> bool {
    this.angle_between(*that) >= 0.01
}

fn linear_velocity_should_rollback(this: &LinearVelocity, that: &LinearVelocity) -> bool {
    (this.0 - that.0).length() >= 0.01
}

fn angular_velocity_should_rollback(this: &AngularVelocity, that: &AngularVelocity) -> bool {
    (this.0 - that.0).abs() >= 0.01
}
