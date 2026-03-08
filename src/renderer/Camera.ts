export class Camera {
  x = 0;
  y = 0;
  zoom = 0.15;
  targetX = 0;
  targetY = 0;
  targetZoom = 0.15;
  smoothing = 0.08;

  update(_dt: number) {
    this.x += (this.targetX - this.x) * this.smoothing;
    this.y += (this.targetY - this.y) * this.smoothing;
    this.zoom += (this.targetZoom - this.zoom) * this.smoothing;
  }

  buildViewMatrix(w: number, h: number): Float32Array {
    const sx = (2 * this.zoom) / w;
    const sy = (-2 * this.zoom) / h;
    const tx = -this.x * sx;
    const ty = -this.y * sy;
    return new Float32Array([sx, 0, 0, 0, sy, 0, tx, ty, 1]);
  }

  screenToWorld(sx: number, sy: number, w: number, h: number): [number, number] {
    return [
      (sx - w / 2) / this.zoom + this.x,
      (sy - h / 2) / this.zoom + this.y,
    ];
  }

  zoomBy(factor: number) {
    this.targetZoom = Math.max(0.003, Math.min(40, this.targetZoom * factor));
  }

  focusOn(x: number, y: number, fitLength: number, screenW: number) {
    this.targetX = x;
    this.targetY = y;
    this.targetZoom = Math.min(40, screenW / (fitLength * 4));
  }
}
