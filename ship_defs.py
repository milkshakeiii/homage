from dataclasses import dataclass
from ursina import Vec3


@dataclass
class ShipDef:
    name: str
    mass: float              # kg — determines handling via F=ma
    thrust_force: float      # Newtons of main engine thrust
    rotation_force: float    # torque multiplier for pitch/yaw/roll
    max_speed: float         # speed cap (units/s)
    drag: float              # linear damping coefficient (0–1 per frame)
    hp: float
    shield: float
    model_scale: Vec3
    color_value: tuple       # (r, g, b, a) 0-255
    faction: str             # 'friendly' or 'enemy'
    weapons: list            # list of weapon definition dicts
    default_autopilot: str   # autopilot mode when AI-controlled


FIGHTER_DEF = ShipDef(
    name='Fighter',
    mass=10.0,
    thrust_force=200.0,
    rotation_force=8.0,
    max_speed=80.0,
    drag=0.02,
    hp=100.0,
    shield=50.0,
    model_scale=Vec3(1, 0.3, 1.5),
    color_value=(0, 220, 255, 255),      # cyan
    faction='friendly',
    weapons=[{'damage': 10, 'cooldown': 0.15, 'speed': 200, 'range': 300, 'color': (0, 255, 255)}],
    default_autopilot='keep_at_range',
)

CARRIER_DEF = ShipDef(
    name='Carrier',
    mass=5000.0,
    thrust_force=8000.0,       # 40x fighter thrust, but 500x mass → 12.5x slower accel
    rotation_force=400.0,
    max_speed=30.0,
    drag=0.01,
    hp=2000.0,
    shield=500.0,
    model_scale=Vec3(4, 2, 10),
    color_value=(150, 150, 165, 255),  # gray
    faction='friendly',
    weapons=[
        {'damage': 8, 'cooldown': 0.1, 'speed': 180, 'range': 350, 'color': (0, 255, 255)},
        {'damage': 30, 'cooldown': 0.8, 'speed': 120, 'range': 400, 'color': (255, 255, 0)},
    ],
    default_autopilot='keep_at_range',
)

ENEMY_FIGHTER_DEF = ShipDef(
    name='Enemy Fighter',
    mass=12.0,
    thrust_force=210.0,
    rotation_force=7.5,
    max_speed=75.0,
    drag=0.02,
    hp=80.0,
    shield=30.0,
    model_scale=Vec3(1, 0.3, 1.5),
    color_value=(255, 40, 25, 255),   # red
    faction='enemy',
    weapons=[{'damage': 8, 'cooldown': 0.2, 'speed': 190, 'range': 280, 'color': (255, 75, 25)}],
    default_autopilot='attack_run',
)
