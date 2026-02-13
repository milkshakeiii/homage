from ursina import Vec3, held_keys, mouse, time, destroy, window, application
from ship import Ship
from ship_defs import FIGHTER_DEF, CARRIER_DEF, ENEMY_FIGHTER_DEF
from physics import update_ship_physics
from autopilot import (
    AutopilotMode, MODE_BY_KEY, MODE_NAMES, run_autopilot,
)
from weapons import Weapon
from combat import apply_damage
from camera_rig import ChaseCam
from hud import HUD
from starfield import Starfield


class GameManager:
    def __init__(self):
        # === Ships ===
        self.all_ships = []
        self.friendly_ships = []
        self.enemy_ships = []
        self.projectiles = []

        # === Player state ===
        self.player_ship_index = 0
        self.player_ship = None
        self.mouse_sensitivity = 0.15

        # === Systems ===
        self.chase_cam = ChaseCam()
        self.hud = HUD()
        self.starfield = Starfield()

        # === Spawn initial scene ===
        self._spawn_scene()

    def _spawn_scene(self):
        # Player fighter
        player = Ship(FIGHTER_DEF, position=Vec3(0, 0, 0))
        player.is_player_controlled = True
        self.all_ships.append(player)
        self.friendly_ships.append(player)

        # Friendly carrier
        carrier = Ship(CARRIER_DEF, position=Vec3(30, -5, -40))
        carrier.autopilot_mode = AutopilotMode.KEEP_AT_RANGE
        self.all_ships.append(carrier)
        self.friendly_ships.append(carrier)

        # Enemy fighters
        enemy_positions = [
            Vec3(100, 20, 150),
            Vec3(-80, -10, 180),
            Vec3(50, 40, 200),
        ]
        for pos in enemy_positions:
            enemy = Ship(ENEMY_FIGHTER_DEF, position=pos)
            enemy.autopilot_mode = AutopilotMode.ATTACK_RUN
            self.all_ships.append(enemy)
            self.enemy_ships.append(enemy)

        # Assign weapons to all ships
        for ship in self.all_ships:
            for wdef in ship.ship_def.weapons:
                w = Weapon(
                    owner=ship,
                    damage=wdef['damage'],
                    cooldown=wdef['cooldown'],
                    speed=wdef['speed'],
                    range=wdef['range'],
                    color_val=wdef['color'],
                )
                ship.weapons.append(w)

        # Assign targets
        self.player_ship = player
        self._assign_targets()
        self.chase_cam.set_target(player, instant=True)

    def _assign_targets(self):
        """Give every ship a target from the opposing faction."""
        # Friendlies target nearest enemy
        for ship in self.friendly_ships:
            ship.target = self._nearest_enemy(ship, self.enemy_ships)
        # Enemies target nearest friendly
        for ship in self.enemy_ships:
            ship.target = self._nearest_enemy(ship, self.friendly_ships)

    def _nearest_enemy(self, ship, enemy_list):
        best = None
        best_dist = float('inf')
        for e in enemy_list:
            if not e.alive:
                continue
            d = (e.position - ship.position).length()
            if d < best_dist:
                best_dist = d
                best = e
        return best

    def update(self, dt):
        """Master update — called every frame from main.py."""
        # 1. Player input
        self._handle_player_input(dt)

        # 2. Autopilot AI for non-player ships (and player ship if AP is on)
        for ship in self.all_ships:
            if not ship.alive:
                continue
            if ship.is_player_controlled and ship.autopilot_mode is None:
                continue  # player is flying manually
            if ship.autopilot_mode is not None:
                run_autopilot(ship, dt)

        # 3. Physics for all ships
        for ship in self.all_ships:
            if ship.alive:
                update_ship_physics(ship, dt)

        # 4. Weapon cooldowns
        for ship in self.all_ships:
            for weapon in ship.weapons:
                weapon.update_cooldown(dt)

        # 5. AI firing
        self._ai_fire(dt)

        # 6. Projectile updates
        self._update_projectiles(dt)

        # 7. Reassign targets periodically (enemies may have died)
        self._reassign_dead_targets()

        # 8. Camera
        self.chase_cam.update(dt)

        # 9. Starfield
        self.starfield.update()

        # 10. HUD
        ap_name = MODE_NAMES.get(self.player_ship.autopilot_mode, 'OFF') if self.player_ship else 'OFF'
        self.hud.update(self.player_ship, ap_name)

    def _handle_player_input(self, dt):
        ship = self.player_ship
        if ship is None or not ship.alive:
            return

        # --- Thrust (WASD + Space/Shift) ---
        thrust = Vec3(0, 0, 0)
        if held_keys['w']:
            thrust.z += 1
        if held_keys['s']:
            thrust.z -= 1
        if held_keys['a']:
            thrust.x -= 1
        if held_keys['d']:
            thrust.x += 1
        if held_keys['space']:
            thrust.y += 1
        if held_keys['shift'] or held_keys['left shift']:
            thrust.y -= 1

        # --- Rotation from mouse ---
        rot = Vec3(0, 0, 0)
        rot.x = mouse.velocity[1] * self.mouse_sensitivity * -600  # pitch
        rot.y = mouse.velocity[0] * self.mouse_sensitivity * 600   # yaw
        if held_keys['q']:
            rot.z += 1
        if held_keys['e']:
            rot.z -= 1

        # --- Manual override: any flight input cancels autopilot ---
        has_input = thrust.length() > 0.01 or abs(rot.x) > 0.01 or abs(rot.y) > 0.01 or abs(rot.z) > 0.01
        if has_input and ship.autopilot_mode is not None:
            ship.autopilot_mode = None

        # --- Write to virtual joystick (only if not on autopilot) ---
        if ship.autopilot_mode is None:
            ship.thrust_input = thrust
            ship.rotation_input = rot

        # --- Fire weapons ---
        if held_keys['left mouse'] or mouse.left:
            for weapon in ship.weapons:
                weapon.fire(self.projectiles)

    def input(self, key):
        """Handle discrete key presses (called by Ursina)."""
        # TAB — switch ship
        if key == 'tab':
            self._switch_ship()

        # T — cycle target
        if key == 't':
            self._cycle_target()

        # 1-5 — toggle autopilot mode
        if key in MODE_BY_KEY:
            mode = MODE_BY_KEY[key]
            if self.player_ship and self.player_ship.alive:
                if self.player_ship.autopilot_mode == mode:
                    self.player_ship.autopilot_mode = None  # toggle off
                else:
                    self.player_ship.autopilot_mode = mode

        # ESC — autopilot off
        if key == 'escape':
            if self.player_ship:
                self.player_ship.autopilot_mode = None

        # H — toggle help overlay
        if key == 'h':
            self.hud.toggle_help()

        # P — toggle mouse lock (so you can click X to close, etc.)
        if key == 'p':
            mouse.locked = not mouse.locked
            mouse.visible = not mouse.locked

        # F11 — toggle fullscreen
        if key == 'f11':
            window.fullscreen = not window.fullscreen

        # Ctrl+Q — quit
        if key == 'q' and (held_keys['control'] or held_keys['left control']):
            application.quit()

    def _switch_ship(self):
        """TAB: cycle through friendly ships."""
        if len(self.friendly_ships) < 2:
            return
        alive_friendly = [s for s in self.friendly_ships if s.alive]
        if len(alive_friendly) < 2:
            return

        # Remove player control from current ship
        if self.player_ship:
            self.player_ship.is_player_controlled = False
            # Give it a default autopilot
            self.player_ship.autopilot_mode = AutopilotMode.KEEP_AT_RANGE
            self.player_ship.thrust_input = Vec3(0, 0, 0)
            self.player_ship.rotation_input = Vec3(0, 0, 0)

        # Find next alive friendly
        current_idx = alive_friendly.index(self.player_ship) if self.player_ship in alive_friendly else 0
        next_idx = (current_idx + 1) % len(alive_friendly)
        new_ship = alive_friendly[next_idx]

        new_ship.is_player_controlled = True
        new_ship.autopilot_mode = None
        new_ship.thrust_input = Vec3(0, 0, 0)
        new_ship.rotation_input = Vec3(0, 0, 0)
        self.player_ship = new_ship
        self.chase_cam.set_target(new_ship)

    def _cycle_target(self):
        """T: cycle through enemy targets."""
        ship = self.player_ship
        if not ship:
            return
        enemies = [e for e in self.enemy_ships if e.alive]
        if not enemies:
            ship.target = None
            return
        if ship.target in enemies:
            idx = enemies.index(ship.target)
            ship.target = enemies[(idx + 1) % len(enemies)]
        else:
            ship.target = enemies[0]

    def _ai_fire(self, dt):
        """Non-player ships fire at their targets when aimed."""
        for ship in self.all_ships:
            if not ship.alive or ship.is_player_controlled:
                continue
            if ship.target is None or not ship.target.alive:
                continue
            # Check if target is roughly in front
            to_target = (ship.target.position - ship.position)
            dist = to_target.length()
            if dist < 1:
                continue
            dot = ship.forward.dot(to_target.normalized())
            # Fire if aimed within ~15 degrees and in range
            if dot > 0.96 and dist < ship.weapons[0].range if ship.weapons else False:
                for weapon in ship.weapons:
                    weapon.fire(self.projectiles)

    def _update_projectiles(self, dt):
        """Move projectiles and handle hits."""
        to_remove = []
        for proj in self.projectiles:
            hit_ship = proj.update_projectile(dt, self.all_ships)
            if hit_ship is not None:
                destroyed = apply_damage(hit_ship, proj.damage)
            if not proj.alive:
                to_remove.append(proj)

        for proj in to_remove:
            self.projectiles.remove(proj)
            destroy(proj)

    def _reassign_dead_targets(self):
        """If a ship's target is dead, find a new one."""
        for ship in self.friendly_ships:
            if ship.alive and (ship.target is None or not ship.target.alive):
                ship.target = self._nearest_enemy(ship, self.enemy_ships)
        for ship in self.enemy_ships:
            if ship.alive and (ship.target is None or not ship.target.alive):
                ship.target = self._nearest_enemy(ship, self.friendly_ships)
