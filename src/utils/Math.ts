export interface Vec2 {
  x: number;
  y: number;
}

export function v2(x: number, y: number): Vec2 {
  return { x, y };
}

export function v2Add(a: Vec2, b: Vec2): Vec2 {
  return { x: a.x + b.x, y: a.y + b.y };
}

export function v2Sub(a: Vec2, b: Vec2): Vec2 {
  return { x: a.x - b.x, y: a.y - b.y };
}

export function v2Scale(a: Vec2, s: number): Vec2 {
  return { x: a.x * s, y: a.y * s };
}

export function v2Len(a: Vec2): number {
  return Math.sqrt(a.x * a.x + a.y * a.y);
}

export function v2LenSq(a: Vec2): number {
  return a.x * a.x + a.y * a.y;
}

export function v2Norm(a: Vec2): Vec2 {
  const l = v2Len(a);
  return l > 0 ? { x: a.x / l, y: a.y / l } : { x: 0, y: 0 };
}

export function v2Dot(a: Vec2, b: Vec2): number {
  return a.x * b.x + a.y * b.y;
}

export function v2Cross(a: Vec2, b: Vec2): number {
  return a.x * b.y - a.y * b.x;
}

export function v2Rotate(a: Vec2, angle: number): Vec2 {
  const c = Math.cos(angle), s = Math.sin(angle);
  return { x: a.x * c - a.y * s, y: a.x * s + a.y * c };
}

export function v2Dist(a: Vec2, b: Vec2): number {
  return v2Len(v2Sub(a, b));
}

export function v2DistSq(a: Vec2, b: Vec2): number {
  return v2LenSq(v2Sub(a, b));
}

export function v2Lerp(a: Vec2, b: Vec2, t: number): Vec2 {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function clamp(v: number, min: number, max: number): number {
  return v < min ? min : v > max ? max : v;
}

export function angleDiff(a: number, b: number): number {
  let d = ((b - a) % (Math.PI * 2) + Math.PI * 3) % (Math.PI * 2) - Math.PI;
  return d;
}
