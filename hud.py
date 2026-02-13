from ursina import Text, Vec2, Entity, color, camera, window


HELP_TEXT = """=== SPACE COMBAT SIMULATOR ===

  FLIGHT
    W / S         Forward / Reverse thrust
    A / D         Strafe left / right
    Space / Shift Thrust up / down
    Mouse         Aim (pitch / yaw)
    Q / E         Roll left / right

  COMBAT
    Left Click    Fire weapons
    T             Cycle target

  AUTOPILOT
    1  Intercept     Fly at target
    2  Evade         Fly away, jink
    3  Keep Range    Hold distance
    4  Orbit         Circle target
    5  Attack Run    Strafe passes
    ESC            Autopilot off

  OTHER
    TAB            Switch ship
    H              Toggle this help
    P              Unlock mouse
    F11            Toggle fullscreen
    Ctrl+Q         Quit
"""


class HUD:
    """Screen-space text overlay showing flight and combat info."""
    def __init__(self):
        base_y = 0.45
        left_x = -0.72
        line_h = 0.045

        self.speed_text = Text(
            text='SPD: 0',
            position=Vec2(left_x, base_y),
            scale=1.2,
            color=color.cyan,
        )
        self.hp_text = Text(
            text='HP: 100',
            position=Vec2(left_x, base_y - line_h),
            scale=1.2,
            color=color.green,
        )
        self.shield_text = Text(
            text='SLD: 50',
            position=Vec2(left_x, base_y - line_h * 2),
            scale=1.2,
            color=color.azure,
        )
        self.ship_text = Text(
            text='SHIP: Fighter',
            position=Vec2(left_x, base_y - line_h * 3),
            scale=1.0,
            color=color.white,
        )
        self.target_text = Text(
            text='TGT: None',
            position=Vec2(left_x, base_y - line_h * 4),
            scale=1.0,
            color=color.yellow,
        )
        self.autopilot_text = Text(
            text='AP: OFF',
            position=Vec2(left_x, base_y - line_h * 5),
            scale=1.2,
            color=color.orange,
        )
        # Center crosshair
        self.crosshair = Text(
            text='+',
            origin=(0, 0),
            scale=2,
            color=color.rgba(255, 255, 255, 128),
        )
        # Brief hint at bottom
        self.controls_text = Text(
            text='H : Controls help    P : Unlock mouse    Ctrl+Q : Quit',
            position=Vec2(0, -0.46),
            origin=(0, 0),
            scale=0.75,
            color=color.rgba(255, 255, 255, 100),
        )

        # === Help overlay (hidden by default) ===
        self.help_bg = Entity(
            parent=camera.ui,
            model='quad',
            color=color.rgba(0, 0, 0, 200),
            scale=(1.0, 0.85),
            position=Vec2(0, 0),
            z=-1,
            visible=False,
        )
        self.help_text = Text(
            text=HELP_TEXT,
            position=Vec2(-0.3, 0.38),
            scale=0.9,
            color=color.rgba(200, 255, 200, 255),
            visible=False,
        )
        self.help_visible = False

    def toggle_help(self):
        self.help_visible = not self.help_visible
        self.help_bg.visible = self.help_visible
        self.help_text.visible = self.help_visible

    def update(self, ship, autopilot_mode_name):
        if ship is None:
            return

        self.speed_text.text = f'SPD: {ship.speed:.0f}'
        self.hp_text.text = f'HP: {ship.hp:.0f}/{ship.max_hp:.0f}'
        self.shield_text.text = f'SLD: {ship.shield:.0f}/{ship.max_shield:.0f}'
        self.ship_text.text = f'SHIP: {ship.ship_name}'

        # HP color
        hp_ratio = ship.hp / max(ship.max_hp, 1)
        if hp_ratio > 0.5:
            self.hp_text.color = color.green
        elif hp_ratio > 0.25:
            self.hp_text.color = color.yellow
        else:
            self.hp_text.color = color.red

        # Shield color
        if ship.shield > 0:
            self.shield_text.color = color.azure
        else:
            self.shield_text.color = color.gray

        # Target
        if ship.target and ship.target.alive:
            dist = (ship.target.position - ship.position).length()
            self.target_text.text = f'TGT: {ship.target.ship_name} [{dist:.0f}m]'
            self.target_text.color = color.yellow
        else:
            self.target_text.text = 'TGT: None'
            self.target_text.color = color.gray

        # Autopilot
        self.autopilot_text.text = f'AP: {autopilot_mode_name}'
        if autopilot_mode_name == 'OFF':
            self.autopilot_text.color = color.gray
        else:
            self.autopilot_text.color = color.orange
