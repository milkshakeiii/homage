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
const fleet: Ship[] = [];     // player-controlled ships
const enemyShips: Ship[] = [];

// Main ship (the "hero")
const mainShip = spawn(2, 0, 0);
fleet.push(mainShip);

// 4 corvettes in diamond
fleet.push(spawn(1, 200, 0));
fleet.push(spawn(1, -200, 0));
fleet.push(spawn(1, 0, 200));
fleet.push(spawn(1, 0, -200));

// 2 skiffs
fleet.push(spawn(0, 500, 100));
fleet.push(spawn(0, 500, -100));

// Enemy destroyer
const enemyDestroyer = spawn(3, 1500, 0, Math.PI);
enemyShips.push(enemyDestroyer);

const allShips = [...fleet, ...enemyShips];

// === Selection state ===
let selected: Set<Ship> = new Set();
selected.add(mainShip); // start with main ship selected
let commandMode: 'none' | 'collisionCourse' | 'orbit' | 'keepAtRange' = 'none';
let showDebug = false;
let enemyPatrol = false;
let enemyPatrolAngle = 0;

// === Camera ===
const CAM_PAN_SPEED = 800; // world units per second at zoom=1
camera.targetX = mainShip.x;
camera.targetY = mainShip.y;
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

/** Ships that receive commands: selected ships, or main ship as fallback */
function commandTargets(): Ship[] {
  if (selected.size > 0) return [...selected];
  return [mainShip];
}

function issueCommand(cmd: Command) {
  for (const s of commandTargets()) {
    s.command = { ...cmd } as Command;
  }
}

// === Game loop ===
let lastTime = performance.now();
let fps = 0, frameCount = 0, fpsTime = 0;

