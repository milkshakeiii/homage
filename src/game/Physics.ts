export interface CircleCollider {
  x: number;
  y: number;
  radius: number;
}

// Simple spatial grid for broad-phase collision
export class SpatialGrid {
  private cellSize: number;
  private cells = new Map<string, number[]>();

  constructor(cellSize: number) {
    this.cellSize = cellSize;
  }

  clear() {
    this.cells.clear();
  }

  private key(cx: number, cy: number): string {
    return `${cx},${cy}`;
  }

  insert(id: number, x: number, y: number, radius: number) {
    const cs = this.cellSize;
    const minCx = Math.floor((x - radius) / cs);
    const maxCx = Math.floor((x + radius) / cs);
    const minCy = Math.floor((y - radius) / cs);
    const maxCy = Math.floor((y + radius) / cs);
    for (let cx = minCx; cx <= maxCx; cx++) {
      for (let cy = minCy; cy <= maxCy; cy++) {
        const k = this.key(cx, cy);
        if (!this.cells.has(k)) this.cells.set(k, []);
        this.cells.get(k)!.push(id);
      }
    }
  }

  query(x: number, y: number, radius: number): Set<number> {
    const cs = this.cellSize;
    const minCx = Math.floor((x - radius) / cs);
    const maxCx = Math.floor((x + radius) / cs);
    const minCy = Math.floor((y - radius) / cs);
    const maxCy = Math.floor((y + radius) / cs);
    const result = new Set<number>();
    for (let cx = minCx; cx <= maxCx; cx++) {
      for (let cy = minCy; cy <= maxCy; cy++) {
        const ids = this.cells.get(this.key(cx, cy));
        if (ids) for (const id of ids) result.add(id);
      }
    }
    return result;
  }
}

export function circlesOverlap(a: CircleCollider, b: CircleCollider): boolean {
  const dx = a.x - b.x, dy = a.y - b.y;
  const rSum = a.radius + b.radius;
  return dx * dx + dy * dy < rSum * rSum;
}
