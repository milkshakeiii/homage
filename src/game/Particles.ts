import { parseColor, Color4 } from '../utils/Color';
import { WebGLRenderer } from '../renderer/WebGLRenderer';

export interface Particle {
  x: number; y: number;
  vx: number; vy: number;
  color: Color4;
  life: number;
  maxLife: number;
  size: number;
}

export class ParticleSystem {
  particles: Particle[] = [];

  spawn(x: number, y: number, vx: number, vy: number, color: string, life: number, size: number) {
    this.particles.push({ x, y, vx, vy, color: parseColor(color), life, maxLife: life, size });
  }

  update(_dt: number) {
    this.particles = this.particles.filter(p => {
      p.x += p.vx;
      p.y += p.vy;
      p.vx *= 0.96;
      p.vy *= 0.96;
      p.life--;
      return p.life > 0;
    });
  }

  draw(renderer: WebGLRenderer, viewMatrix: Float32Array) {
    if (this.particles.length === 0) return;
    const colorMap = new Map<string, Particle[]>();
    for (const p of this.particles) {
      const key = p.color.join(',');
      if (!colorMap.has(key)) colorMap.set(key, []);
      colorMap.get(key)!.push(p);
    }
    for (const [_key, ps] of colorMap) {
      renderer.dynReset();
      for (const p of ps) {
        const alpha = p.life / p.maxLife;
        const sz = p.size * alpha;
        if (sz > 0.1) renderer.dynCircle(p.x, p.y, sz, 6);
      }
      const c = ps[0].color;
      const avgAlpha = ps.reduce((a, p) => a + p.life / p.maxLife, 0) / ps.length;
      renderer.dynFlush(viewMatrix, [c[0], c[1], c[2], avgAlpha]);
    }
  }
}
