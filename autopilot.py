from enum import Enum
from ursina import Vec3
import math


class AutopilotMode(Enum):
    INTERCEPT = 'intercept'
    EVADE = 'evade'
    KEEP_AT_RANGE = 'keep_at_range'
    ORBIT = 'orbit'
    ATTACK_RUN = 'attack_run'


MODE_BY_KEY = {
    '1': AutopilotMode.INTERCEPT,
    '2': AutopilotMode.EVADE,
    '3': AutopilotMode.KEEP_AT_RANGE,
    '4': AutopilotMode.ORBIT,
    '5': AutopilotMode.ATTACK_RUN,
}

MODE_NAMES = {
    AutopilotMode.INTERCEPT: 'Intercept',
    AutopilotMode.EVADE: 'Evade',
    AutopilotMode.KEEP_AT_RANGE: 'Keep Range',
    AutopilotMode.ORBIT: 'Orbit',
    AutopilotMode.ATTACK_RUN: 'Attack Run',
}


def _aim_at(ship, target_pos, dt):
    """Compute rotation_input to aim ship's forward toward target_pos."""
    to_target = (target_pos - ship.position)
    if to_target.length() < 0.01:
        return Vec3(0, 0, 0)
    to_target = to_target.normalized()

    # Decompose into local-space pitch and yaw
    local_forward = ship.forward
    local_right = ship.right
    local_up = ship.up

    # Yaw: how far off are we horizontally?
    dot_right = to_target.dot(local_right)
    dot_forward = to_target.dot(local_forward)

    # Pitch: how far off are we vertically?
    dot_up = to_target.dot(local_up)

    yaw = _clamp(dot_right * 2.0, -1, 1)
    pitch = _clamp(-dot_up * 2.0, -1, 1)

    return Vec3(pitch, yaw, 0)


def _clamp(v, lo, hi):
    return max(lo, min(hi, v))


def run_autopilot(ship, dt):
    """Execute the ship's current autopilot mode. Writes to thrust_input and rotation_input."""
    if ship.autopilot_mode is None or ship.target is None or not ship.target.alive:
        ship.thrust_input = Vec3(0, 0, 0)
        ship.rotation_input = Vec3(0, 0, 0)
        return

    mode = ship.autopilot_mode
    target = ship.target
    to_target = target.position - ship.position
    dist = to_target.length()

    if mode == AutopilotMode.INTERCEPT:
        _intercept(ship, target, to_target, dist, dt)
    elif mode == AutopilotMode.EVADE:
        _evade(ship, target, to_target, dist, dt)
    elif mode == AutopilotMode.KEEP_AT_RANGE:
        _keep_at_range(ship, target, to_target, dist, dt)
    elif mode == AutopilotMode.ORBIT:
        _orbit(ship, target, to_target, dist, dt)
    elif mode == AutopilotMode.ATTACK_RUN:
        _attack_run(ship, target, to_target, dist, dt)


def _intercept(ship, target, to_target, dist, dt):
    """Fly straight at the target, full thrust."""
    # Lead the target based on closing velocity
    lead_pos = target.position + target.velocity * min(dist / max(ship.max_speed, 1), 2.0)
    ship.rotation_input = _aim_at(ship, lead_pos, dt)
    # Thrust forward once reasonably aimed
    dot = ship.forward.dot((lead_pos - ship.position).normalized()) if dist > 1 else 1
    ship.thrust_input = Vec3(0, 0, max(dot, 0.3))


def _evade(ship, target, to_target, dist, dt):
    """Fly away from the target with evasive jinking."""
    away_pos = ship.position - to_target.normalized() * 100
    # Add some perpendicular offset for jinking
    import time as _time
    jink = math.sin(_time.time() * 3) * 40
    away_pos += ship.right * jink
    ship.rotation_input = _aim_at(ship, away_pos, dt)
    ship.thrust_input = Vec3(0, 0, 1)


def _keep_at_range(ship, target, to_target, dist, dt, desired_range=150):
    """Maintain a specific distance from target."""
    ship.rotation_input = _aim_at(ship, target.position, dt)
    if dist < desired_range * 0.7:
        # Too close, back off (reverse)
        ship.thrust_input = Vec3(0, 0, -0.6)
    elif dist > desired_range * 1.3:
        # Too far, close in
        ship.thrust_input = Vec3(0, 0, 0.8)
    else:
        # Comfortable range, strafe a bit
        import time as _time
        strafe = math.sin(_time.time() * 2) * 0.4
        ship.thrust_input = Vec3(strafe, 0, 0.1)


def _orbit(ship, target, to_target, dist, dt, orbit_radius=100):
    """Circle around the target at a set radius."""
    if dist < 1:
        ship.thrust_input = Vec3(1, 0, 0)
        return

    # Aim perpendicular to the line-to-target (orbit direction)
    to_target_norm = to_target.normalized()
    # Choose orbit direction: cross with world up, fallback to ship up
    orbit_dir = to_target_norm.cross(Vec3(0, 1, 0))
    if orbit_dir.length() < 0.1:
        orbit_dir = to_target_norm.cross(Vec3(1, 0, 0))
    orbit_dir = orbit_dir.normalized()

    # Blend orbit direction with approach/retreat to maintain radius
    radius_error = (dist - orbit_radius) / orbit_radius
    blend_approach = _clamp(radius_error, -0.5, 0.5)
    aim_point = target.position + orbit_dir * orbit_radius * 0.5
    if radius_error > 0.2:
        aim_point = target.position  # close in
    elif radius_error < -0.2:
        aim_point = ship.position + orbit_dir * 100  # swing wide

    ship.rotation_input = _aim_at(ship, aim_point, dt)
    ship.thrust_input = Vec3(0, 0, 0.7 + blend_approach * 0.3)


def _attack_run(ship, target, to_target, dist, dt):
    """Intercept → fire → break away → repeat. State stored on ship."""
    if not hasattr(ship, '_attack_run_phase'):
        ship._attack_run_phase = 'approach'
        ship._attack_run_timer = 0

    phase = ship._attack_run_phase

    if phase == 'approach':
        # Fly at target
        _intercept(ship, target, to_target, dist, dt)
        # Switch to break when close
        if dist < 60:
            ship._attack_run_phase = 'break'
            ship._attack_run_timer = 0
            ship._attack_run_break_dir = ship.right if math.sin(id(ship)) > 0 else -ship.right

    elif phase == 'break':
        # Break away after passing
        ship._attack_run_timer += dt
        break_pos = ship.position + ship.forward * 50 + ship._attack_run_break_dir * 80 + Vec3(0, 20, 0)
        ship.rotation_input = _aim_at(ship, break_pos, dt)
        ship.thrust_input = Vec3(0, 0, 1)
        if ship._attack_run_timer > 2.5:
            ship._attack_run_phase = 'reengage'
            ship._attack_run_timer = 0

    elif phase == 'reengage':
        # Turn back toward target
        ship._attack_run_timer += dt
        ship.rotation_input = _aim_at(ship, target.position, dt)
        ship.thrust_input = Vec3(0, 0, 0.6)
        if ship._attack_run_timer > 1.5 or dist > 200:
            ship._attack_run_phase = 'approach'
