from ursina import Vec3, time, color


def update_ship_physics(ship, dt):
    """Apply the virtual joystick inputs to ship physics.

    thrust_input (local space):  x = strafe, y = vertical, z = forward/back
    rotation_input:              x = pitch,  y = yaw,      z = roll
    """
    if not ship.alive:
        return

    # --- Rotation ---
    rot = ship.rotation_input
    rot_speed = ship.rotation_force / ship.mass  # heavier ships turn slower
    ship.rotation_x -= rot.x * rot_speed * dt * 60  # pitch
    ship.rotation_y -= rot.y * rot_speed * dt * 60  # yaw
    ship.rotation_z -= rot.z * rot_speed * dt * 60  # roll

    # --- Thrust → Acceleration (F = ma) ---
    t = ship.thrust_input
    # Convert local-space thrust to world-space force
    world_thrust = (
        ship.forward * t.z +
        ship.right * t.x +
        ship.up * t.y
    ) * ship.thrust_force

    acceleration = world_thrust / ship.mass
    ship.velocity += acceleration * dt

    # --- Drag (velocity damping) ---
    ship.velocity *= (1.0 - ship.drag)

    # --- Speed cap ---
    spd = ship.velocity.length()
    if spd > ship.max_speed:
        ship.velocity = ship.velocity.normalized() * ship.max_speed

    # --- Position update ---
    ship.position += ship.velocity * dt

    # --- Engine glow ---
    thrust_magnitude = abs(t.z) + abs(t.x) + abs(t.y)
    ship.engine_glow.visible = thrust_magnitude > 0.05
    if ship.engine_glow.visible:
        glow_intensity = min(thrust_magnitude, 1.0)
        ship.engine_glow.color = color.rgba(
            int(80 + 130 * glow_intensity),
            int(130 + 80 * glow_intensity),
            255,
            int(100 + 155 * glow_intensity),
        )
