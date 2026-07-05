//! Hull definitions: per-hull stats consumed by the shared sim (movement,
//! firing), the server (spawning, hit radii, costs), and the client
//! (rendering dimensions, purchase menu). Handling is hull identity
//! (DESIGN §4.2 guidepost 8): the first thing distinguishing two hulls is
//! how they feel to fly.
//!
//! All numbers are placeholder tuning (CLAUDE.md: expect them to change).

use crate::protocol::HullKind;
use crate::sim;

/// How a hull is flown (DESIGN §4.1). Pilot: tank controls, weapons down the
/// nose. Gunship: hull flies on momentum while the mouse aims an independent
/// turret. Captain: omnidirectional drift + mouse-targeted abilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    Pilot,
    Gunship,
    Captain,
}

#[derive(Debug, Clone, Copy)]
pub struct WeaponStats {
    pub cooldown_ticks: u16,
    pub bullet_speed: f32,
    pub damage: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct HullStats {
    pub archetype: Archetype,
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
    archetype: Archetype::Pilot,
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
        damage: 1,
    }),
};

/// The ore hauler (DESIGN §5): 10x a fighter's hold, tougher, slower,
/// ponderous, and armed with a token self-defense pea-shooter. Its skill
/// expression is route planning and load management, not dogfighting.
const HARVESTER: HullStats = HullStats {
    archetype: Archetype::Pilot,
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
        damage: 1,
    }),
};

/// The anti-fighter screen (DESIGN §5): the first Gunship hull. The hull is
/// quick but deliberate; the mouse-aimed turret is the fast part. Rapid,
/// short-cooldown shots reward tracking.
const CORVETTE: HullStats = HullStats {
    archetype: Archetype::Gunship,
    cost: 25,
    health: 4,
    cargo_capacity: 3,
    accel: 320.0,
    brake: 460.0,
    turn_speed: 2.8,
    max_speed: 380.0,
    length: 36.0,
    width: 24.0,
    hit_radius: 14.0,
    weapon: Some(WeaponStats {
        cooldown_ticks: 9,
        bullet_speed: 560.0,
        damage: 1,
    }),
};

/// The capital-killer (DESIGN §5): slow dumb-fire torpedoes that punch
/// through mothership damage reduction, on a hull that loses every dogfight.
/// Helpless against fighters by geometry: the torpedo can't track them.
const BOMBER: HullStats = HullStats {
    archetype: Archetype::Pilot,
    cost: 20,
    health: 4,
    cargo_capacity: 2,
    accel: 300.0,
    brake: 420.0,
    turn_speed: 2.6,
    max_speed: 340.0,
    length: 40.0,
    width: 26.0,
    hit_radius: 16.0,
    weapon: Some(WeaponStats {
        cooldown_ticks: 96,
        bullet_speed: 260.0,
        damage: 25,
    }),
};

/// The mobile dropoff (DESIGN §5/§6): a Captain-archetype carrier-type hull.
/// Pushing it toward contested fields shortens your team's haul routes and
/// paints a target on you. Unarmed; its weapon is positioning.
const RESOURCE_CONTROLLER: HullStats = HullStats {
    archetype: Archetype::Captain,
    cost: 40,
    health: 12,
    cargo_capacity: 0,
    accel: 150.0,
    brake: 200.0,
    turn_speed: 0.0,
    max_speed: 140.0,
    length: 90.0,
    width: 90.0,
    hit_radius: 42.0,
    weapon: None,
};

/// The forward spawn point for combat hulls (DESIGN §5): the team's mobile
/// front line. Unarmed for now; escorting it is your team's job.
const STRIKE_CARRIER: HullStats = HullStats {
    archetype: Archetype::Captain,
    cost: 60,
    health: 20,
    cargo_capacity: 0,
    accel: 130.0,
    brake: 180.0,
    turn_speed: 0.0,
    max_speed: 115.0,
    length: 120.0,
    width: 120.0,
    hit_radius: 56.0,
    weapon: None,
};

