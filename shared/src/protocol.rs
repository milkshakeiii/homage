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
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Weapon {
    pub last_fire_tick: Tick,
    pub cooldown_ticks: u16,
    pub bullet_speed: f32,
}

impl Weapon {
    pub fn new(cooldown_ticks: u16, bullet_speed: f32) -> Self {
        Self {
            last_fire_tick: Tick(0),
            cooldown_ticks,
            bullet_speed,
        }
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BulletMarker {
    pub owner: PeerId,
}

/// Despawns `lifetime_ticks` after `origin_tick`.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BulletLifetime {
    pub origin_tick: Tick,
    pub lifetime_ticks: i32,
}

pub fn color_from_id(client_id: PeerId) -> Color {
    let hue = ((client_id.to_bits().wrapping_mul(30)) % 360) as f32;
    Color::hsl(hue, 0.8, 0.5)
}

// Inputs

/// One tick of input for an asteroids-style ship.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, Clone, Reflect)]
pub struct ShipInput {
    pub thrust: bool,
    pub turn_left: bool,
    pub turn_right: bool,
    pub fire: bool,
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

        app.component::<Name>().replicate();
        app.component::<PlayerId>().replicate();
        app.component::<PlayerColor>().replicate();
        app.component::<Health>().replicate();
        app.component::<BulletMarker>().replicate();
        app.component::<BulletLifetime>().replicate();

        app.component::<Weapon>().replicate().predict();

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
