import { WebGLRenderer } from './renderer/WebGLRenderer';
import { Camera } from './renderer/Camera';
import { Input } from './game/Input';
import { Ship, createShip, triggerAbility, allShipDefs } from './game/Ship';
import { ParticleSystem } from './game/Particles';
import { updateAbility, drawAbility } from './game/Abilities';

// === Init ===
const canvas = document.getElementById('c') as HTMLCanvasElement;
const hudEl = document.getElementById('hud')!;
const renderer = new WebGLRenderer(canvas);
const camera = new Camera();
const input = new Input(canvas, camera);
const particles = new ParticleSystem();

window.addEventListener('resize', () => renderer.resize());

// === Create ships (showcase layout for now) ===
const shipDefs = allShipDefs();
const ships: Ship[] = [];
let placeX = 0;
for (let i = 0; i < shipDefs.length; i++) {
  const def = shipDefs[i];
  ships.push(createShip(def, renderer, placeX, 0, i));
  const nextLen = i < shipDefs.length - 1 ? shipDefs[i + 1].length : 0;
  placeX += def.length * 1.5 + nextLen * 1.5 + 50;
}

// === Number keys to focus ships ===
window.addEventListener('keydown', e => {
  const idx = parseInt(e.key) - 1;
  if (idx >= 0 && idx < ships.length) {
    const s = ships[idx];
    camera.focusOn(s.x, s.y, s.def.length, renderer.W);
    triggerAbility(s);
  }
});

// === Game loop ===
let lastTime = performance.now();
let fps = 0, frameCount = 0, fpsTime = 0;

function loop(now: number) {
  const dt = (now - lastTime) / 1000;
  lastTime = now;
  const time = now / 1000;

  // === UPDATE ===
  camera.update(dt);
  const viewMatrix = camera.buildViewMatrix(renderer.W, renderer.H);

  // Click to trigger ability
  const click = input.consumeClick();
  if (click) {
    let best: Ship | null = null, bestDist = Infinity;
    for (const s of ships) {
      const d = Math.hypot(click[0] - s.x, click[1] - s.y);
      const hitR = s.def.length * 0.8;
      if (d < hitR && d < bestDist) { best = s; bestDist = d; }
    }
    if (best) triggerAbility(best);
  }

  // Update abilities & particles
  for (const s of ships) updateAbility(s, time, particles);
  particles.update(dt);

  // === RENDER ===
  renderer.beginFrame(camera, time);

  // Draw ships
  for (const ship of ships) {
    renderer.drawShip(ship.buffers, ship.x, ship.y, ship.angle, viewMatrix, camera.zoom);
  }

  // Draw ability effects
  for (const s of ships) drawAbility(s, renderer, viewMatrix, time, camera.zoom);

  // Draw particles
  particles.draw(renderer, viewMatrix);

  renderer.endFrame();

  // === HUD ===
  frameCount++;
  fpsTime += dt;
  if (fpsTime >= 0.5) { fps = Math.round(frameCount / fpsTime); frameCount = 0; fpsTime = 0; }

  const shipCount = ships.length;
  const zoomMeters = renderer.W / camera.zoom;
  let scaleLabel: string;
  if (zoomMeters < 100) scaleLabel = `~${Math.round(zoomMeters)}m across`;
  else if (zoomMeters < 2000) scaleLabel = `~${(zoomMeters / 1000).toFixed(1)}km across`;
  else scaleLabel = `~${Math.round(zoomMeters / 1000)}km across`;

  hudEl.innerHTML = `FPS: ${fps}<br>SHIPS: ${shipCount}<br>ZOOM: ${camera.zoom.toFixed(3)} px/m<br>${scaleLabel}`;

  requestAnimationFrame(loop);
}

requestAnimationFrame(loop);
