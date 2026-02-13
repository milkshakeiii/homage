from ursina import Entity, Vec3, color, destroy, time


class Weapon:
    """A weapon mounted on a ship. Handles cooldown and firing."""
    def __init__(self, owner, damage=10, cooldown=0.2, speed=200, range=300, color_val=(0, 1, 1)):
        self.owner = owner
        self.damage = damage
        self.cooldown = cooldown
        self.speed = speed
        self.range = range
        self.color_val = color_val
        self._timer = 0.0

    def can_fire(self):
        return self._timer <= 0

    def update_cooldown(self, dt):
        if self._timer > 0:
            self._timer -= dt

    def fire(self, projectiles_list):
        if not self.can_fire():
            return None
        self._timer = self.cooldown
        p = Projectile(
            owner=self.owner,
            damage=self.damage,
            speed=self.speed,
            max_range=self.range,
            color_val=self.color_val,
        )
        projectiles_list.append(p)
        return p


class Projectile(Entity):
    """A fast-moving projectile with distance-based collision."""
    def __init__(self, owner, damage, speed, max_range, color_val, **kwargs):
        c = color_val
        super().__init__(
            model='cube',
            color=color.rgba(c[0], c[1], c[2], 230),
            scale=Vec3(0.1, 0.1, 0.6),
            position=owner.position + owner.forward * 2,
            **kwargs,
        )
        # Match owner's rotation so it looks right
        self.rotation = owner.rotation

        self.owner = owner
        self.faction = owner.faction
        self.damage = damage
        self.speed = speed
        self.max_range = max_range
        self.direction = owner.forward.normalized()
        self.distance_traveled = 0.0
        self.alive = True

    def update_projectile(self, dt, ships):
        """Move forward and check distance-based collision against ships."""
        if not self.alive:
            return

        move = self.direction * self.speed * dt
        self.position += move
        self.distance_traveled += move.length()

        # Range check
        if self.distance_traveled > self.max_range:
            self.alive = False
            return

        # Distance-based collision
        hit_radius = 3.0
        for ship in ships:
            if not ship.alive:
                continue
            if ship.faction == self.faction:
                continue
            if ship is self.owner:
                continue
            dist = (self.position - ship.position).length()
            # Scale hit radius by ship size
            ship_radius = max(ship.scale_x, ship.scale_y, ship.scale_z) * 0.6
            if dist < hit_radius + ship_radius:
                self.alive = False
                return ship  # Return the hit ship
        return None
