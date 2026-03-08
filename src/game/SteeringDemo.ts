import { WebGLRenderer } from '../renderer/WebGLRenderer';
import { Camera } from '../renderer/Camera';
import { Input } from './Input';
import {
  Ship, createShip,
  makeSkiff, makeCorvette, makeFrigate, makeDestroyer, makeCruiser, makeMothership,
  ShipDef,
} from './Ship';
import { ParticleSystem } from './Particles';
import { updateAbility, drawAbility } from './Abilities';
import { Command, updateCommand } from './Commands';
import { v2Len, v2Sub } from '../utils/Math';

// === Init ===
const canvas = document.getElementById('c') as HTMLCanvasElement;
const hudEl = document.getElementById('hud')!;
const renderer = new WebGLRenderer(canvas);
const camera = new Camera();
const input = new Input(canvas, camera);
const particles = new ParticleSystem();

window.addEventListener('resize', () => renderer.resize());

// === Ship factory helpers ===
const SHIP_MAKERS: (() => ShipDef)[] = [makeSkiff, makeCorvette, makeFrigate, makeDestroyer, makeCruiser, makeMothership];

function spawn(defIdx: number, x: number, y: number, angle = 0): Ship {
  const def = SHIP_MAKERS[defIdx]();
  const ship = createShip(def, renderer, x, y, defIdx);
  ship.angle = angle;
  return ship;
}

// === Scene setup ===
const ships: Ship[] = [];
const enemyShips: Ship[] = [];

// Player fleet
const frigate = spawn(2, 0, 0);
ships.push(frigate);

// 4 corvettes in diamond
ships.push(spawn(1, 200, 0));
ships.push(spawn(1, -200, 0));
ships.push(spawn(1, 0, 200));
ships.push(spawn(1, 0, -200));

// 2 skiffs
ships.push(spawn(0, 500, 100));
ships.push(spawn(0, 500, -100));

// Enemy destroyer
const enemyDestroyer = spawn(3, 1500, 0, Math.PI);
enemyShips.push(enemyDestroyer);

const allShips = [...ships, ...enemyShips];

// === Selection state ===
let selected: Set<Ship> = new Set();
let commandMode: 'none' | 'collisionCourse' | 'orbit' | 'keepAtRange' = 'none';
let showDebug = false;
let enemyPatrol = false;
let enemyPatrolAngle = 0;

// === Camera initial ===
camera.targetZoom = 0.5;

// === Input handling ===
const keysJustPressed = new Set<string>();
window.addEventListener('keydown', e => {
  keysJustPressed.add(e.key.toLowerCase());
});

function consumeKey(key: string): boolean {
  if (keysJustPressed.has(key)) {
    keysJustPressed.delete(key);
    return true;
  }
  return false;
}

function findShipAtPoint(wx: number, wy: number, pool: Ship[]): Ship | null {
  let best: Ship | null = null;
  let bestDist = Infinity;
  for (const s of pool) {
    const d = Math.hypot(wx - s.x, wy - s.y);
    const hitR = s.def.length * 0.8;
    if (d < hitR && d < bestDist) {
      best = s;
      bestDist = d;
    }
  }
  return best;
}

function issueCommandToSelected(cmd: Command) {
  for (const s of selected) {
    s.command = { ...cmd } as Command;
  }
}

// === Game loop ===
let lastTime = performance.now();
let fps = 0, frameCount = 0, fpsTime = 0;

