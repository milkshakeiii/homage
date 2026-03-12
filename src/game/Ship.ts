import { parseColor } from '../utils/Color';
import { ShipPartBuffers, WebGLRenderer } from '../renderer/WebGLRenderer';
import { ShipPhysicsConfig, getPhysicsConfig } from './ShipPhysics';
import type { Command } from './Commands';

export interface ShipPartDef {
  verts: number[][];
  stroke: string;
  fill: string;
  width: number;
}

export interface ShipDef {
  parts: ShipPartDef[];
  length: number;
  name: string;
  color: string;
}

export interface Ship {
  def: ShipDef;
  buffers: ShipPartBuffers[];
  x: number;
  y: number;
  vx: number;
  vy: number;
  angle: number;
  angularVel: number;
  health: number;
  maxHealth: number;
  ability: AbilityState | null;
  abilityName: string;
  physicsConfig?: ShipPhysicsConfig;
  command?: Command;
  evasive?: { jinkTimer: number; jinkAngle: number };
}

export type AbilityState =
  | { type: 'afterburner'; timer: number; max: number }
  | { type: 'missiles'; timer: number; max: number; missiles: Missile[] }
  | { type: 'shield'; timer: number; max: number }
  | { type: 'broadside'; timer: number; max: number; flashes: BroadsideFlash[] }
  | { type: 'lance'; timer: number; max: number }
  | { type: 'fighters'; timer: number; max: number; fighters: Fighter[] };

export interface Missile {
  x: number; y: number;
  vx: number; vy: number;
  trail: { x: number; y: number }[];
  life: number;
}

export interface BroadsideFlash {
  timer: number;
  x: number; yTop: number; yBot: number;
}

export interface Fighter {
  x: number; y: number;
  vx: number; vy: number;
  angle: number;
  life: number;
}

const ABILITIES = ['AFTERBURNER', 'MISSILE SALVO', 'SHIELD BUBBLE', 'BROADSIDE', 'ENERGY LANCE', 'DEPLOY FIGHTERS'];

export function createShip(def: ShipDef, renderer: WebGLRenderer, x: number, y: number, abilityIdx: number): Ship {
  const buffers = def.parts.map(p =>
    renderer.createShipPartBuffers(p.verts, parseColor(p.stroke), parseColor(p.fill), p.width)
  );
  return {
    def, buffers, x, y, vx: 0, vy: 0, angle: 0, angularVel: 0,
    health: 100, maxHealth: 100,
    ability: null, abilityName: ABILITIES[abilityIdx] ?? 'AFTERBURNER',
    physicsConfig: getPhysicsConfig(def.name),
  };
}

export function triggerAbility(ship: Ship) {
  if (ship.ability) return;
  switch (ship.abilityName) {
    case 'AFTERBURNER': ship.ability = { type: 'afterburner', timer: 0, max: 90 }; break;
    case 'MISSILE SALVO': ship.ability = { type: 'missiles', timer: 0, max: 120, missiles: [] }; break;
    case 'SHIELD BUBBLE': ship.ability = { type: 'shield', timer: 0, max: 150 }; break;
    case 'BROADSIDE': ship.ability = { type: 'broadside', timer: 0, max: 100, flashes: [] }; break;
    case 'ENERGY LANCE': ship.ability = { type: 'lance', timer: 0, max: 120 }; break;
    case 'DEPLOY FIGHTERS': ship.ability = { type: 'fighters', timer: 0, max: 180, fighters: [] }; break;
  }
}

// === Ship definitions ===

