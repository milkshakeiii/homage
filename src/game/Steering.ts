import { Vec2, v2Sub, v2Add, v2Scale, v2Norm, v2Len, angleDiff, clamp } from '../utils/Math';

export interface SteeringOutput {
  force: Vec2;
  thrust: number;   // 0..1
  desiredAngle: number;
}

export interface AgentState {
  pos: Vec2;
  vel: Vec2;
  angle: number;
  maxSpeed: number;
  maxForce: number;
}

function steer(force: Vec2, thrust: number): SteeringOutput {
  const desiredAngle = (force.x !== 0 || force.y !== 0)
    ? Math.atan2(force.y, force.x)
    : 0;
  return { force, thrust: clamp(thrust, 0, 1), desiredAngle };
}

/** Full speed toward target point */
export function seek(agent: AgentState, target: Vec2): SteeringOutput {
  const desired = v2Sub(target, agent.pos);
  const dir = v2Norm(desired);
  return steer(v2Scale(dir, agent.maxForce), 1);
}

/** Decelerate near target */
export function arrive(agent: AgentState, target: Vec2, slowRadius = 150): SteeringOutput {
  const toTarget = v2Sub(target, agent.pos);
  const dist = v2Len(toTarget);
  if (dist < 1) return steer({ x: 0, y: 0 }, 0);

  const dir = v2Scale(toTarget, 1 / dist);
  const desiredSpeed = dist < slowRadius
    ? agent.maxSpeed * (dist / slowRadius)
    : agent.maxSpeed;

  const desired = v2Scale(dir, desiredSpeed);
  const force = v2Sub(desired, agent.vel);
  const fLen = v2Len(force);
  const clamped = fLen > agent.maxForce ? v2Scale(force, agent.maxForce / fLen) : force;
  const thrust = clamp(desiredSpeed / agent.maxSpeed, 0, 1);
  return steer(clamped, thrust);
}

/** Away from threat */
export function flee(agent: AgentState, threat: Vec2): SteeringOutput {
  const away = v2Sub(agent.pos, threat);
  const dir = v2Norm(away);
  return steer(v2Scale(dir, agent.maxForce), 1);
}

/** Intercept predicted position */
export function pursue(agent: AgentState, targetPos: Vec2, targetVel: Vec2): SteeringOutput {
  const toTarget = v2Sub(targetPos, agent.pos);
  const dist = v2Len(toTarget);
  const lookAhead = dist / agent.maxSpeed;
  const predicted = v2Add(targetPos, v2Scale(targetVel, lookAhead));
  return seek(agent, predicted);
}

/** Flee predicted position */
export function evade(agent: AgentState, pursuerPos: Vec2, pursuerVel: Vec2): SteeringOutput {
  const toP = v2Sub(pursuerPos, agent.pos);
  const dist = v2Len(toP);
  const lookAhead = dist / agent.maxSpeed;
  const predicted = v2Add(pursuerPos, v2Scale(pursuerVel, lookAhead));
  return flee(agent, predicted);
}

/** Tangent-seeking orbit around center */
export function orbit(agent: AgentState, center: Vec2, radius: number, clockwise: boolean): SteeringOutput {
  const toCenter = v2Sub(center, agent.pos);
  const dist = v2Len(toCenter);
  if (dist < 1) return seek(agent, v2Add(center, { x: radius, y: 0 }));

  const radDir = v2Scale(toCenter, 1 / dist);
  // Tangent direction (perpendicular to radial)
  const tangent: Vec2 = clockwise
    ? { x: radDir.y, y: -radDir.x }
    : { x: -radDir.y, y: radDir.x };

  // Blend tangent + correction toward desired radius
  const radiusError = (dist - radius) / radius;
  const correctionWeight = clamp(radiusError * 2, -1, 1);

  const desired = v2Norm(v2Add(tangent, v2Scale(radDir, correctionWeight)));
  return steer(v2Scale(desired, agent.maxForce), 1);
}

/** Maintain distance band from target */
export function keepAtRange(
  agent: AgentState, target: Vec2, targetVel: Vec2,
  range: number, tolerance = 50
): SteeringOutput {
  const toTarget = v2Sub(target, agent.pos);
  const dist = v2Len(toTarget);

  if (dist < 1) return flee(agent, target);

  const dir = v2Scale(toTarget, 1 / dist);

  if (dist < range - tolerance) {
    // Too close — back away
    return steer(v2Scale(dir, -agent.maxForce), 1);
  } else if (dist > range + tolerance) {
    // Too far — close in, predict target
    const lookAhead = (dist - range) / agent.maxSpeed;
    const predicted = v2Add(target, v2Scale(targetVel, lookAhead * 0.5));
    return seek(agent, predicted);
  }

  // In band — match target velocity, orbit slightly
  const tangent: Vec2 = { x: -dir.y, y: dir.x };
  const lateral = v2Scale(tangent, agent.maxForce * 0.3);
  const matchVel = v2Sub(targetVel, agent.vel);
  const force = v2Add(lateral, v2Scale(matchVel, 0.5));
  const fLen = v2Len(force);
  const clamped = fLen > agent.maxForce ? v2Scale(force, agent.maxForce / fLen) : force;
  return steer(clamped, 0.3);
}

/** Weighted blend of multiple steering outputs */
export function blendSteering(outputs: { output: SteeringOutput; weight: number }[]): SteeringOutput {
  let fx = 0, fy = 0, thrust = 0, totalWeight = 0;
  let baseAngle = outputs.length > 0 ? outputs[0].output.desiredAngle : 0;
  let angleDeltaSum = 0;

  for (const { output, weight } of outputs) {
    fx += output.force.x * weight;
    fy += output.force.y * weight;
    thrust += output.thrust * weight;
    angleDeltaSum += angleDiff(baseAngle, output.desiredAngle) * weight;
    totalWeight += weight;
  }

  if (totalWeight > 0) {
    const inv = 1 / totalWeight;
    fx *= inv;
    fy *= inv;
    thrust *= inv;
    angleDeltaSum *= inv;
  }

  return {
    force: { x: fx, y: fy },
    thrust: clamp(thrust, 0, 1),
    desiredAngle: baseAngle + angleDeltaSum,
  };
}
