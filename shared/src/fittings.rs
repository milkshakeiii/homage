//! The fitting catalog (DESIGN §5): points buy permanent-for-the-match
//! unlocks; equipping happens on the spawn screen, limited to what the spawn
//! facility stocks. Every entry here is implemented — no placeholder rows.
//!
//! Stocking deviations from the DESIGN table (until the outfitter hull and
//! alive-docking exist): outfitter-stocked items live at the strike carrier,
//! resource-controller-stocked items at any carrier.

use crate::protocol::{FittingId, HullKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Weapon,
    Utility,
    HullMod,
}

/// Where a fitting can be equipped from (spawn facility kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stocking {
    Everywhere,
    AnyCarrier,
    StrikeCarrierOnly,
}

/// The facility a spawn happens from, for stocking checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpawnFacility {
    Mothership,
    StrikeCarrier,
    FleetCarrier,
}

pub fn stocked_at(stocking: Stocking, facility: SpawnFacility) -> bool {
    match stocking {
        Stocking::Everywhere => true,
        Stocking::AnyCarrier => matches!(
            facility,
            SpawnFacility::StrikeCarrier | SpawnFacility::FleetCarrier
        ),
        // The strike/fleet asymmetry (DESIGN §6): the best weapons and
        // frames come off the strike carrier only.
        Stocking::StrikeCarrierOnly => facility == SpawnFacility::StrikeCarrier,
    }
}

pub struct FittingDef {
    pub id: FittingId,
    pub name: &'static str,
    pub slot: Slot,
    /// Points to unlock (0 = always unlocked).
    pub cost: u32,
    pub stocking: Stocking,
    pub blurb: &'static str,
}

pub const CATALOG: [FittingDef; 9] = [
    FittingDef { id: FittingId::PulseCannon, name: "Pulse Cannon", slot: Slot::Weapon, cost: 0, stocking: Stocking::Everywhere,
        blurb: "Standard-issue autocannon. Reliable, unremarkable, yours." },
    FittingDef { id: FittingId::ScatterGun, name: "Scatter Gun", slot: Slot::Weapon, cost: 8, stocking: Stocking::AnyCarrier,
        blurb: "Three-pellet spread, short reach. Ruins knife fights for the other guy." },
    FittingDef { id: FittingId::LongLance, name: "Long-Lance Railgun", slot: Slot::Weapon, cost: 20, stocking: Stocking::StrikeCarrierOnly,
        blurb: "Slow cycle, extreme velocity. Leading shots become sniping." },
    FittingDef { id: FittingId::Afterburner, name: "Afterburner", slot: Slot::Utility, cost: 10, stocking: Stocking::Everywhere,
        blurb: "Hold SHIFT: +50% thrust, +25% top speed. Heat limits arrive later." },
    FittingDef { id: FittingId::BlinkThruster, name: "Blink Thruster", slot: Slot::Utility, cost: 25, stocking: Stocking::StrikeCarrierOnly,
        blurb: "SHIFT: an instant impulse along your nose, 3s cooldown. Be somewhere else." },
    FittingDef { id: FittingId::GyroTuning, name: "Gyro Tuning", slot: Slot::HullMod, cost: 8, stocking: Stocking::Everywhere,
        blurb: "+25% turn rate. The cheapest way to feel better." },
    FittingDef { id: FittingId::ArmorPlate, name: "Armor Plate", slot: Slot::HullMod, cost: 8, stocking: Stocking::Everywhere,
        blurb: "+2 hull, -10% thrust. Trade dance for endurance." },
    FittingDef { id: FittingId::LightweightFrame, name: "Lightweight Frame", slot: Slot::HullMod, cost: 15, stocking: Stocking::StrikeCarrierOnly,
        blurb: "+15% thrust and top speed, -1 hull. Speed is armor, allegedly." },
    FittingDef { id: FittingId::CompactedHold, name: "Compacted Hold", slot: Slot::HullMod, cost: 15, stocking: Stocking::AnyCarrier,
        blurb: "+50% cargo capacity, -15% top speed. One more rock." },
];

pub fn def(id: FittingId) -> &'static FittingDef {
    CATALOG.iter().find(|d| d.id == id).expect("fitting in catalog")
}

/// Weapon behavior per fitting, scaled from the hull's base weapon.
pub struct WeaponProfile {
    pub cooldown_ticks: u16,
    pub bullet_speed: f32,
    pub pellets: u8,
    /// Radians between adjacent pellets.
    pub spread: f32,
    /// Multiplier on BULLET_LIFETIME_TICKS.
    pub lifetime_mult: f32,
    pub damage: u16,
}

pub fn weapon_profile(weapon: FittingId, hull: HullKind) -> Option<WeaponProfile> {
    let base = crate::hulls::stats(hull).weapon?;
    Some(match weapon {
        FittingId::ScatterGun => WeaponProfile {
            cooldown_ticks: (base.cooldown_ticks as f32 * 1.6) as u16,
            bullet_speed: base.bullet_speed * 0.85,
            pellets: 3,
            spread: 0.11,
            lifetime_mult: 0.5,
            damage: 1,
        },
        FittingId::LongLance => WeaponProfile {
            cooldown_ticks: (base.cooldown_ticks as f32 * 2.4) as u16,
            bullet_speed: base.bullet_speed * 1.9,
            pellets: 1,
            spread: 0.0,
            lifetime_mult: 1.2,
            damage: 1,
        },
        // Pulse cannon and anything unexpected: the hull's stock weapon.
        _ => WeaponProfile {
            cooldown_ticks: base.cooldown_ticks,
            bullet_speed: base.bullet_speed,
            pellets: 1,
            spread: 0.0,
            lifetime_mult: 1.0,
            damage: base.damage,
        },
    })
}

/// Passive movement/durability modifiers from the hull-mod slot.
pub struct HullModEffects {
    pub accel_mult: f32,
    pub max_speed_mult: f32,
    pub turn_mult: f32,
    pub health_bonus: i32,
    pub cargo_mult: f32,
}

impl Default for HullModEffects {
    fn default() -> Self {
        Self {
            accel_mult: 1.0,
            max_speed_mult: 1.0,
            turn_mult: 1.0,
            health_bonus: 0,
            cargo_mult: 1.0,
        }
    }
}

pub fn hull_mod_effects(hull_mod: Option<FittingId>) -> HullModEffects {
    match hull_mod {
        Some(FittingId::GyroTuning) => HullModEffects {
            turn_mult: 1.25,
            ..Default::default()
        },
        Some(FittingId::ArmorPlate) => HullModEffects {
            health_bonus: 2,
            accel_mult: 0.9,
            ..Default::default()
        },
        Some(FittingId::LightweightFrame) => HullModEffects {
            accel_mult: 1.15,
            max_speed_mult: 1.15,
            health_bonus: -1,
            ..Default::default()
        },
        Some(FittingId::CompactedHold) => HullModEffects {
            cargo_mult: 1.5,
            max_speed_mult: 0.85,
            ..Default::default()
        },
        _ => HullModEffects::default(),
    }
}

/// Afterburner effect while the ability key is held.
pub const AFTERBURNER_ACCEL_MULT: f32 = 1.5;
pub const AFTERBURNER_SPEED_MULT: f32 = 1.25;
/// Blink: instantaneous velocity impulse along the nose, and its cooldown.
pub const BLINK_IMPULSE: f32 = 320.0;
pub const BLINK_COOLDOWN_TICKS: u16 = 192;
