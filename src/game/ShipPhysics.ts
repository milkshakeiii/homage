import { angleDiff, clamp } from '../utils/Math';
import type { Ship } from './Ship';
import type { SteeringOutput } from './Steering';

export interface ShipPhysicsConfig {
  thrust: number;
  maxSpeed: number;
  turnRate: number;
  drag: number;
  mass: number;
  radius: number;
}

const PHYSICS_TABLE: Record<string, ShipPhysicsConfig> = {
  SKIFF:      { thrust: 120, maxSpeed: 200, turnRate: 4.0,  drag: 1.5, mass: 10,    radius: 5 },
  CORVETTE:   { thrust: 80,  maxSpeed: 150, turnRate: 2.5,  drag: 1.2, mass: 50,    radius: 25 },
  FRIGATE:    { thrust: 50,  maxSpeed: 100, turnRate: 1.5,  drag: 1.0, mass: 200,   radius: 75 },
  DESTROYER:  { thrust: 35,  maxSpeed: 70,  turnRate: 0.8,  drag: 0.8, mass: 800,   radius: 200 },
  CRUISER:    { thrust: 25,  maxSpeed: 50,  turnRate: 0.4,  drag: 0.6, mass: 2000,  radius: 400 },
  MOTHERSHIP: { thrust: 15,  maxSpeed: 30,  turnRate: 0.15, drag: 0.4, mass: 10000, radius: 900 },
};

export function getPhysicsConfig(name: string): ShipPhysicsConfig {
  return PHYSICS_TABLE[name] ?? PHYSICS_TABLE.FRIGATE;
}

export function updateShipPhysics(ship: Ship, steering: SteeringOutput, dt: number): void {
  const cfg = ship.physicsConfig;
  if (!cfg) return;

  // 1. Rotate toward desired angle
  const aDiff = angleDiff(ship.angle, steering.desiredAngle);
  const maxTurn = cfg.turnRate * dt;
  ship.angularVel = clamp(aDiff, -maxTurn, maxTurn) / dt;
  ship.angle += ship.angularVel * dt;

  // 2. Apply thrust in current facing direction
  const thrustMag = cfg.thrust * clamp(steering.thrust, 0, 1);
  const ax = Math.cos(ship.angle) * thrustMag;
  const ay = Math.sin(ship.angle) * thrustMag;
  ship.vx += ax * dt;
  ship.vy += ay * dt;

  // 3. Clamp to max speed
  const speed = Math.sqrt(ship.vx * ship.vx + ship.vy * ship.vy);
  if (speed > cfg.maxSpeed) {
    const s = cfg.maxSpeed / speed;
    ship.vx *= s;
    ship.vy *= s;
  }

  // 4. Apply drag
  const dragFactor = 1 - cfg.drag * dt;
  ship.vx *= dragFactor;
  ship.vy *= dragFactor;

  // 5. Integrate position
  ship.x += ship.vx * dt;
  ship.y += ship.vy * dt;
}
