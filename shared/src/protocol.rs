//! The network protocol: replicated components, player inputs, and their
//! registration with lightyear.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::math::Curve;
use bevy::prelude::*;
use core::f32::consts::{PI, TAU};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Bundle)]
pub struct ShipBundle {
    id: PlayerId,
    position: ShipPosition,
    heading: ShipHeading,
    velocity: ShipVelocity,
    color: PlayerColor,
}

impl ShipBundle {
    pub fn new(id: PeerId, position: Vec2) -> Self {
        let hue = ((id.to_bits().wrapping_mul(30)) % 360) as f32;
        let heading = if position == Vec2::ZERO {
            0.0
        } else {
            position.to_angle()
        };
        Self {
            id: PlayerId(id),
            position: ShipPosition(position),
            heading: ShipHeading(heading),
            velocity: ShipVelocity(Vec2::ZERO),
            color: PlayerColor(Color::hsl(hue, 0.8, 0.5)),
        }
    }
}

// Components

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerId(pub PeerId);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerColor(pub Color);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Deref, DerefMut)]
pub struct ShipPosition(pub Vec2);

/// Ship facing, in radians, kept wrapped to [-PI, PI].
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect, Deref, DerefMut)]
pub struct ShipHeading(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Reflect, Deref, DerefMut)]
pub struct ShipVelocity(pub Vec2);

impl Ease for ShipPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            ShipPosition(Vec2::lerp(start.0, end.0, t))
        })
    }
}

impl Ease for ShipHeading {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        // Interpolate along the shortest arc so crossing the -PI/PI seam
        // doesn't produce a full spin.
        FunctionCurve::new(Interval::UNIT, move |t| {
            let delta = (end.0 - start.0 + PI).rem_euclid(TAU) - PI;
            ShipHeading(start.0 + delta * t)
        })
    }
}

impl Ease for ShipVelocity {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            ShipVelocity(Vec2::lerp(start.0, end.0, t))
        })
    }
}

// Inputs

/// One tick of input for an asteroids-style ship.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, Clone, Reflect)]
pub struct ShipInput {
    pub thrust: bool,
    pub turn_left: bool,
    pub turn_right: bool,
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
        app.add_plugins(input::native::InputPlugin::<Inputs>::default());

        app.component::<PlayerId>().replicate();
        app.component::<PlayerColor>().replicate();

        app.component::<ShipPosition>()
            .replicate()
            .predict()
            .add_linear_interpolation();

        app.component::<ShipHeading>()
            .replicate()
            .predict()
            .add_linear_interpolation();

        app.component::<ShipVelocity>()
            .replicate()
            .predict()
            .add_linear_interpolation();
    }
}
