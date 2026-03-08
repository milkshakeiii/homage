export type Color4 = [number, number, number, number];

export function parseColor(css: string): Color4 {
  if (css.startsWith('rgba')) {
    const m = css.match(/[\d.]+/g)!;
    return [+m[0] / 255, +m[1] / 255, +m[2] / 255, +m[3]];
  }
  if (css.startsWith('#')) {
    if (css.length === 4) {
      return [
        parseInt(css[1] + css[1], 16) / 255,
        parseInt(css[2] + css[2], 16) / 255,
        parseInt(css[3] + css[3], 16) / 255,
        1.0,
      ];
    }
    return [
      parseInt(css.slice(1, 3), 16) / 255,
      parseInt(css.slice(3, 5), 16) / 255,
      parseInt(css.slice(5, 7), 16) / 255,
      1.0,
    ];
  }
  return [1, 1, 1, 1];
}
