from ursina import Entity, Vec3, color, destroy, time, invoke
import random


def apply_damage(ship, amount):
    """Apply damage to a ship (shields first, then HP). Returns True if ship was destroyed."""
    if not ship.alive:
        return False
    ship.take_damage(amount)
    # Flash the ship white on hit
    _flash_hit(ship)
    if not ship.alive:
        spawn_explosion(ship.position, ship.scale)
        ship.visible = False
        ship.engine_glow.visible = False
        return True
    return False


def _flash_hit(ship):
    """Brief white flash on hit."""
    original_color = ship.color
    ship.color = color.white
    invoke(setattr, ship, 'color', original_color, delay=0.05)


def spawn_explosion(position, scale, num_debris=8):
    """Create a simple explosion effect with flying debris cubes."""
    for _ in range(num_debris):
        size = random.uniform(0.2, 0.8) * max(scale.x, scale.y, scale.z) * 0.3
        debris = Entity(
            model='cube',
            color=color.rgba(
                random.randint(200, 255),
                random.randint(75, 180),
                random.randint(0, 50),
                255,
            ),
            scale=size,
            position=position + Vec3(
                random.uniform(-1, 1),
                random.uniform(-1, 1),
                random.uniform(-1, 1),
            ) * max(scale.x, scale.z) * 0.3,
        )
        # Animate outward and fade
        end_pos = debris.position + Vec3(
            random.uniform(-1, 1),
            random.uniform(-1, 1),
            random.uniform(-1, 1),
        ).normalized() * random.uniform(5, 20)
        duration = random.uniform(0.5, 1.5)
        debris.animate_position(end_pos, duration=duration, curve='out_expo')
        debris.animate_scale(0, duration=duration, curve='in_expo')
        destroy(debris, delay=duration + 0.1)

    # Central flash
    flash = Entity(
        model='cube',
        color=color.rgba(255, 230, 130, 230),
        scale=max(scale.x, scale.z) * 1.5,
        position=position,
    )
    flash.animate_scale(0, duration=0.3, curve='out_expo')
    destroy(flash, delay=0.4)
