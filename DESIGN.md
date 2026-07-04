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

**Win condition (APPROVED):** destroy the enemy mothership. No score timeout,
no secondary objectives — the mothership is the single thing each team defends
and the single thing each team must kill. Comebacks stay possible because a
team that has lost every carrier can still rebuild from the mothership.

**Teams (APPROVED):** two teams, up to 32 players each. Players are assigned to
the smaller team on connect (team choice / parties can come much later).
Uneven teams are fine during development.

**Match lifecycle (APPROVED):** for now, a match is just "the server is up."
Mothership dies → announce winner → reset the world after a short pause.
Lobbies, map votes, etc. are out of scope.

## 2. The mothership

**What it is (APPROVED):** an unpiloted team structure — nobody flies it. It
sits at the team's anchor point. (A slow, team-steered mothership is flavorful
but raises "who steers it" questions the no-commander design deliberately
avoids; revisit after the first playable.)

**What it does (APPROVED — from vision):** it is the only place carrier-type
hulls can be purchased, and it is the default resource dropoff.

**Cold start / bootstrap (APPROVED):** the vision says non-carrier ships spawn
from carriers, which leaves tick zero (no carriers yet) undefined. The two
*economy* hulls — the free starter fighter and the harvester — can always
spawn at the mothership itself. Combat hulls (corvette, bomber, and up)
require a real carrier. This bootstraps the match, and doubles as a comeback
mechanic: a team that loses every carrier is knocked back to fighters and
harvesters, not eliminated.

**Durability (APPROVED):** enormous HP pool, no regeneration in the first
playable (regen or repairable-by-teammates is a good later knob). Killing it
should require a sustained, committed assault by capital-killer hulls — not
fighter chip damage. Possibly flat damage reduction against small weapons so
bombers/frigates are *required*, not just efficient.

## 3. Economy — resources

**Source (APPROVED):** asteroids. Shooting an asteroid cracks it into ore
fragments that scatter ballistically; flying into a fragment scoops it. The
skill expression is real: crack the rock so the fragments scatter toward your
team, chase drifting fragments, thread a debris field under fire. No
click-and-wait mining beam.

*Validated in play (Henry, M1):* forgiving enough for a first try, but the
hone-able skill emerged as intended — crack the rock **while flying toward
it**, then fly a tight loop through the debris before it disperses. Tuning
that changes fragment scatter speed or TTL should preserve that
close-the-distance-then-loop rhythm.

**Cargo (APPROVED):** every hull has a cargo capacity; the harvester's dwarfs
everyone else's (order of 10x a fighter's). Carried ore adds mass — a loaded
harvester accelerates and turns noticeably worse, so hauling home through
hostile space is itself a piloting problem, and "one more rock before I head
back" is a real gamble.

**Deposit (APPROVED — from vision):** resources only count once deposited at a
dropoff: the mothership, a resource controller, or a fleet carrier. Deposit
works by flying within the dropoff's radius; the transfer takes a couple of
seconds rather than being instant, so a harvester at a forward dropoff is
briefly stationary and vulnerable.

**Death drops cargo (APPROVED):** dying with undeposited ore ejects it as
scoopable fragments — recoverable by your team, or stolen by the enemy who
killed you. Deposited resources are never lost (vision).

**Personal, not pooled (APPROVED):** deposited resources go to the depositing
player's personal bank, matching the vision's "players bring their resources
back there if they want to build one of the carrier-type hulls." No team pool;
generosity happens socially ("escort me and the next carrier is yours" — or
simply gifting, later).

## 4. Controls & game feel

### 4.1 Control archetypes (APPROVED — Henry, 2026-07-02)

There is no single control scheme. **How a ship is flown is a hull property**
— the fourth axis of hull identity alongside size, speed, and weapons, and a
big part of why very big and very little ships stay interesting beyond size.
Three archetypes:

- **Pilot** (tank controls — the current scheme): A/D rotate, W thrust,
  weapons fire down the nose. You *are* the weapon; facing and inertia
  management is the whole skill. The asteroids resource game is played this
  way. Hulls: starter fighter, harvester, bomber.
- **Gunship** (turret ship): the hull flies on momentum (thrust/strafe keys)
  while the mouse independently aims a fast-moving turret. You fly a platform
  that carries a weapon; the skill is split between positioning the platform
  and tracking with the turret. Hulls: corvette, missile boat, megalaser
  frigate (with a slow-traverse turret — same scheme, different feel).
- **Captain** (field controller): the ship drifts slowly and thrusts
  omnidirectionally (WASD in screen space, no meaningful facing) while the
  mouse targets *abilities* on the battlefield — point-defense zones, repair
  beams, tractor fields, spawn/deploy placement. Your weapon is the field
  itself. Hulls: resource controller, carriers, outfitter.