/// The infrastructure carrier (DESIGN §6): dropoff + spawn host + the sole
/// builder of sub-carriers (outfitter, next slice). The strike carrier gets
/// the best hulls; the fleet carrier is where logistics lives.
const FLEET_CARRIER: HullStats = HullStats {
    archetype: Archetype::Captain,
    cost: 80,
    health: 30,
    cargo_capacity: 0,
    accel: 110.0,
    brake: 160.0,
    turn_speed: 0.0,
    max_speed: 95.0,
    length: 150.0,
    width: 150.0,
    hit_radius: 70.0,
    weapon: None,
};

/// The sub-carrier (DESIGN §6): built only AT a fleet carrier — the
/// ships-build-ships tree one level deeper. Not a spawn host; it exists to
/// be docked at: the sole source of outfitter-exclusive modules, pushed
/// forward as a mobile pit stop.
const OUTFITTER: HullStats = HullStats {
    archetype: Archetype::Captain,
    cost: 30,
    health: 8,
    cargo_capacity: 0,
    accel: 170.0,
    brake: 230.0,
    turn_speed: 0.0,
    max_speed: 160.0,
    length: 70.0,
    width: 70.0,
    hit_radius: 33.0,
    weapon: None,
};

pub fn stats(kind: HullKind) -> &'static HullStats {
    match kind {
        HullKind::Fighter => &FIGHTER,
        HullKind::Harvester => &HARVESTER,
        HullKind::Corvette => &CORVETTE,
        HullKind::Bomber => &BOMBER,
        HullKind::ResourceController => &RESOURCE_CONTROLLER,
        HullKind::StrikeCarrier => &STRIKE_CARRIER,
        HullKind::FleetCarrier => &FLEET_CARRIER,
        HullKind::Outfitter => &OUTFITTER,
    }
}

/// Hulls offered in the respawn menu, in display order.
pub const PURCHASABLE: [HullKind; 8] = [
    HullKind::Fighter,
    HullKind::Harvester,
    HullKind::Corvette,
    HullKind::Bomber,
    HullKind::ResourceController,
    HullKind::StrikeCarrier,
    HullKind::FleetCarrier,
    HullKind::Outfitter,
];

/// Hulls that host spawns for others (the mothership always does).
pub fn is_spawn_carrier(kind: HullKind) -> bool {
    matches!(kind, HullKind::StrikeCarrier | HullKind::FleetCarrier)
}

/// Where a hull may spawn (DESIGN §2 cold start / §6 spawn flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullClass {
    /// Fighter, harvester: spawn at the mothership or any friendly carrier —
    /// a team knocked back to nothing can always rebuild.
    Economy,
    /// Corvette and up: require a live friendly carrier.
    Combat,
    /// Carrier-type hulls: built at the mothership only.
    CarrierType,
    /// Sub-carriers (outfitter): built only at a fleet carrier.
    SubCarrier,
}

pub fn class(kind: HullKind) -> HullClass {
    match kind {
        HullKind::Fighter | HullKind::Harvester => HullClass::Economy,
        HullKind::Corvette | HullKind::Bomber => HullClass::Combat,
        HullKind::ResourceController | HullKind::StrikeCarrier | HullKind::FleetCarrier => {
            HullClass::CarrierType
        }
        HullKind::Outfitter => HullClass::SubCarrier,
    }
}

/// Points awarded for destroying a ship of this hull (DESIGN §5): scaled by
/// hull class so big-game hunting pays. Placeholder curve off the hull cost.
pub fn kill_bounty(kind: HullKind) -> u32 {
    4 + stats(kind).cost / 5
}

pub fn display_name(kind: HullKind) -> &'static str {
    match kind {
        HullKind::Fighter => "Fighter",
        HullKind::Harvester => "Harvester",
        HullKind::Corvette => "Corvette",
        HullKind::Bomber => "Bomber",
        HullKind::ResourceController => "Res. Controller",
        HullKind::StrikeCarrier => "Strike Carrier",
        HullKind::FleetCarrier => "Fleet Carrier",
        HullKind::Outfitter => "Outfitter",
    }
}

/// Hulls a ship can dock at for refits (the mothership always allows it).
pub fn is_dockable(kind: HullKind) -> bool {
    matches!(
        kind,
        HullKind::StrikeCarrier | HullKind::FleetCarrier | HullKind::Outfitter
    )
}
