from ursina import Entity, Vec3, color, camera
import random


class Starfield:
    """Background stars that surround the camera, creating an illusion of deep space."""
    def __init__(self, num_stars=600, radius=500):
        self.stars = []
        self.radius = radius
        for _ in range(num_stars):
            pos = Vec3(
                random.uniform(-1, 1),
                random.uniform(-1, 1),
                random.uniform(-1, 1),
            ).normalized() * random.uniform(radius * 0.5, radius)

            brightness = random.randint(120, 255)
            blue_tint = random.randint(int(brightness * 0.8), brightness)
            size = random.uniform(0.3, 1.2)

            star = Entity(
                model='quad',
                color=color.rgba(brightness, brightness, blue_tint, 255),
                scale=size,
                position=pos,
                billboard=True,
                unlit=True,
            )
            self.stars.append(star)

    def update(self):
        """Keep stars centered on the camera so they appear infinitely far away."""
        cam_pos = camera.position
        for star in self.stars:
            offset = star.position - cam_pos
            if offset.length() > self.radius * 1.2:
                star.position = cam_pos - offset.normalized() * self.radius * random.uniform(0.8, 1.0)