function loop(now: number) {
  const dt = Math.min((now - lastTime) / 1000, 0.05); // cap dt
  lastTime = now;
  const time = now / 1000;

  // === HANDLE INPUT ===

  // Keyboard commands
  if (consumeKey('s')) {
    for (const s of selected) s.command = undefined;
    commandMode = 'none';
  }
  if (consumeKey('c')) commandMode = 'collisionCourse';
  if (consumeKey('o')) commandMode = 'orbit';
  if (consumeKey('k')) commandMode = 'keepAtRange';
  if (consumeKey('d')) showDebug = !showDebug;
  if (consumeKey('t')) enemyPatrol = !enemyPatrol;

  if (consumeKey('e')) {
    for (const s of selected) {
      s.command = {
        type: 'evasive',
        baseDir: s.angle + Math.PI,
        jinkTimer: 0.3,
        jinkAngle: 0,
      };
    }
    commandMode = 'none';
  }

  // Number keys to spawn ships at cursor position
  for (let i = 0; i < 6; i++) {
    if (consumeKey(String(i + 1))) {
      const [wx, wy] = camera.screenToWorld(input.mouseX, input.mouseY, canvas.width, canvas.height);
      const s = spawn(i, wx, wy);
      ships.push(s);
      allShips.push(s);
    }
  }

  // Left click — select or target
  const click = input.consumeClick();
  if (click) {
    if (commandMode !== 'none') {
      // Targeting click for command mode
      const target = findShipAtPoint(click[0], click[1], allShips);
      if (target && commandMode === 'collisionCourse') {
        issueCommandToSelected({ type: 'collisionCourse', targetShip: target });
      } else if (target && commandMode === 'orbit') {
        const radius = target.def.length * 2;
        issueCommandToSelected({ type: 'orbit', targetShip: target, radius, clockwise: true });
      } else if (target && commandMode === 'keepAtRange') {
        issueCommandToSelected({ type: 'keepAtRange', targetShip: target, range: 500 });
      }
      commandMode = 'none';
    } else {
      // Selection click
      const hit = findShipAtPoint(click[0], click[1], allShips);
      if (hit) {
        if (input.shiftHeld) {
          if (selected.has(hit)) selected.delete(hit);
          else selected.add(hit);
        } else {
          selected.clear();
          selected.add(hit);
        }
      } else if (!input.shiftHeld) {
        selected.clear();
      }
    }
  }

  // Right click — move command
  const rightClick = input.consumeRightClick();
  if (rightClick && selected.size > 0) {
    issueCommandToSelected({ type: 'move', target: { x: rightClick[0], y: rightClick[1] } });
    commandMode = 'none';
  }

  // === UPDATE ===
  camera.update(dt);

  // Enemy patrol behavior
  if (enemyPatrol) {
    enemyPatrolAngle += 0.3 * dt;
    const cx = 1500, cy = 0, r = 600;
    const tx = cx + Math.cos(enemyPatrolAngle) * r;
    const ty = cy + Math.sin(enemyPatrolAngle) * r;
    enemyDestroyer.command = { type: 'move', target: { x: tx, y: ty } };
  }

  // Update all ship commands/physics
  for (const s of allShips) {
    updateCommand(s, dt);
  }

  // Update abilities & particles
  for (const s of allShips) updateAbility(s, time, particles);
  particles.update(dt);

  const viewMatrix = camera.buildViewMatrix(renderer.W, renderer.H);

  // === RENDER ===
  renderer.beginFrame(camera, time);

  // Draw ships
  for (const ship of allShips) {
    renderer.drawShip(ship.buffers, ship.x, ship.y, ship.angle, viewMatrix, camera.zoom);
  }

  // Selection rings
  for (const s of selected) {
    const r = s.def.length * 0.7;
    renderer.dynCircle(s.x, s.y, r, 48);
    renderer.dynFlush(viewMatrix, [0, 1, 1, 0.3]);
  }

  // Command mode indicator — draw ring around hovered target
  if (commandMode !== 'none') {
    const [wx, wy] = camera.screenToWorld(input.mouseX, input.mouseY, canvas.width, canvas.height);
    const hover = findShipAtPoint(wx, wy, allShips);
    if (hover) {
      const r = hover.def.length * 0.8;
      renderer.dynCircle(hover.x, hover.y, r, 48);
      renderer.dynFlush(viewMatrix, [1, 0.5, 0, 0.4]);
    }
  }

  // Move command target dots
  for (const s of selected) {
    if (s.command?.type === 'move') {
      const t = s.command.target;
      renderer.dynCircle(t.x, t.y, 5 / camera.zoom, 12);
      renderer.dynFlush(viewMatrix, [0, 1, 0, 0.6]);
    }
  }

  // Debug overlay
  if (showDebug) {
    drawDebug(viewMatrix);
  }

  // Draw ability effects
  for (const s of allShips) drawAbility(s, renderer, viewMatrix, time, camera.zoom);

  // Draw particles
  particles.draw(renderer, viewMatrix);

  renderer.endFrame();

  // === HUD ===
  frameCount++;
  fpsTime += dt;
  if (fpsTime >= 0.5) { fps = Math.round(frameCount / fpsTime); frameCount = 0; fpsTime = 0; }

  const selCount = selected.size;
  const modeLabel = commandMode !== 'none' ? ` | MODE: ${commandMode.toUpperCase()}` : '';
  const patrolLabel = enemyPatrol ? ' | PATROL ON' : '';
  const zoomMeters = renderer.W / camera.zoom;
  let scaleLabel: string;
  if (zoomMeters < 100) scaleLabel = `~${Math.round(zoomMeters)}m`;
  else if (zoomMeters < 2000) scaleLabel = `~${(zoomMeters / 1000).toFixed(1)}km`;
  else scaleLabel = `~${Math.round(zoomMeters / 1000)}km`;

  hudEl.innerHTML = `FPS: ${fps} | SHIPS: ${allShips.length} | SEL: ${selCount}${modeLabel}${patrolLabel}<br>ZOOM: ${camera.zoom.toFixed(3)} (${scaleLabel})`;

  keysJustPressed.clear();
  requestAnimationFrame(loop);
}

