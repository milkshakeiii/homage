import { Ship } from './Ship';
import { ParticleSystem } from './Particles';
import { WebGLRenderer } from '../renderer/WebGLRenderer';

export function updateAbility(ship: Ship, _time: number, particles: ParticleSystem) {
  const ab = ship.ability;
  if (!ab) return;
  ab.timer++;
  const t = ab.timer / ab.max;
  const { x, y, def } = ship;
  const len = def.length;

  switch (ab.type) {
    case 'afterburner': {
      const intensity = Math.sin(t * Math.PI);
      for (let i = 0; i < 3; i++) {
        const spread = (Math.random() - 0.5) * len * 0.4;
        const speed = len * 0.3 + Math.random() * len * 0.2;
        particles.spawn(
          x - len * 0.5 + spread * 0.3, y + spread,
          -speed * (0.5 + intensity), (Math.random() - 0.5) * len * 0.1,
          Math.random() > 0.5 ? '#f80' : '#ff0', 20 + Math.random() * 20, len * 0.05 * intensity + 1,
        );
      }
      break;
    }
    case 'missiles': {
      if (ab.timer % 8 === 0 && ab.timer < 60) {
        const my = (Math.random() - 0.5) * len * 0.6;
        ab.missiles.push({
          x: x + len * 0.2, y: y + my,
          vx: len * 0.08 + Math.random() * len * 0.05, vy: my * 0.05,
          trail: [], life: 80,
        });
      }
      ab.missiles = ab.missiles.filter(m => {
        m.x += m.vx; m.y += m.vy; m.vy += (Math.random() - 0.5) * 0.5;
        m.trail.push({ x: m.x, y: m.y });
        if (m.trail.length > 15) m.trail.shift();
        m.life--;
        if (m.life <= 0) {
          for (let i = 0; i < 8; i++) {
            const a = Math.random() * Math.PI * 2;
            particles.spawn(m.x, m.y, Math.cos(a) * len * 0.05, Math.sin(a) * len * 0.05, '#f80', 20, len * 0.02);
          }
        }
        return m.life > 0;
      });
      break;
    }
    case 'shield': break;
    case 'broadside': {
      const fireRate = 10, startDelay = 10, elapsed = ab.timer - startDelay;
      if (elapsed >= 0 && elapsed % fireRate === 0 && elapsed < fireRate * 6) {
        const gunIdx = Math.floor(elapsed / fireRate), gunX = 120 - gunIdx * 50;
        ab.flashes.push({ timer: 20, x: x + gunX, yTop: y + 65 + gunIdx * 2, yBot: y - 65 - gunIdx * 2 });
      }
      ab.flashes = ab.flashes.filter(f => {
        f.timer--;
        if (f.timer % 3 === 0) {
          for (const fy of [f.yTop, f.yBot]) {
            const dir = fy > y ? 1 : -1;
            particles.spawn(f.x, fy, (Math.random() - 0.5) * len * 0.02, dir * len * 0.08, '#ff0', 15, len * 0.01);
          }
        }
        return f.timer > 0;
      });
      break;
    }
    case 'lance': {
      const chargeT = 0.4;
      if (t < chargeT) {
        const ct = t / chargeT, radius = len * (1 - ct) * 0.8;
        for (let i = 0; i < 2; i++) {
          const a = Math.random() * Math.PI * 2, r = radius * (0.5 + Math.random() * 0.5);
          particles.spawn(
            x + len * 0.3 + Math.cos(a) * r, y + Math.sin(a) * r,
            -Math.cos(a) * len * 0.05 * ct, -Math.sin(a) * len * 0.05 * ct,
            '#f44', 10, len * 0.005 * (1 + ct * 2),
          );
        }
      } else {
        // Impact sparks during beam
        if (Math.random() > 0.5) {
          const beamLen = len * 5;
          particles.spawn(
            x + len * 0.3 + beamLen * (0.8 + Math.random() * 0.2),
            y + (Math.random() - 0.5) * len * 0.2,
            (Math.random() - 0.5) * len * 0.05, (Math.random() - 0.5) * len * 0.1,
            '#f88', 10, len * 0.01,
          );
        }
      }
      break;
    }
    case 'fighters': {
      if (ab.timer % 15 === 0 && ab.fighters.length < 18) {
        const bayIdx = Math.floor(Math.random() * 6);
        const a1 = (bayIdx / 6) * Math.PI * 2 - Math.PI / 6;
        const a2 = ((bayIdx + 1) / 6) * Math.PI * 2 - Math.PI / 6;
        const mid = (a1 + a2) / 2;
        const bx = x + Math.cos(mid) * len * 0.36, by = y + Math.sin(mid) * len * 0.36;
        ab.fighters.push({
          x: bx, y: by,
          vx: Math.cos(mid) * len * 0.02 + (Math.random() - 0.5) * len * 0.005,
          vy: Math.sin(mid) * len * 0.02 + (Math.random() - 0.5) * len * 0.005,
          angle: mid, life: 120 + Math.random() * 60,
        });
      }
      ab.fighters = ab.fighters.filter(f => {
        f.x += f.vx; f.y += f.vy; f.vx *= 1.01;
        f.angle = Math.atan2(f.vy, f.vx); f.life--;
        if (f.life % 2 === 0) particles.spawn(f.x, f.y, -f.vx * 0.3, -f.vy * 0.3, '#48f', 15, len * 0.003);
        return f.life > 0;
      });
      break;
    }
  }
  if (ab.timer >= ab.max) ship.ability = null;
}

