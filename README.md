32 vs. 32 multiplayer action RTS+asteroids game built using bevy and lightyear.

## Vision

*Homage* is a 32v32 action game that chases the feeling of Savage: The Battle
for Newerth — two armies of real players colliding over a persistent strategic
map — but streamlined. Where Savage put one commander behind an RTS interface
issuing orders to everyone else, *Homage* has no commander at all. The strategy
layer is distributed among the players themselves, borrowing Homeworld's
central idea that ships build ships: a single mothership anchors each team and
constructs the carrier-type hulls — resource controllers and a few flavors of
true carrier — and those carriers in turn construct and field the smaller ships
that spawn from them. The tech tree emerges from what the team collectively
chooses to build, and every unit on the field is a human being flying it.

Everything, including the economy, is arcade-y and skill-based. Harvesting
resources is a flying challenge, not a click-and-wait, and hauling them home
matters: resources only count once deposited, and dying with an undeposited
load means losing it. The mothership is the default dropoff, but resource
controllers — and the biggest carriers — can receive deposits too, so pushing a
forward dropoff out toward contested fields shortens your team's haul routes
and paints a target on it at the same time. Resources buy hulls, with the
carrier-type hulls built at the mothership itself. Alongside resources, points
are awarded automatically for nearly anything that helps your team, and are
spent fitting out your ship. Both currencies persist through death, so a match
is a rhythm of sorties: fly out, fight or haul, die or return, and come back in
something better suited to what the battle has become.

The design's north star is that small and very large ships are both
interesting to fly, all the time. Homeworld's fleet doctrine is the model:
bombers that shred frigates, missile boats that swat fighters, corvettes in the
messy middle, megalaser frigates that exist to crack capital ships. No hull is
a stepping stone to be outgrown — a skilled pilot in a cheap interceptor is a
real threat to the enemy's economy and screens, while a carrier captain is a
mobile spawn point, forward dropoff, and rallying line whose positioning can
win or lose the map. Victory belongs to the team that self-organizes the better
fleet and flies it better.

## Development

The workspace has four crates:

- `shared` — the network protocol (replicated components, inputs), the avian2d
  physics setup, and the ship/bullet simulation, which runs identically on
  client and server so client-side prediction stays in sync.
- `server` — headless dedicated server (`cargo run -p homage_server`), with
  lag-compensated hit detection: targets are rewound to the interpolated state
  the shooter saw when validating hits.
- `client` — windowed client (`cargo run -p homage_client -- <client_id>`).
  Add `bot` as a second argument for a self-driving client.
- `e2e` — headless integration tests (`cargo test -p homage_e2e`): a real
  server and real clients in one process over loopback UDP, time-stepped
  manually so full gameplay scenarios verify in under a second.

### Running locally

```sh
cargo run -p homage_server
# in separate terminals, with unique client ids:
cargo run -p homage_client -- 1
cargo run -p homage_client -- 2
```

Controls: `W`/`↑` thrust, `S`/`↓` brake, `A`/`←` and `D`/`→` turn, `Space` or
`LMB` fire (Gunship hulls aim their turret with the mouse; Captain hulls take
WASD as screen-space nudges — see DESIGN.md §4.1). Hold `Backspace` ~1s to
self-destruct. Death opens the spawn system: click a facility on the map
(`L` skips to loadout), pick a hull on the loadout screen, `M`/`Esc` back to
the map, and hit SPAWN to deploy — there is no auto-respawn.

Dev cheats (manual testing; position cheats target the mouse cursor):
`F1` +50 ore · `F2` spawn asteroid · `F3` spawn ore fragments · `F4` spawn an
enemy target drone · `F5` teleport · `F6` heal.
