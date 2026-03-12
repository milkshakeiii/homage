import type { Ship } from './Ship';
import type { SteeringOutput, AgentState } from './Steering';
import { seek, arrive, pursue, orbit, keepAtRange, blendSteering } from './Steering';
import { updateShipPhysics } from './ShipPhysics';
import type { Vec2 } from '../utils/Math';

// === Command types (discriminated union) ===

export type Command =
  | { type: 'move'; target: Vec2 }
  | { type: 'collisionCourse'; targetShip: Ship }
  | { type: 'orbit'; targetShip: Ship; radius: number; clockwise: boolean }
  | { type: 'keepAtRange'; targetShip: Ship; range: number };

// === Helper: build AgentState from Ship ===

function agentFromShip(ship: Ship): AgentState {
  const cfg = ship.physicsConfig!;
  return {
    pos: { x: ship.x, y: ship.y },
    vel: { x: ship.vx, y: ship.vy },
    angle: ship.angle,
    maxSpeed: cfg.maxSpeed,
    maxForce: cfg.thrust,
  };
}

// === Resolve command to steering output ===

export function resolveCommand(ship: Ship, command: Command): SteeringOutput {
  const agent = agentFromShip(ship);

  switch (command.type) {
    case 'move':
      return arrive(agent, command.target);

    case 'collisionCourse':
      return pursue(agent,
        { x: command.targetShip.x, y: command.targetShip.y },
        { x: command.targetShip.vx, y: command.targetShip.vy }
      );

    case 'orbit':
      return orbit(agent,
        { x: command.targetShip.x, y: command.targetShip.y },
        command.radius, command.clockwise
      );

    case 'keepAtRange':
      return keepAtRange(agent,
        { x: command.targetShip.x, y: command.targetShip.y },
        { x: command.targetShip.vx, y: command.targetShip.vy },
        command.range
      );
  }
}

/** Apply evasive jink on top of base steering */
function applyEvasive(ship: Ship, base: SteeringOutput): SteeringOutput {
  const ev = ship.evasive;
  if (!ev) return base;

  const agent = agentFromShip(ship);
  const jinkTarget: Vec2 = {
    x: ship.x + Math.cos(ship.angle + ev.jinkAngle) * 300,
    y: ship.y + Math.sin(ship.angle + ev.jinkAngle) * 300,
  };
  const jink = seek(agent, jinkTarget);

  return blendSteering([
    { output: base, weight: 0.6 },
    { output: jink, weight: 0.4 },
  ]);
}

/** Update evasive jink timer, resolve command, apply physics */
export function updateCommand(ship: Ship, dt: number): void {
  if (!ship.physicsConfig) return;

  // Tick evasive jink
  if (ship.evasive) {
    ship.evasive.jinkTimer -= dt;
    if (ship.evasive.jinkTimer <= 0) {
      ship.evasive.jinkTimer = 0.3 + Math.random() * 0.5;
      ship.evasive.jinkAngle = (Math.random() - 0.5) * Math.PI * 1.5;
    }
  }

  if (!ship.command) return;

  let steering = resolveCommand(ship, ship.command);
  steering = applyEvasive(ship, steering);
  updateShipPhysics(ship, steering, dt);
}