function loop(now: number) {
  const dt = Math.min((now - lastTime) / 1000, 0.05);
  lastTime = now;
  const time = now / 1000;

  // === CAMERA PANNING ===
  const panSpeed = CAM_PAN_SPEED / camera.zoom * dt;

  // WASD
  if (input.isDown('w')) camera.targetY -= panSpeed;
  if (input.isDown('s')) camera.targetY += panSpeed;
  if (input.isDown('a')) camera.targetX -= panSpeed;
  if (input.isDown('d')) camera.targetX += panSpeed;

  // Edge-pan (fullscreen only)
  const edge = input.edgePan();
  camera.targetX += edge.x * panSpeed;
  camera.targetY += edge.y * panSpeed;

  // Space: snap to main ship
  if (consumeKey(' ')) {
    camera.targetX = mainShip.x;
    camera.targetY = mainShip.y;
  }

  // F: toggle fullscreen
  if (consumeKey('f')) {
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      document.documentElement.requestFullscreen();
    }
  }

  // === COMMANDS ===
  if (consumeKey('x')) {
    for (const s of commandTargets()) s.command = undefined;
    commandMode = 'none';
  }
  if (consumeKey('c')) commandMode = 'collisionCourse';
  if (consumeKey('o')) commandMode = 'orbit';
  if (consumeKey('k')) commandMode = 'keepAtRange';

  if (consumeKey('e')) {
    for (const s of commandTargets()) {
      s.evasive = s.evasive ? undefined : { jinkTimer: 0.3, jinkAngle: 0 };
    }
  }

  // Tab: cycle selection through fleet
  if (consumeKey('tab')) {
    const current = selected.size === 1 ? [...selected][0] : null;
    const idx = current ? fleet.indexOf(current) : -1;
    const next = fleet[(idx + 1) % fleet.length];
    selected.clear();
    selected.add(next);
  }

  // Toggle enemy patrol
  if (consumeKey('t')) enemyPatrol = !enemyPatrol;

  // Toggle debug
  if (consumeKey('g')) showDebug = !showDebug;

  // Number keys to spawn ships at cursor position
  for (let i = 0; i < 6; i++) {
    if (consumeKey(String(i + 1))) {
      const [wx, wy] = camera.screenToWorld(input.mouseX, input.mouseY, canvas.width, canvas.height);
      const s = spawn(i, wx, wy);
      fleet.push(s);
      allShips.push(s);
    }
  }

  // Left click — select or target
  const click = input.consumeClick();
  if (click) {
    if (commandMode !== 'none') {
      const target = findShipAtPoint(click[0], click[1], allShips);
      if (target && commandMode === 'collisionCourse') {
        issueCommand({ type: 'collisionCourse', targetShip: target });
      } else if (target && commandMode === 'orbit') {
        const radius = target.def.length * 2;
        issueCommand({ type: 'orbit', targetShip: target, radius, clockwise: true });
      } else if (target && commandMode === 'keepAtRange') {
        issueCommand({ type: 'keepAtRange', targetShip: target, range: 500 });
      }
      commandMode = 'none';
    } else {
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

  // Box-select
  const box = input.consumeBoxSelect();
  if (box) {
    const wb = input.boxToWorld(box);
    if (!input.shiftHeld) selected.clear();
    for (const s of fleet) {
      if (s.x >= wb.minX && s.x <= wb.maxX && s.y >= wb.minY && s.y <= wb.maxY) {
        selected.add(s);
      }
    }
  }

  // Right click — move command
  const rightClick = input.consumeRightClick();
  if (rightClick) {
    issueCommand({ type: 'move', target: { x: rightClick[0], y: rightClick[1] } });
    commandMode = 'none';
  }

  // === UPDATE ===
  camera.update(dt);

  if (enemyPatrol) {
    enemyPatrolAngle += 0.3 * dt;
    const cx = 1500, cy = 0, r = 600;
    const tx = cx + Math.cos(enemyPatrolAngle) * r;
    const ty = cy + Math.sin(enemyPatrolAngle) * r;
    enemyDestroyer.command = { type: 'move', target: { x: tx, y: ty } };
  }

  for (const s of allShips) {
    updateCommand(s, dt);
  }

  for (const s of allShips) updateAbility(s, time, particles);
  particles.update(dt);

  const viewMatrix = camera.buildViewMatrix(renderer.W, renderer.H);

  // === RENDER ===
  renderer.beginFrame(camera, time);

  for (const ship of allShips) {
    renderer.drawShip(ship.buffers, ship.x, ship.y, ship.angle, viewMatrix, camera.zoom);
  }

  // Main ship persistent highlight (dimmer than selection ring)
  if (!selected.has(mainShip)) {
    const r = mainShip.def.length * 0.7;
    renderer.dynCircle(mainShip.x, mainShip.y, r, 48);
    renderer.dynFlush(viewMatrix, [0, 1, 1, 0.12]);
  }

  // Selection rings
  for (const s of selected) {
    const r = s.def.length * 0.7;
    renderer.dynCircle(s.x, s.y, r, 48);
    renderer.dynFlush(viewMatrix, [0, 1, 1, 0.3]);
  }

  // Command mode indicator
  if (commandMode !== 'none') {
    const [wx, wy] = camera.screenToWorld(input.mouseX, input.mouseY, canvas.width, canvas.height);
    const hover = findShipAtPoint(wx, wy, allShips);
    if (hover) {
      const r = hover.def.length * 0.8;
      renderer.dynCircle(hover.x, hover.y, r, 48);
      renderer.dynFlush(viewMatrix, [1, 0.5, 0, 0.4]);
    }
  }

  // Move command target dots (shown for command targets, including main ship fallback)
  for (const s of commandTargets()) {
    if (s.command?.type === 'move') {
      const t = s.command.target;
      renderer.dynCircle(t.x, t.y, 5 / camera.zoom, 12);
      renderer.dynFlush(viewMatrix, [0, 1, 0, 0.6]);
    }
  }

  // Box-select preview rectangle
  if (input.boxDragging) {
    const [wx0, wy0] = camera.screenToWorld(input.boxDragX0, input.boxDragY0, canvas.width, canvas.height);
    const [wx1, wy1] = camera.screenToWorld(input.mouseX, input.mouseY, canvas.width, canvas.height);
    const lw = 1.5 / camera.zoom;
    renderer.dynLine(wx0, wy0, wx1, wy0, lw);
    renderer.dynLine(wx1, wy0, wx1, wy1, lw);
    renderer.dynLine(wx1, wy1, wx0, wy1, lw);
    renderer.dynLine(wx0, wy1, wx0, wy0, lw);
    renderer.dynLineFlush(viewMatrix, [0, 1, 0, 0.5]);
  }

  if (showDebug) drawDebug(viewMatrix);

  for (const s of allShips) drawAbility(s, renderer, viewMatrix, time, camera.zoom);
  particles.draw(renderer, viewMatrix);

  renderer.endFrame();

  // === HUD ===
  frameCount++;
  fpsTime += dt;
  if (fpsTime >= 0.5) { fps = Math.round(frameCount / fpsTime); frameCount = 0; fpsTime = 0; }

  const selCount = selected.size;
  const anyEvasive = [...commandTargets()].some(s => s.evasive);
  const modeLabel = commandMode !== 'none' ? ` | MODE: ${commandMode.toUpperCase()}` : '';
  const evasiveLabel = anyEvasive ? ' | EVASIVE' : '';
  const patrolLabel = enemyPatrol ? ' | PATROL ON' : '';
  const zoomMeters = renderer.W / camera.zoom;
  let scaleLabel: string;
  if (zoomMeters < 100) scaleLabel = `~${Math.round(zoomMeters)}m`;
  else if (zoomMeters < 2000) scaleLabel = `~${(zoomMeters / 1000).toFixed(1)}km`;
  else scaleLabel = `~${Math.round(zoomMeters / 1000)}km`;

  hudEl.innerHTML = `FPS: ${fps} | SHIPS: ${allShips.length} | SEL: ${selCount}${modeLabel}${evasiveLabel}${patrolLabel}<br>ZOOM: ${camera.zoom.toFixed(3)} (${scaleLabel})`;

  keysJustPressed.clear();
  requestAnimationFrame(loop);
}

function drawDebug(viewMatrix: Float32Array) {
  const lineHW = 2 / camera.zoom;

  for (const s of allShips) {
    if (!s.physicsConfig) continue;

    const velScale = 2;
    renderer.dynLine(s.x, s.y, s.x + s.vx * velScale, s.y + s.vy * velScale, lineHW);
    renderer.dynLineFlush(viewMatrix, [0, 1, 0, 0.7]);

    if (s.command) {
      const headLen = s.def.length * 1.5;
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

    if (s.command?.type === 'collisionCourse') {
      const t = s.command.targetShip;
      const predX = t.x + t.vx * 0.5;
      const predY = t.y + t.vy * 0.5;
      renderer.dynCircle(predX, predY, 8 / camera.zoom, 8);
      renderer.dynFlush(viewMatrix, [1, 0, 0, 0.8]);
    }

    if (s.command?.type === 'keepAtRange') {
      const t = s.command.targetShip;
      renderer.dynCircle(t.x, t.y, s.command.range, 48);
      renderer.dynFlush(viewMatrix, [1, 0.5, 0, 0.15]);
    }
  }
}

requestAnimationFrame(loop);
