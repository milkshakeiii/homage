# Homage — Design Decisions

The [README's Vision section](README.md#vision) is the north star. This document
turns it into concrete, implementable decisions. Every decision has a status:

- **PROPOSED** — Claude's default; implemented if not vetoed, but easy to change.
- **APPROVED** — Henry has signed off; changing it requires a conversation.
- **OPEN** — genuinely undecided; needs Henry's input before implementation
  reaches it.

Numbers (costs, HP, speeds) are all tuning placeholders — treat them as orders
of magnitude, not commitments.

## 1. Match structure & win condition

**Win condition (PROPOSED):** destroy the enemy mothership. No score timeout,
no secondary objectives — the mothership is the single thing each team defends
and the single thing each team must kill. Comebacks stay possible because a
team that has lost every carrier can still rebuild from the mothership.

**Teams (PROPOSED):** two teams, up to 32 players each. Players are assigned to
the smaller team on connect (team choice / parties can come much later).
Uneven teams are fine during development.

**Match lifecycle (PROPOSED):** for now, a match is just "the server is up."
Mothership dies → announce winner → reset the world after a short pause.
Lobbies, map votes, etc. are out of scope.

## 2. The mothership

**What it is (PROPOSED):** an unpiloted team structure — nobody flies it. It
sits at the team's anchor point. (A slow, team-steered mothership is flavorful
but raises "who steers it" questions the no-commander design deliberately
avoids; revisit after the first playable.)

**What it does (APPROVED — from vision):** it is the only place carrier-type
hulls can be purchased, and it is the default resource dropoff.

**Cold start / bootstrap (PROPOSED):** the vision says non-carrier ships spawn
from carriers, which leaves tick zero (no carriers yet) undefined. Proposal:
the two *economy* hulls — the free starter fighter and the harvester — can
always spawn at the mothership itself. Combat hulls (corvette, bomber, and up)
require a real carrier. This bootstraps the match, and doubles as a comeback
mechanic: a team that loses every carrier is knocked back to fighters and
harvesters, not eliminated.

**Durability (PROPOSED):** enormous HP pool, no regeneration in the first
playable (regen or repairable-by-teammates is a good later knob). Killing it
should require a sustained, committed assault by capital-killer hulls — not
fighter chip damage. Possibly flat damage reduction against small weapons so
bombers/frigates are *required*, not just efficient.

## 3. Economy — resources

**Source (PROPOSED):** asteroids. Shooting an asteroid cracks it into ore
fragments that scatter ballistically; flying into a fragment scoops it. The
skill expression is real: crack the rock so the fragments scatter toward your
team, chase drifting fragments, thread a debris field under fire. No
click-and-wait mining beam.

**Cargo (PROPOSED):** every hull has a cargo capacity; the harvester's dwarfs
everyone else's (order of 10x a fighter's). Carried ore adds mass — a loaded
harvester accelerates and turns noticeably worse, so hauling home through
hostile space is itself a piloting problem, and "one more rock before I head
back" is a real gamble.

**Deposit (APPROVED — from vision):** resources only count once deposited at a
dropoff: the mothership, a resource controller, or (later) a fleet carrier.
Deposit works by flying within the dropoff's radius; the transfer takes a
couple of seconds rather than being instant, so a harvester at a forward
dropoff is briefly stationary and vulnerable.

**Death drops cargo (PROPOSED):** dying with undeposited ore ejects it as
scoopable fragments — recoverable by your team, or stolen by the enemy who
killed you. Deposited resources are never lost (vision).

**Personal, not pooled (PROPOSED):** deposited resources go to the depositing
player's personal bank, matching the vision's "players bring their resources
back there if they want to build one of the carrier-type hulls." No team pool;
generosity happens socially ("escort me and the next carrier is yours" — or
simply gifting, later). Flag this one if you wanted a shared team economy.

## 4. Points & fitting

**Earning (PROPOSED):** points are awarded automatically for team-positive
actions. First playable set: damage dealt (per point of damage), kill bounties
(scaled by victim hull class), and resource deposits (so pure haulers level
too). Later: assists, escort/repair, spawn-hosting as a carrier.