Known risk (acknowledged): three input models to learn, tune, and net-predict.
Mitigations: everyone's first ship (starter fighter) is a Pilot hull, so there
is exactly one scheme to learn on day one; archetypes arrive one per milestone
rather than all at once; and all three share the same feel guideposts below.
The archetypes also mark out tech-tree design space: unlocking a hull can mean
unlocking a *new way to play*, not a bigger number.

### 4.2 Feel guideposts (PROPOSED)

The bar: pressing buttons and flying should feel like an addictive studio
arcade game. Reference points: Subspace/Continuum (drift dogfights),
Luftrausers (commitment and recovery), Celeste-school input forgiveness,
Vlambeer-school juice, Homeworld fleet doctrine.

1. **Zero input delay, always.** Every press produces a same-tick predicted
   response plus an immediate cosmetic response (muzzle flash, thruster
   flare). Cosmetics never wait for the server; at 150 ping the *feedback* is
   still instant even when the authoritative result isn't.
2. **Inertia is the skill ceiling; damping is the skill floor.** Newtonian
   drift with enough damping that a mediocre pilot can recover, low enough
   that a good pilot carries speed through turns. Rough bar: fighters reach
   max speed in ~1.5 s, and reversing course is noticeably faster than
   accelerating from rest — mistakes are correctable, commitments are real.
3. **Your velocity is part of your aim.** All projectiles inherit shooter
   velocity; nothing is hitscan; projectile speeds are tuned so leading the
   target is the core aiming skill in every archetype.
4. **Commit buttons, not hold buttons.** Abilities are discrete commitments
   with visible wind-up, travel, and cooldown (dash, burner heat, torpedo
   arming) — mastery is timing, not actions-per-minute.
5. **Forgiveness under the hood.** Input buffering (a fire press during
   cooldown fires on the first legal tick; a tap between fixed ticks still
   registers), slightly generous bullet radii, lenient deposit/refit radii.
   The player must never truthfully say "I pressed it and nothing happened."
6. **Juice without art.** Camera look-ahead in the velocity direction, gentle
   zoom-out with speed, small *capped* screenshake on hits taken and nearby
   deaths, hit flashes, thruster trails, expanding kill rings — all
   gizmo-drawable today. Placeholder synth SFX land with the first juice
   pass, not after art: sound is half of feel.
7. **Readable at a glance.** Every ship telegraphs facing, thrust state, and
   velocity; bullets leave tracers; deaths are unambiguous. Anyone who dies
   should know what killed them without a killcam.
8. **Handling is hull identity.** The *first* thing that distinguishes two
   hulls is how they feel to move — turn rate, acceleration, mass, archetype —
   before weapons or HP. If two hulls feel the same to fly, they're one hull.

### 4.3 Input protocol implication (PROPOSED)

One superset `ShipInput` for all archetypes rather than per-archetype enums:
buttons (thrust/turn/strafe/fire/abilities) + quantized aim angle (u16) +
quantized cursor world-position (for Captain ability targeting). Unused fields
idle at zero for hulls that don't read them. Keeps lightyear input replication
and prediction rollback uniform across archetypes.

## 5. Points, fittings & facilities

**Earning (APPROVED):** points are awarded automatically for team-positive
actions. First playable set: damage dealt (per point of damage), kill bounties
(scaled by victim hull class), resource deposits (so pure haulers level too),
and cracking asteroids (proportional to the ore released — Henry, M3). Later:
assists, escort/repair, spawn-hosting as a carrier.

**Spending (APPROVED):** points buy permanent-for-the-match unlocks of
fittings — alternate weapons, utility modules, hull mods. Once unlocked, a
fitting is yours for the rest of the match; nothing is re-bought per life.

**Slots (APPROVED):** each hull has a weapon slot, a utility slot, and a hull
mod slot. First playable ships just the weapon slot with 2–3 options per hull.

