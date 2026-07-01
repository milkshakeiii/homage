//! Asteroids-style ship movement. Runs in `FixedUpdate` on the server and on
//! the client (for the predicted ship), so it must be deterministic: constant
//! timestep, no wall-clock time, no randomness.

use crate::protocol::{ShipHeading, ShipInput, ShipPosition, ShipVelocity};
use bevy::prelude::*;
use core::f32::consts::{PI, TAU};

/// Seconds per simulation tick. A constant (rather than `Res<Time>`) so that
/// prediction rollbacks re-simulate ticks identically.
pub const TICK_DT: f32 = 1.0 / crate::FIXED_TIMESTEP_HZ as f32;

pub const TURN_RATE: f32 = 3.5; // rad/s
pub const THRUST_ACCEL: f32 = 300.0; // units/s^2
pub const DRAG: f32 = 0.35; // fraction of velocity lost per second
pub const MAX_SPEED: f32 = 420.0; // units/s

/// Wrap an angle to [-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Advance one ship by one tick. Only writes through the `Mut` pointers when a
/// value actually changes, to avoid spurious change-detection (and therefore
/// spurious replication) for idle ships.
pub fn apply_ship_input(
    mut position: Mut<ShipPosition>,
    mut heading: Mut<ShipHeading>,
    mut velocity: Mut<ShipVelocity>,
    input: &ShipInput,
) {
    let turn = (input.turn_left as i8 - input.turn_right as i8) as f32;
    if turn != 0.0 {
        heading.0 = wrap_angle(heading.0 + turn * TURN_RATE * TICK_DT);
    }

    let mut new_velocity = velocity.0;
    if input.thrust {
        new_velocity += Vec2::from_angle(heading.0) * THRUST_ACCEL * TICK_DT;
    }
    new_velocity *= 1.0 - DRAG * TICK_DT;
    if new_velocity.length_squared() > MAX_SPEED * MAX_SPEED {
        new_velocity = new_velocity.normalize() * MAX_SPEED;
    }
    // Let ships actually come to rest instead of drifting forever on epsilons.
    if !input.thrust && new_velocity.length_squared() < 0.01 {
        new_velocity = Vec2::ZERO;
    }

    if new_velocity != velocity.0 {
        velocity.0 = new_velocity;
    }
    if new_velocity != Vec2::ZERO {
        position.0 += new_velocity * TICK_DT;
    }
}