**Spending (PROPOSED):** points buy permanent-for-the-match unlocks of
fittings — alternate weapons, utility modules, hull mods. Once unlocked, a
fitting is yours for the rest of the match and can be equipped on any
compatible hull at every spawn, free. This honors both halves of the vision's
"points are spent on fitting out your ship" and "points stick with you through
deaths": what you *spend* is the unlock; nothing is re-bought per life. (The
alternative — Savage-style re-buying fittings each life — is harsher and adds
a per-death shop chore; rejected as un-arcade-y.)

**Slots (PROPOSED):** each hull has a weapon slot, a utility slot, and a hull
mod slot. First playable ships just the weapon slot with 2–3 options per hull
(e.g. fighter: pulse gun / spread gun / long-lance).

## 5. Hull roster

Costs are paid from the buyer's personal resource bank. "Spawns at" is where a
player can take spawn in one.

| Hull | Class | Role | Built/bought at | Spawns at | First playable? |
|---|---|---|---|---|---|
| Starter fighter | small | scout, harass, free default | free | mothership + carriers | yes |
| Harvester | small | ore hauling; big cargo, weak gun | any spawn point | mothership + carriers | yes |
| Corvette | small | anti-fighter screen | carrier | carriers | yes |
| Bomber | small | anti-capital torpedoes, helpless vs fighters | carrier | carriers | yes |
| Missile boat | medium | area anti-fighter, slow | carrier | carriers | later |
| Megalaser frigate | large | capital-killer, ponderous | carrier | carriers | later |
| Resource controller | carrier-type | mobile dropoff, lightly armed | mothership | n/a (piloted from purchase) | yes |
| Escort carrier | carrier-type | forward spawn point for combat hulls | mothership | n/a | yes |
| Fleet carrier | carrier-type | spawn point + dropoff + heavy | mothership | n/a | later |

**Carrier-type hulls are piloted (PROPOSED):** buying a resource controller or
carrier puts *you* in it — it is your ship, per the vision's "carrier captain"
role. Its value to the team (forward spawns, forward dropoff) exists only
while you keep it alive and well-positioned.

**Rock-paper-scissors by geometry, not damage tables (PROPOSED):** roles
emerge from physics — turn rates, projectile speeds, hitbox sizes — rather
than "+50% vs class X" multipliers. A bomber loses to a fighter because its
torpedoes can't track something that fast, not because a table says so.
This keeps everything skill-expressive: a great bomber pilot *can* clip a
careless fighter.

## 6. Spawning & death

**Spawn flow (PROPOSED):** while dead you see the map and pick a spawn point
from your team's eligible facilities, then pick a hull you can afford (starter
fighter is always free). Respawn delay ~3 s for small hulls, scaling up with
hull class.

**Hulls are lost on death (PROPOSED):** a purchased hull is gone when it dies;
buy another. Combined with permanent fitting unlocks and banked
resources/points, death costs you *equipment* but never *progress* — which
keeps big-ship losses meaningful (economy) while staying arcade about it.

## 7. Map (first playable)

**One symmetric arena (PROPOSED):** roughly 12000×8000 units, motherships at
opposite ends, a dense contested asteroid belt in the middle, and safer,
thinner belts near each mothership so bootstrap harvesting isn't instantly
contested. Soft boundary (gentle push-back force, screen-edge warning) rather
than a wall or wraparound.

## 8. Roadmap

- **M0 — foundation (done):** flight + combat netcode: prediction, prespawned
  bullets, lag-compensated hits, respawn.
- **M1 — economy loop:** teams; mothership as structure + dropoff; asteroids
  that crack into scoopable fragments; cargo with mass penalty; deposit;
  personal resource bank; cargo drop on death. *Verified end-to-end by the
  headless integration harness.*
- **M2 — construction & spawning:** hull purchase; distinct hull stats;
  carrier-type hulls as pilotable ships; spawn-point selection; carrier spawn
  hosting; cold-start rules.
- **M3 — points & fittings:** point awards; fitting unlocks; weapon slot with
  2–3 options per hull.
- **M4 — win condition & breadth:** mothership HP/kill flow, match reset,
  remaining roster (missile boat, megalaser frigate, fleet carrier), map
  balance pass.

## 9. Explicitly out of scope (for now)

Matchmaking/lobbies, cross-match persistence, art & sound (gizmo rendering is
fine), text/voice comms, spectating, anti-cheat beyond server authority.
