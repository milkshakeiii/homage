from ursina import Vec3, camera, lerp, mouse, application


class ChaseCam:
    """Third-person chase camera with smooth follow and dynamic FOV."""
    def __init__(self):
        self.target_ship = None
        self.offset = Vec3(0, 4, -14)    # local-space offset behind and above
        self.look_offset = Vec3(0, 1, 8)  # point ahead of ship to look at
        self.follow_speed = 6.0
        self.fov_base = 75
        self.fov_speed_boost = 20        # max FOV increase at full speed
        self._transitioning = False
        self._transition_timer = 0
        self._transition_duration = 0.8
        self._transition_start_pos = Vec3(0, 0, 0)
        self._transition_start_rot = Vec3(0, 0, 0)

        # Disable Ursina default mouse behavior (only when window exists)
        if application.base and hasattr(application.base, 'win') and application.base.win:
            mouse.locked = True
            mouse.visible = False

    def set_target(self, ship, instant=False):
        """Switch which ship the camera follows."""
        if ship is self.target_ship:
            return
        if not instant and self.target_ship is not None:
            self._transitioning = True
            self._transition_timer = 0
            self._transition_start_pos = Vec3(*camera.position)
            self._transition_start_rot = Vec3(*camera.rotation)
        self.target_ship = ship
        if instant:
            self._snap_to_target()

    def _snap_to_target(self):
        if self.target_ship is None:
            return
        pos, look = self._compute_desired()
        camera.position = pos
        camera.look_at(look)

    def _compute_desired(self):
        ship = self.target_ship
        # Camera position: behind and above in ship's local space
        desired_pos = (
            ship.position
            + ship.back * abs(self.offset.z)
            + ship.up * self.offset.y
        )
        # Look-at point: ahead of ship
        look_point = (
            ship.position
            + ship.forward * self.look_offset.z
            + ship.up * self.look_offset.y
        )
        return desired_pos, look_point

    def update(self, dt):
        if self.target_ship is None or not self.target_ship.alive:
            return

        desired_pos, look_point = self._compute_desired()

        if self._transitioning:
            self._transition_timer += dt
            t = min(self._transition_timer / self._transition_duration, 1.0)
            # Smooth step
            t = t * t * (3 - 2 * t)
            camera.position = lerp(self._transition_start_pos, desired_pos, t)
            camera.look_at(look_point)
            if t >= 1.0:
                self._transitioning = False
        else:
            # Smooth follow
            camera.position = lerp(camera.position, desired_pos, self.follow_speed * dt)
            camera.look_at(look_point)

        # Dynamic FOV based on speed
        speed_ratio = self.target_ship.speed / max(self.target_ship.max_speed, 1)
        target_fov = self.fov_base + self.fov_speed_boost * speed_ratio
        camera.fov = lerp(camera.fov, target_fov, 3 * dt)
