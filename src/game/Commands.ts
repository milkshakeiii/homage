import type { Ship } from './Ship';
import type { SteeringOutput, AgentState } from './Steering';
import { seek, arrive, pursue, orbit, keepAtRange, blendSteering } from './Steering';
import { updateShipPhysics } from './ShipPhysics';
import type { Vec2 } from '../utils/Math';

// === Command types (discriminated union) ===

export type Command =
  | { type: 'move'; target: Vec2 }
  | { type: 'collisionCourse'; targetShip: Ship }
  | { type: 'evasive'; baseDir: number; jinkTimer: number; jinkAngle: number }
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

    case 'evasive': {
      const baseTarget: Vec2 = {
        x: ship.x + Math.cos(command.baseDir) * 500,
        y: ship.y + Math.sin(command.baseDir) * 500,
      };
      const jinkTarget: Vec2 = {
        x: ship.x + Math.cos(command.baseDir + command.jinkAngle) * 300,
        y: ship.y + Math.sin(command.baseDir + command.jinkAngle) * 300,
      };
      return blendSteering([
        { output: seek(agent, baseTarget), weight: 0.4 },
        { output: seek(agent, jinkTarget), weight: 0.6 },
      ]);
    }

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

/** Update command state (timers) then apply physics */
export function updateCommand(ship: Ship, dt: number): void {
  if (!ship.command || !ship.physicsConfig) return;

  const cmd = ship.command;

  // Evasive jink timer
  if (cmd.type === 'evasive') {
    cmd.jinkTimer -= dt;
    if (cmd.jinkTimer <= 0) {
      cmd.jinkTimer = 0.3 + Math.random() * 0.5;
      cmd.jinkAngle = (Math.random() - 0.5) * Math.PI * 1.5;
    }
  }

  const steering = resolveCommand(ship, cmd);
  updateShipPhysics(ship, steering, dt);
}