**Facility stocking (PROPOSED):** *unlocking* a fitting costs points and can
be done from anywhere (it's account state), but *equipping* it happens inside
the refit radius of a friendly facility **that stocks it** — and facilities
stock different things (see catalog). Spawning auto-equips your saved loadout
if the spawn facility stocks all of it; anything exclusive gets filled with a
tier-1 default until you visit the right facility. This is the "trade flying
time for an optimized ship" loop: the fully-kitted ship requires a hull from
one facility and modules from another, and a team whose infrastructure is
well-placed turns that trip from a chore into a pit stop.

**Placeholder catalog (PROPOSED — names and effects are all placeholders):**

| Slot | Fitting | Effect | Stocked at | Tier |
|---|---|---|---|---|
| Weapon | Pulse cannon | fighter default | everywhere | 1 |
| Weapon | Scatter gun | close-range spread | any carrier | 1 |
| Weapon | Long-lance railgun | slow RoF, very fast projectile | **strike carrier only** | 2 |
| Weapon | Flak burst | proximity-detonating anti-fighter (corvette) | any carrier | 1 |
| Weapon | Torpedo | bomber default: slow, huge, dumb-fire | any carrier | 1 |
| Weapon | Mag-torpedo | mild tracking, less damage | **strike carrier only** | 2 |
| Utility | Afterburner | heat-limited boost | everywhere | 1 |
| Utility | Blink thruster | impulse dash on cooldown | **outfitter only** | 2 |
| Utility | Shield capacitor | timed damage absorb (active block) | **outfitter only** | 2 |
| Utility | Tractor scoop | wider ore pickup, pulls fragments | resource controller | 1 |
| Utility | Repair drone | slow out-of-combat regen | **outfitter only** | 2 |
| Hull mod | Gyro tuning | +turn rate | everywhere | 1 |
| Hull mod | Armor plate | +HP, +mass | everywhere | 1 |
| Hull mod | Lightweight frame | −HP, +acceleration | **strike carrier only** | 2 |
| Hull mod | Compacted hold | +cargo capacity, worse handling | resource controller | 2 |
| Hull mod | Mag-clamp hold | keep half your cargo on death | resource controller | 2 |

## 6. Hull roster & construction tree

**Construction tree (PROPOSED):** the mothership builds carrier-types;
carriers field the rest; and one carrier can build *sub-carriers* — small
specialist facilities that extend the ships-build-ships tree a level deeper.

```
Mothership ─┬─ Resource controller   dropoff; spawns harvesters; stocks harvest tech
            ├─ Strike carrier        spawns combat hulls; SOLE source of heavy
            │                        combat hulls and tier-2 weapons/frames
            └─ Fleet carrier         dropoff; spawns small combat hulls;
                  │                  SOLE builder of sub-carriers
                  └─ Outfitter       sub-carrier: sells no hulls; SOLE source
                                     of tier-2 utility modules; fast refit
```

The strike carrier and fleet carrier are deliberately *asymmetric* rather than
small/large versions of each other: the strike carrier is where the best
**hulls** come from; the fleet carrier is infrastructure — dropoff, spawns,
and the outfitter line, where the best **modules** come from. A team with only
strike carriers hits hard with unoptimized ships; a team with only fleet
carriers flies tricked-out corvettes but can't field a megalaser frigate.

| Hull | Class | Archetype | Role | Built/bought at | First playable? |
|---|---|---|---|---|---|
| Starter fighter | small | Pilot | scout, harass, free default | free (mothership + carriers) | yes |
| Harvester | small | Pilot | ore hauling; big cargo, weak gun | any spawn point | yes |
| Corvette | small | Gunship | anti-fighter turret platform | carrier | yes |
| Bomber | small | Pilot | anti-capital torpedoes, helpless vs fighters | carrier | yes |
| Missile boat | medium | Gunship | area anti-fighter, slow | carrier | later |
| Megalaser frigate | large | Gunship | capital-killer, slow-traverse turret | **strike carrier** | later |
| Resource controller | carrier-type | Captain | mobile dropoff, lightly armed | mothership | yes |
| Strike carrier | carrier-type | Captain | forward spawn; heavy-hull + weapon source | mothership | yes |
| Fleet carrier | carrier-type | Captain | spawn + dropoff; builds sub-carriers | mothership | later |
| Outfitter | sub-carrier | Captain | forward refit; tier-2 module source | **fleet carrier** | later |

**Carrier-type hulls are piloted (APPROVED):** buying a resource controller,
carrier, or outfitter puts *you* in it — it is your ship, per the vision's
"carrier captain" role. Its value to the team (forward spawns, forward
dropoff, refit range) exists only while you keep it alive and well-positioned.

**Rock-paper-scissors by geometry, not damage tables (APPROVED):** roles
emerge from physics — turn rates, projectile speeds, turret traverse, hitbox
sizes — rather than "+50% vs class X" multipliers. A bomber loses to a fighter
because its torpedoes can't track something that fast, not because a table
says so. This keeps everything skill-expressive: a great bomber pilot *can*
clip a careless fighter.

## 7. Spawning & death

**Spawn flow (APPROVED — Henry, 2026-07-04):** no auto-respawn. Death opens
the **map screen**: click a friendly facility to spawn from (eligibility by
hull class, §2). Then the **loadout screen** (Savage XR's layout: weapons and
items grids with a detail panel on the left; hulls row, big hull preview,
equipped slots, and a SPAWN button on the right; ore + points readouts).
Map-first matters: the chosen facility scopes the shop — which hulls it can
field and which modules it stocks. You can hop back to the map freely; SPAWN
deploys you (the ~3 s respawn delay is only the *earliest* moment — an early
click queues). Omitted from Savage's screen: COMMAND (no commander), Request
(nobody to request from), item stock counts (our stocking is about *where*,
not *how many*). Presets (saved loadouts) are a later nicety.

**Docking & refit (APPROVED — Henry, 2026-07-04):** the loadout screen is
always "the docked UI" — death is just being force-docked with a facility
still to choose. Flying into a friendly facility's refit radius and holding
the dock key stows your ship and opens the same screen; leaving it undocks
you *at that facility* with hull, health, and cargo intact — a refit trip is
a pit stop, not a death tax (the cost is the flying time, per §5). Equipment
changes only happen docked, limited to what the facility stocks; docking at
a dropoff facility deposits your hold while you shop.

**Hulls are lost on death (APPROVED):** a purchased hull is gone when it dies;
buy another. Combined with permanent fitting unlocks and banked
resources/points, death costs you *equipment* but never *progress* — which
keeps big-ship losses meaningful (economy) while staying arcade about it.

## 8. Map (first playable)

**One symmetric arena (APPROVED):** roughly 12000×8000 units, motherships at
opposite ends, a dense contested asteroid belt in the middle, and safer,
thinner belts near each mothership so bootstrap harvesting isn't instantly
contested. Soft boundary (gentle push-back force, screen-edge warning) rather
than a wall or wraparound.

## 9. Tech tree depth (OPEN)

Henry's verdict on the current tree: **not yet interesting enough** —
deliberately deferred rather than settled. The pieces now on the board that
open the space: control archetypes (a hull unlock can be a new way to play),
facility stocking (geography of tech), and sub-carriers (deeper
ships-build-ships chains). Revisit properly once M2 makes facilities real.
Candidate directions parked here: more sub-carrier specializations, hull
variants unlocked by facility combinations, team-level tech unlocked by
holding map features.

## 10. Roadmap

- **M0 — foundation (done):** flight + combat netcode: prediction, prespawned
  bullets, lag-compensated hits, respawn; e2e test harness.
- **M0.5 — controls & feel foundation (done):** superset input protocol
  (§4.3); Pilot archetype polish — input buffering, brake, tuned drift (§4.2
  bars); camera look-ahead + speed zoom; first juice pass (thruster trails,
  hit flash, kill rings, capped shake); placeholder synth SFX; fixed client
  prediction to actually simulate locally (smooth flight, same-frame input
  response). Exit bar met: Henry flew it.
- **M1 — economy loop (done):** teams; mothership as structure + dropoff;
  asteroids that crack into scoopable fragments; cargo with mass penalty;
  deposit; personal resource bank; cargo drop on death. All covered by e2e
  tests. (Harvester handling-under-load re-checks when the harvester hull
  lands in M2.)
- **M2 — construction & spawning (done):** hull purchase on respawn via
  SpawnOrder; per-hull stats (fighter, harvester, corvette, resource
  controller, strike carrier); Gunship archetype (corvette's mouse turret);
  Captain archetype (omnidirectional drift); resource controller as mobile
  dropoff; cold-start rules (combat hulls require a live carrier and spawn
  beside it; carrier-types build at the mothership). Spawn-point
  picking: Tab on the death screen cycles eligible friendly facilities (a
  map-screen version can come later). Deferred: facility stocking + refit
  radius move to M3 with fittings.
- **M3 — points & fittings (done):** point awards (hits, kill bounties by
  hull, deposits, asteroid cracks); fitting unlocks spending points, match-
  permanent, validated against facility stocking with per-slot fallback;
  implemented catalog: pulse cannon / scatter gun / long-lance railgun
  (weapons), afterburner / blink thruster (utility, SHIFT), gyro tuning /
  armor plate / lightweight frame / compacted hold (hull mods). Stocking
  deviations until the outfitter + docking exist: outfitter-stocked items
  live at the strike carrier, resource-controller items at any carrier.
- **M3.5 — awareness UI (Henry, 2026-07-03):** corner minimap; hold **M**
  for a full-screen map (the natural home for the click-to-pick spawn point
  deferred from M2); hold **Tab** for the traditional scoreboard — both team
  rosters with K/D and points (needs per-player kill/death counters next to
  the points ledger). Spawn-facility cycling moves off Tab (to Q/E) when the
  scoreboard lands.
- **M4 — win condition & breadth:** mothership HP/kill flow, match reset,
  fleet carrier + outfitter + remaining roster, map balance pass.

Every milestone re-checks the §4.2 guideposts; feel regressions are release
blockers, not polish debt.

## 11. Explicitly out of scope (for now)

Matchmaking/lobbies, cross-match persistence, art & sound beyond placeholder
SFX (gizmo rendering is fine), text/voice comms, spectating, anti-cheat beyond
server authority.
