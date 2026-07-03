//! Hull definitions: per-hull stats consumed by the shared sim (movement,
//! firing), the server (spawning, hit radii, costs), and the client
//! (rendering dimensions, purchase menu). Handling is hull identity
//! (DESIGN §4.2 guidepost 8): the first thing distinguishing two hulls is
//! how they feel to fly.
//!
//! All numbers are placeholder tuning (CLAUDE.md: expect them to change).

use crate::protocol::HullKind;
use crate::sim;

#[derive(Debug, Clone, Copy)]
pub struct WeaponStats {
    pub cooldown_ticks: u16,
    pub bullet_speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct HullStats {
    /// Resource cost to buy (0 = free default).
    pub cost: u32,
    pub health: u16,
    pub cargo_capacity: u16,
    /// units/s^2 forward thrust.
    pub accel: f32,
    /// units/s^2 velocity-opposing brake.
    pub brake: f32,
    /// rad/s.
    pub turn_speed: f32,
    pub max_speed: f32,
    /// Visual/collider dimensions (ships face +X, triangle length x width).
    pub length: f32,
    pub width: f32,
    /// Lag-compensation hit circle.
    pub hit_radius: f32,
    pub weapon: Option<WeaponStats>,
}

const FIGHTER: HullStats = HullStats {
    cost: 0,
    health: sim::SHIP_HEALTH,
    cargo_capacity: sim::FIGHTER_CARGO_CAPACITY,
    accel: sim::THRUST_ACCEL,
    brake: sim::BRAKE_DECEL,
    turn_speed: sim::TURN_SPEED,
    max_speed: sim::MAX_SPEED,
    length: sim::SHIP_LENGTH,
    width: sim::SHIP_WIDTH,
    hit_radius: 12.0,
    weapon: Some(WeaponStats {
        cooldown_ticks: sim::FIRE_COOLDOWN_TICKS,
        bullet_speed: sim::BULLET_SPEED,
    }),
};

/// The ore hauler (DESIGN §5): 10x a fighter's hold, tougher, slower,
/// ponderous, and armed with a token self-defense pea-shooter. Its skill
/// expression is route planning and load management, not dogfighting.
const HARVESTER: HullStats = HullStats {
    cost: 15,
    health: 5,
    cargo_capacity: 50,
    accel: 280.0,
    brake: 420.0,
    turn_speed: 2.4,
    max_speed: 310.0,
    length: 44.0,
    width: 30.0,
    hit_radius: 18.0,
    weapon: Some(WeaponStats {
        cooldown_ticks: 40,
        bullet_speed: 380.0,
    }),
};

pub fn stats(kind: HullKind) -> &'static HullStats {
    match kind {
        HullKind::Fighter => &FIGHTER,
        HullKind::Harvester => &HARVESTER,
    }
}

/// Hulls offered in the respawn menu, in display order.
pub const PURCHASABLE: [HullKind; 2] = [HullKind::Fighter, HullKind::Harvester];

pub fn display_name(kind: HullKind) -> &'static str {
    match kind {
        HullKind::Fighter => "Fighter",
        HullKind::Harvester => "Harvester",
    }
}