function drawDebug(viewMatrix: Float32Array) {
  const lineHW = 2 / camera.zoom;

  for (const s of allShips) {
    if (!s.physicsConfig) continue;

    // Green: current velocity
    const velScale = 2;
    renderer.dynLine(s.x, s.y, s.x + s.vx * velScale, s.y + s.vy * velScale, lineHW);
    renderer.dynLineFlush(viewMatrix, [0, 1, 0, 0.7]);

    // Yellow: desired heading direction (from command)
    if (s.command) {
      const headLen = s.def.length * 1.5;
      // Compute desired angle from current command
      let desAngle = s.angle;
      if (s.command.type === 'move') {
        const dx = s.command.target.x - s.x;
        const dy = s.command.target.y - s.y;
        if (dx !== 0 || dy !== 0) desAngle = Math.atan2(dy, dx);
      } else if (s.command.type === 'collisionCourse' || s.command.type === 'orbit' || s.command.type === 'keepAtRange') {
        const t = s.command.targetShip;
        desAngle = Math.atan2(t.y - s.y, t.x - s.x);
      }
      renderer.dynLine(s.x, s.y, s.x + Math.cos(desAngle) * headLen, s.y + Math.sin(desAngle) * headLen, lineHW);
      renderer.dynLineFlush(viewMatrix, [1, 1, 0, 0.5]);
    }

    // Cyan: orbit/arrival radius
    if (s.command?.type === 'orbit') {
      const t = s.command.targetShip;
      renderer.dynCircle(t.x, t.y, s.command.radius, 48);
      renderer.dynFlush(viewMatrix, [0, 1, 1, 0.15]);
    }
    if (s.command?.type === 'move') {
      const dist = v2Len(v2Sub(s.command.target, { x: s.x, y: s.y }));
      if (dist < 200) {
        renderer.dynCircle(s.command.target.x, s.command.target.y, 150, 32);
        renderer.dynFlush(viewMatrix, [0, 1, 1, 0.1]);
      }
    }

    // Red dot: target ship position (intercept indicator)
    if (s.command?.type === 'collisionCourse') {
      const t = s.command.targetShip;
      const predX = t.x + t.vx * 0.5;
      const predY = t.y + t.vy * 0.5;
      renderer.dynCircle(predX, predY, 8 / camera.zoom, 8);
      renderer.dynFlush(viewMatrix, [1, 0, 0, 0.8]);
    }

    // Keep at range — draw range ring
    if (s.command?.type === 'keepAtRange') {
      const t = s.command.targetShip;
      renderer.dynCircle(t.x, t.y, s.command.range, 48);
      renderer.dynFlush(viewMatrix, [1, 0.5, 0, 0.15]);
    }
  }
}

requestAnimationFrame(loop);
