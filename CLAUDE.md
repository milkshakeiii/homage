# CLAUDE.md

32v32 multiplayer action RTS (bevy 0.19 + lightyear 0.28 + avian2d 0.7).
Read `README.md` for the game vision and `DESIGN.md` for concrete design
decisions before implementing gameplay — DESIGN.md is the design ground truth,
and its PROPOSED/APPROVED/OPEN statuses tell you what is settled vs. what
needs Henry's input.

## Workspace

- `shared` — network protocol (`protocol.rs`), shared simulation (`sim.rs`),
  physics setup (`lib.rs`). Everything here runs identically on client and
  server; determinism is required for prediction rollbacks to work.
- `shared/src/hulls.rs` — per-hull stat tables, hull classes (who builds
  what where), control archetypes. `shared/src/fittings.rs` — the fitting
  catalog: costs, facility stocking, weapon profiles, hull-mod effects.
- `server` — headless dedicated server (`server/src/lib.rs` + thin main).
  Hand-rolled Valve-style lag compensation (see comments there for why not
  lightyear_avian's plugin). Authoritative state that must survive death
  lives in resources keyed by PeerId (Banks, PointsStore, Unlocks, KdStore,
  DockedStates), mirrored onto ship components for replication.
- `client` — windowed client, gizmo-only rendering (`client/src/lib.rs`,
  spawn/loadout/map/scoreboard UI in `client/src/spawn_screen.rs`, feel
  effects in `client/src/juice.rs`). `cargo run -p homage_client -- <id>`
  (unique id per client), append `bot` for a self-driving client.
- `e2e` — the headless integration-test harness (`TestNet`): one real server
  App plus N real client Apps in one process over loopback UDP, time-stepped
  manually (one tick per `update()`), so a connect→shoot→kill→respawn cycle
  runs in well under a second.

## Build / run / test

```sh
cargo build --workspace          # first build is slow (bevy); later builds are quick
cargo run -p homage_server
cargo run -p homage_client -- 1  # separate terminal, unique id per client
cargo test --workspace           # includes headless client+server integration tests
```

Integration tests live in `e2e/tests/` on top of the `TestNet` harness in
`e2e/src/lib.rs`; each test uses a unique loopback port (tests run in
parallel). Every gameplay feature should land with an end-to-end test there
("harvester deposits → bank increments"), not just unit tests. Set
`TEST_LOG=1` for server logs while debugging a test.

Gotcha: `cargo run -p <crate>` unifies features per-package, not
per-workspace, so it may rebuild bevy after a `--workspace` build. Prefer
`cargo build --workspace` first, and expect the first `cargo run` after
adding a crate to be slow.

## Conventions & gotchas

- Simulation code goes in `shared/src/sim.rs` and must be deterministic:
  manual velocity integration, constants (not wall-clock), tick arithmetic via
  lightyear `Tick`. Anything server-only (damage, economy authority) stays in
  the server crate.
- Server is authoritative for damage/health/economy; those components
  replicate but are never predicted. Movement/firing are predicted.
- Don't filter shared systems on `Without<Interpolated>` — lightyear 0.28
  leaves `Interpolated` on server-side entities (see note in `sim.rs`).
- Physics components (RigidBody, Collider, damping) are NOT replicated. Any
  predicted entity the client must simulate needs them inserted client-side
  (see `add_predicted_ship_physics` in `client/src/lib.rs`); without a
  RigidBody avian silently ignores the entity and "prediction" degrades to
  snapping to server packets at the send rate. Symptom: velocity changes but
  position doesn't.
- The client verifies smooth rendering empirically: run with
  `HOMAGE_MOTION_DEBUG=1` and check the fraction of zero-delta frames.
- Bump `PROTOCOL_VERSION` in `shared/src/lib.rs` whenever the wire format
  changes (components/messages/channels/inputs in `protocol.rs`). Mismatched
  builds then refuse to connect instead of silently dropping unknown
  messages — restart BOTH server and client binaries after pulling.
- Drain `MessageReceiver<M>` in `Update`, NEVER `FixedUpdate`: lightyear
  clears receiver buffers every render frame (`Last`), and FixedUpdate skips
  frames — messages get dropped probabilistically (~75% at the server's
  256Hz loop / 64Hz tick). The e2e harness CANNOT catch this class of bug
  (manual time stepping runs exactly one tick per update); message-flow
  features must also be verified against real binaries (HOMAGE_AUTO_SCUTTLE
  exists for this).
- Ship-mirrored components (Bank, Points, UnlockedFittings) die with the
  ship, and the death/dock screens are exactly where the player acts on that
  state. Anything those screens need must ALSO reach the client as a
  server→client message consumed into a resource in Update (WealthUpdate →
  WealthCache, DockedNotice → DockedAt, MatchResult → LastMatchResult) —
  in BOTH windowed and headless modes, so e2e tests can assert on the
  resource (raw MessageReceiver buffers are cleared every frame and cannot
  be drained from test code after `tick()`).
- Per-player stats that must outlive ships AND be visible to everyone
  (scoreboard) live on replicated roster entities (RosterEntry — deliberately
  not PlayerId, so ship systems never match them).
- Player-visible UI strings must be ASCII: bevy's default font has no glyphs
  for em-dashes/middle dots/ellipses (they render as boxes).
- Island sleeping is disabled in avian (incompatible with rollbacks); the
  IslandPlugin itself must stay (see `shared/src/lib.rs`).
- `app.set_error_handler(bevy::ecs::error::warn)` on the server: ECS command
  failures must not kill a dedicated server.

## Where things stand

DESIGN.md §10 tracks milestone status (M0–M3.5 done; M4 mostly done) and
§12 is the living upcoming-work list — read it before picking new work.
Dev cheats (F1–F7) are always-on for development and MUST be gated or
stripped before any public build.

## Process (agreed with Henry)

- Autonomous work happens directly on `main` (Henry, 2026-07-03), committed
  and pushed in small, frequent commits.
- Rendering stays gizmo-based placeholder art for now.
- Tuning numbers in DESIGN.md are placeholders; put actual constants in
  `sim.rs` and expect them to change.