export function drawAbility(ship: Ship, renderer: WebGLRenderer, viewMatrix: Float32Array, time: number, zoom: number) {
  const ab = ship.ability;
  if (!ab) return;
  const t = ab.timer / ab.max;
  const { x, y, def } = ship;
  const len = def.length;

  switch (ab.type) {
    case 'missiles': {
      // Ensure trails/heads are at least 2px on screen
      const minW = 2 / zoom;
      const trailW = Math.max(len * 0.005, minW);
      const headR = Math.max(len * 0.01, minW);
      for (const m of ab.missiles) {
        if (m.trail.length > 1) {
          for (let i = 0; i < m.trail.length - 1; i++) {
            renderer.dynLine(m.trail[i].x, m.trail[i].y, m.trail[i + 1].x, m.trail[i + 1].y, trailW);
          }
          renderer.dynLineFlush(viewMatrix, [1, 0.27, 0.27, 0.8]);
        }
        renderer.dynReset();
        renderer.dynCircle(m.x, m.y, headR, 8);
        renderer.dynFlush(viewMatrix, [1, 1, 1, 1]);
      }
      break;
    }
    case 'shield': {
      const radius = len * 0.7;
      const pulse = 0.9 + 0.1 * Math.sin(time * 10);
      const alpha = Math.sin(t * Math.PI) * 0.6;
      const r = radius * pulse;
      for (let i = 0; i < 6; i++) {
        const a1 = (i / 6) * Math.PI * 2 + time * 0.5;
        const a2 = ((i + 1) / 6) * Math.PI * 2 + time * 0.5;
        const segAlpha = alpha * (0.5 + 0.5 * Math.sin(time * 6 + i));
        renderer.dynLine(
          x + Math.cos(a1) * r, y + Math.sin(a1) * r,
          x + Math.cos(a2) * r, y + Math.sin(a2) * r, len * 0.01,
        );
        renderer.dynLineFlush(viewMatrix, [0, 1, 1, segAlpha]);
      }
      break;
    }
    case 'broadside': {
      for (const f of ab.flashes) {
        const alpha = f.timer / 20;
        const flashLen = len * 0.2;
        for (const fy of [f.yTop, f.yBot]) {
          const dir = fy > y ? 1 : -1;
          renderer.dynLine(f.x, fy, f.x, fy + dir * flashLen, len * 0.015);
          renderer.dynLineFlush(viewMatrix, [1, 1, 0.4, alpha]);
          renderer.dynReset();
          renderer.dynCircle(f.x, fy, len * 0.02 * alpha, 8);
          renderer.dynFlush(viewMatrix, [1, 1, 0.8, alpha]);
        }
      }
      break;
    }
    case 'lance': {
      const chargeT = 0.4;
      if (t < chargeT) {
        const ct = t / chargeT;
        renderer.dynReset();
        renderer.dynCircle(x + len * 0.3, y, len * 0.03 * ct * 2, 12);
        renderer.dynFlush(viewMatrix, [1, 0.27, 0.27, ct]);
      } else {
        const ft = (t - chargeT) / (1 - chargeT);
        const beamAlpha = ft < 0.1 ? ft / 0.1 : (1 - ft);
        const beamLen = len * 5;
        const beamW = len * 0.04 * (1 + Math.sin(time * 30) * 0.2);
        renderer.dynLine(x + len * 0.3, y, x + len * 0.3 + beamLen, y, beamW);
        renderer.dynLineFlush(viewMatrix, [1, 0.4, 0.4, beamAlpha]);
        renderer.dynLine(x + len * 0.3, y, x + len * 0.3 + beamLen, y, beamW * 0.3);
        renderer.dynLineFlush(viewMatrix, [1, 0.85, 0.85, beamAlpha * 0.8]);
      }
      break;
    }
    case 'fighters': {
      const miniVerts: number[][] = [[4, 0], [-2, -2], [-3, 0], [-2, 2]];
      const scale = len * 0.012;
      for (const f of ab.fighters) {
        const alpha = Math.min(1, f.life / 30);
        const cos = Math.cos(f.angle), sin = Math.sin(f.angle);
        renderer.dynReset();
        const tx: number[] = [];
        for (const v of miniVerts) {
          tx.push(f.x + (v[0] * cos - v[1] * sin) * scale, f.y + (v[0] * sin + v[1] * cos) * scale);
        }
        const data = new Float32Array(12);
        data[0] = tx[0]; data[1] = tx[1]; data[2] = tx[2]; data[3] = tx[3]; data[4] = tx[4]; data[5] = tx[5];
        data[6] = tx[0]; data[7] = tx[1]; data[8] = tx[4]; data[9] = tx[5]; data[10] = tx[6]; data[11] = tx[7];
        renderer.dynTriangles(data, 6);
        renderer.dynFlush(viewMatrix, [0.27, 0.53, 1, alpha]);
      }
      break;
    }
  }
}