export function makeSkiff(): ShipDef {
  return {
    parts: [
      { verts: [[5,0],[3,1.2],[1,1.8],[-3,2.5],[-5,2],[-4.5,0.8],[-5,0],[-4.5,-0.8],[-5,-2],[-3,-2.5],[1,-1.8],[3,-1.2]], stroke:'#0ff', fill:'rgba(0,255,255,0.08)', width:1.5 },
      { verts: [[4,0],[2.5,0.8],[1,0.6],[1,-0.6],[2.5,-0.8]], stroke:'#0ff', fill:'rgba(0,255,255,0.25)', width:1 },
      { verts: [[-5,1.2],[-6.5,1.5],[-6.5,0.5],[-5,0.8]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1 },
      { verts: [[-5,-0.8],[-6.5,-0.5],[-6.5,-1.5],[-5,-1.2]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1 },
      { verts: [[0,1.8],[-1,3.5],[-3,3.2],[-2,2.2]], stroke:'#088', fill:'rgba(0,136,136,0.15)', width:1 },
      { verts: [[0,-1.8],[-1,-3.5],[-3,-3.2],[-2,-2.2]], stroke:'#088', fill:'rgba(0,136,136,0.15)', width:1 },
    ],
    length: 10, name: 'SKIFF', color: '#0ff',
  };
}

export function makeCorvette(): ShipDef {
  return {
    parts: [
      { verts: [[25,0],[20,4],[12,6],[5,7],[-10,8],[-20,6],[-25,3],[-25,-3],[-20,-6],[-10,-8],[5,-7],[12,-6],[20,-4]], stroke:'#f0f', fill:'rgba(255,0,255,0.06)', width:1.5 },
      { verts: [[10,3],[5,5],[-2,5],[-5,3],[-5,-3],[-2,-5],[5,-5],[10,-3]], stroke:'#f0f', fill:'rgba(255,0,255,0.15)', width:1 },
      { verts: [[22,3],[28,2.5],[28,4],[22,4.5]], stroke:'#f88', fill:'rgba(255,136,136,0.3)', width:1 },
      { verts: [[22,-4.5],[28,-4],[28,-2.5],[22,-3]], stroke:'#f88', fill:'rgba(255,136,136,0.3)', width:1 },
      { verts: [[-25,5],[-32,4],[-32,-4],[-25,-5]], stroke:'#f80', fill:'rgba(255,136,0,0.2)', width:1.5 },
    ],
    length: 50, name: 'CORVETTE', color: '#f0f',
  };
}

export function makeFrigate(): ShipDef {
  return {
    parts: [
      { verts: [[75,0],[65,8],[50,12],[20,15],[-20,16],[-50,14],[-70,10],[-75,5],[-75,-5],[-70,-10],[-50,-14],[-20,-16],[20,-15],[50,-12],[65,-8]], stroke:'#0f0', fill:'rgba(0,255,0,0.05)', width:2 },
      { verts: [[30,6],[15,10],[-5,10],[-15,7],[-15,-7],[-5,-10],[15,-10],[30,-6]], stroke:'#0f0', fill:'rgba(0,255,0,0.12)', width:1.5 },
      { verts: [[40,8],[35,12],[25,12],[20,8]], stroke:'#8f8', fill:'rgba(136,255,136,0.2)', width:1 },
      { verts: [[40,-8],[35,-12],[25,-12],[20,-8]], stroke:'#8f8', fill:'rgba(136,255,136,0.2)', width:1 },
      { verts: [[-75,8],[-85,6],[-85,2],[-75,3]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1 },
      { verts: [[-75,-3],[-85,-2],[-85,-6],[-75,-8]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1 },
    ],
    length: 150, name: 'FRIGATE', color: '#0f0',
  };
}

export function makeDestroyer(): ShipDef {
  const gunsL: ShipPartDef[] = [], gunsR: ShipPartDef[] = [];
  for (let i = 0; i < 6; i++) {
    const x = 120 - i * 50;
    gunsL.push({ verts: [[x,55+i*2],[x+10,65+i*2],[x+5,78+i*2],[x-5,78+i*2],[x-10,65+i*2]], stroke:'#ff8', fill:'rgba(255,255,136,0.15)', width:1 });
    gunsR.push({ verts: [[x,-55-i*2],[x+10,-65-i*2],[x+5,-78-i*2],[x-5,-78-i*2],[x-10,-65-i*2]], stroke:'#ff8', fill:'rgba(255,255,136,0.15)', width:1 });
  }
  const engines: ShipPartDef[] = [];
  for (let i = -3; i <= 3; i++) {
    engines.push({ verts: [[-200,i*9+4],[-218,i*9+3],[-218,i*9-3],[-200,i*9-4]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1 });
  }
  return {
    parts: [
      { verts: [[200,0],[180,25],[120,50],[40,65],[-40,70],[-120,68],[-180,55],[-200,35],[-200,-35],[-180,-55],[-120,-68],[-40,-70],[40,-65],[120,-50],[180,-25]], stroke:'#ff0', fill:'rgba(255,255,0,0.04)', width:2 },
      { verts: [[200,0],[190,18],[160,30],[120,30],[120,-30],[160,-30],[190,-18]], stroke:'#ff0', fill:'rgba(255,255,0,0.1)', width:1.5 },
      { verts: [[60,25],[30,45],[-30,48],[-60,40],[-60,-40],[-30,-48],[30,-45],[60,-25]], stroke:'#ff0', fill:'rgba(255,255,0,0.1)', width:1.5 },
      { verts: [[100,8],[50,12],[-50,12],[-100,8],[-100,-8],[-50,-12],[50,-12],[100,-8]], stroke:'#880', fill:'rgba(136,136,0,0.08)', width:1 },
      { verts: [[80,50],[20,62],[-60,65],[-120,60],[-100,50],[-20,48],[60,42]], stroke:'#aa0', fill:'rgba(170,170,0,0.06)', width:1 },
      { verts: [[80,-50],[20,-62],[-60,-65],[-120,-60],[-100,-50],[-20,-48],[60,-42]], stroke:'#aa0', fill:'rgba(170,170,0,0.06)', width:1 },
      ...gunsL, ...gunsR, ...engines,
    ],
    length: 400, name: 'DESTROYER', color: '#ff0',
  };
}

export function makeCruiser(): ShipDef {
  const engines: ShipPartDef[] = [];
  for (const baseY of [-70, -90, 70, 90]) {
    engines.push({ verts: [[-400,baseY+6],[-430,baseY+4],[-430,baseY-4],[-400,baseY-6]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:1.5 });
  }
  return {
    parts: [
      { verts: [[400,-60],[380,-40],[300,-30],[100,-25],[-100,-25],[-300,-30],[-380,-40],[-400,-60],[-400,-100],[-380,-110],[-300,-120],[-100,-125],[100,-125],[300,-120],[380,-110],[400,-100]], stroke:'#f80', fill:'rgba(255,136,0,0.04)', width:2.5 },
      { verts: [[400,60],[380,40],[300,30],[100,25],[-100,25],[-300,30],[-380,40],[-400,60],[-400,100],[-380,110],[-300,120],[-100,125],[100,125],[300,120],[380,110],[400,100]], stroke:'#f80', fill:'rgba(255,136,0,0.04)', width:2.5 },
      { verts: [[120,-25],[120,25],[-120,25],[-120,-25]], stroke:'#fa0', fill:'rgba(255,170,0,0.06)', width:2 },
      { verts: [[80,-18],[80,18],[-80,18],[-80,-18]], stroke:'#fa0', fill:'rgba(255,170,0,0.12)', width:1.5 },
      { verts: [[400,-80],[430,-70],[430,-90],[400,-100]], stroke:'#f80', fill:'rgba(255,136,0,0.1)', width:1.5 },
      { verts: [[400,80],[430,70],[430,90],[400,100]], stroke:'#f80', fill:'rgba(255,136,0,0.1)', width:1.5 },
      { verts: [[120,-12],[200,-8],[200,8],[120,12]], stroke:'#f44', fill:'rgba(255,68,68,0.15)', width:1.5 },
      { verts: [[250,-100],[240,-130],[220,-130],[210,-100]], stroke:'#f88', fill:'rgba(255,136,136,0.15)', width:1 },
      { verts: [[-100,-100],[-110,-130],[-130,-130],[-140,-100]], stroke:'#f88', fill:'rgba(255,136,136,0.15)', width:1 },
      { verts: [[250,100],[240,130],[220,130],[210,100]], stroke:'#f88', fill:'rgba(255,136,136,0.15)', width:1 },
      { verts: [[-100,100],[-110,130],[-130,130],[-140,100]], stroke:'#f88', fill:'rgba(255,136,136,0.15)', width:1 },
      ...engines,
    ],
    length: 800, name: 'CRUISER', color: '#f80',
  };
}

export function makeMothership(): ShipDef {
  const R = 900;
  const hull: number[][] = [], inner: number[][] = [], bridge: number[][] = [];
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2 - Math.PI / 6;
    hull.push([Math.cos(a) * R, Math.sin(a) * R]);
    inner.push([Math.cos(a) * R * 0.5, Math.sin(a) * R * 0.5]);
    bridge.push([Math.cos(a) * R * 0.2, Math.sin(a) * R * 0.2]);
  }
  const spokes: ShipPartDef[] = [];
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2 - Math.PI / 6;
    const cos = Math.cos(a), sin = Math.sin(a), w = 25;
    const pa = a + Math.PI / 2;
    const dx = Math.cos(pa) * w, dy = Math.sin(pa) * w;
    spokes.push({ verts: [
      [cos*R*0.52+dx,sin*R*0.52+dy],[cos*R*0.95+dx,sin*R*0.95+dy],
      [cos*R*0.95-dx,sin*R*0.95-dy],[cos*R*0.52-dx,sin*R*0.52-dy]
    ], stroke:'#66c', fill:'rgba(100,100,200,0.06)', width:1.5 });
  }
  const bays: ShipPartDef[] = [];
  for (let i = 0; i < 6; i++) {
    const a1 = (i / 6) * Math.PI * 2 - Math.PI / 6;
    const a2 = ((i + 1) / 6) * Math.PI * 2 - Math.PI / 6;
    const mid = (a1 + a2) / 2;
    const bx = Math.cos(mid) * R * 0.72, by = Math.sin(mid) * R * 0.72;
    const pa = mid + Math.PI / 2, bw = 60, bh = 30;
    bays.push({ verts: [
      [bx+Math.cos(mid)*bh+Math.cos(pa)*bw,by+Math.sin(mid)*bh+Math.sin(pa)*bw],
      [bx-Math.cos(mid)*bh+Math.cos(pa)*bw,by-Math.sin(mid)*bh+Math.sin(pa)*bw],
      [bx-Math.cos(mid)*bh-Math.cos(pa)*bw,by-Math.sin(mid)*bh-Math.sin(pa)*bw],
      [bx+Math.cos(mid)*bh-Math.cos(pa)*bw,by+Math.sin(mid)*bh-Math.sin(pa)*bw]
    ], stroke:'#48f', fill:'rgba(68,136,255,0.12)', width:1.5 });
  }
  const engines: ShipPartDef[] = [];
  for (let i = -3; i <= 3; i++) {
    const ey = i * 30;
    engines.push({ verts: [[-R-10,ey+12],[-R-55,ey+8],[-R-55,ey-8],[-R-10,ey-12]], stroke:'#f80', fill:'rgba(255,136,0,0.3)', width:2 });
  }
  return {
    parts: [
      { verts: hull, stroke:'#88f', fill:'rgba(136,136,255,0.03)', width:3 },
      { verts: inner, stroke:'#88f', fill:'rgba(136,136,255,0.05)', width:2 },
      { verts: bridge, stroke:'#aaf', fill:'rgba(170,170,255,0.1)', width:2 },
      ...spokes, ...bays,
      { verts: [[0,-R-10],[-20,-R-60],[20,-R-60]], stroke:'#ff0', fill:'rgba(255,255,0,0.15)', width:1 },
      { verts: [[0,R+10],[-20,R+60],[20,R+60]], stroke:'#ff0', fill:'rgba(255,255,0,0.15)', width:1 },
      ...engines,
    ],
    length: 2000, name: 'MOTHERSHIP', color: '#88f',
  };
}

export function allShipDefs(): ShipDef[] {
  return [makeSkiff(), makeCorvette(), makeFrigate(), makeDestroyer(), makeCruiser(), makeMothership()];
}
