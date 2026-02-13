from ursina import Entity, Vec3, color, Mesh
from ship_defs import ShipDef


def _make_ship_mesh():
    """Simple wedge mesh — a pointed nose with a flat back."""
    # verts: nose, left-back, right-back, top-back, bottom-back
    verts = [
        (0, 0, 1),       # 0 nose
        (-0.5, 0, -0.5), # 1 left
        (0.5, 0, -0.5),  # 2 right
        (0, 0.25, -0.5), # 3 top
        (0, -0.15, -0.5),# 4 bottom
    ]
    tris = [
        (0, 3, 1),  # top-left
        (0, 2, 3),  # top-right
        (0, 1, 4),  # bottom-left
        (0, 4, 2),  # bottom-right
        (1, 3, 2),  # back-top
        (1, 2, 4),  # back-bottom
    ]
    return Mesh(vertices=verts, triangles=tris)


_ship_mesh = None


def _get_ship_mesh():
    global _ship_mesh
    if _ship_mesh is None:
        _ship_mesh = _make_ship_mesh()
    return _ship_mesh


class Ship(Entity):
    def __init__(self, ship_def: ShipDef, position=Vec3(0, 0, 0), **kwargs):
        c = ship_def.color_value
        super().__init__(
            model='cube',
            color=color.rgba(*c) if isinstance(c, tuple) else c,
            scale=ship_def.model_scale,
            position=position,
            **kwargs,
        )

        # === Definition ===
        self.ship_def = ship_def
        self.ship_name = ship_def.name
        self.faction = ship_def.faction

        # === Physics properties ===
        self.mass = ship_def.mass
        self.thrust_force = ship_def.thrust_force
        self.rotation_force = ship_def.rotation_force
        self.max_speed = ship_def.max_speed
        self.drag = ship_def.drag

        # === Virtual joystick — the core interface ===
        self.thrust_input = Vec3(0, 0, 0)    # local-space: x=strafe, y=vertical, z=forward
        self.rotation_input = Vec3(0, 0, 0)  # x=pitch, y=yaw, z=roll

        # === Flight state ===
        self.velocity = Vec3(0, 0, 0)

        # === Combat state ===
        self.hp = ship_def.hp
        self.max_hp = ship_def.hp
        self.shield = ship_def.shield
        self.max_shield = ship_def.shield
        self.alive = True

        # === Weapons (populated by game_manager) ===
        self.weapons = []

        # === Control state ===
        self.is_player_controlled = False
        self.autopilot_mode = None           # None or AutopilotMode enum value
        self.target = None                   # Ship reference for combat/autopilot

        # === Engine glow (visual) ===
        self.engine_glow = Entity(
            parent=self,
            model='cube',
            color=color.rgba(80, 150, 255, 180),
            scale=Vec3(0.3, 0.3, 0.1) / ship_def.model_scale,
            position=Vec3(0, 0, -0.55) / ship_def.model_scale,
        )
        self.engine_glow.visible = False

    @property
    def speed(self):
        return self.velocity.length() if self.velocity.length() > 0.001 else 0.0

    @property
    def forward_dir(self):
        return self.forward

    def take_damage(self, amount):
        if not self.alive:
            return
        # Shields absorb first
        if self.shield > 0:
            absorbed = min(self.shield, amount)
            self.shield -= absorbed
            amount -= absorbed
        if amount > 0:
            self.hp -= amount
        if self.hp <= 0:
            self.hp = 0
            self.alive = False

    def repair(self, hp_amount=0, shield_amount=0):
        self.hp = min(self.hp + hp_amount, self.max_hp)
        self.shield = min(self.shield + shield_amount, self.max_shield)
