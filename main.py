#!/usr/bin/env python3
"""Sci-Fi Spaceflight Simulator — main entry point."""

import sys
import os

# Ensure the spacesim directory is on the path for local imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from ursina import Ursina, window, color, time, application, Vec3, camera, scene

app = Ursina(
    title='Space Combat Simulator',
    borderless=False,
    fullscreen=False,
    development_mode=False,
)

# Disable simplepbr (it fails on macOS OpenGL <3.3) — use unlit rendering
try:
    from panda3d.core import LightRampAttrib, AmbientLight as P3DAmbient, DirectionalLight as P3DDir, NodePath
    # Remove any existing simplepbr shader
    scene.set_shader_auto(False) if hasattr(scene, 'set_shader_auto') else None
    camera.shader = None
    scene.shader = None
except Exception:
    pass

# Window setup — black space background
window.color = color.black
window.exit_button.visible = False
window.fps_counter.enabled = True

# Basic Panda3D lighting (no shaders needed)
from panda3d.core import AmbientLight as P3DAmbientLight, DirectionalLight as P3DDirectionalLight
ambient_node = P3DAmbientLight('ambient')
ambient_node.setColor((0.3, 0.3, 0.4, 1))
ambient_np = scene.attachNewNode(ambient_node)
scene.setLight(ambient_np)

sun_node = P3DDirectionalLight('sun')
sun_node.setColor((0.9, 0.85, 0.8, 1))
sun_np = scene.attachNewNode(sun_node)
sun_np.lookAt(1, -1, 1)
scene.setLight(sun_np)

# Import and create game manager (spawns the scene)
from game_manager import GameManager
gm = GameManager()


def update():
    dt = time.dt
    gm.update(dt)


def input(key):
    gm.input(key)


app.run()
