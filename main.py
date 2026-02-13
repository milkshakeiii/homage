#!/usr/bin/env python3
"""Sci-Fi Spaceflight Simulator — main entry point."""

import sys
import os
import types

# Ensure the script directory is on the path for local imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# === macOS OpenGL 2.1 compatibility ===
# macOS only provides OpenGL 2.1 (GLSL 120) in compatibility mode.
# Ursina's default entity shader (unlit_with_fog_shader) requires GLSL 130/140
# which fails silently, making all 3D objects invisible.
# Fix: fake out simplepbr AND disable the default entity shader.

fake_simplepbr = types.ModuleType('simplepbr')
fake_simplepbr.init = lambda **kwargs: None
sys.modules['simplepbr'] = fake_simplepbr

from ursina import Ursina, window, color, time, application, Vec3, camera, scene, Entity

# Disable the default shader on ALL entities — use fixed-function pipeline
Entity.default_shader = None

app = Ursina(
    title='Space Combat Simulator',
    borderless=False,
    fullscreen=False,
    development_mode=False,
)

# Window setup — black space background
window.color = color.black
window.exit_button.visible = False
window.fps_counter.enabled = True

# Basic Panda3D fixed-function lighting
from panda3d.core import AmbientLight as P3DAmbientLight, DirectionalLight as P3DDirectionalLight
render = application.base.render

ambient_node = P3DAmbientLight('ambient')
ambient_node.setColor((0.35, 0.35, 0.45, 1))
ambient_np = render.attachNewNode(ambient_node)
render.setLight(ambient_np)

sun_node = P3DDirectionalLight('sun')
sun_node.setColor((0.9, 0.85, 0.8, 1))
sun_np = render.attachNewNode(sun_node)
sun_np.lookAt(1, -1, 1)
render.setLight(sun_np)

# Import and create game manager (spawns the scene)
from game_manager import GameManager
gm = GameManager()


def update():
    dt = time.dt
    gm.update(dt)


def input(key):
    gm.input(key)


app.run()
