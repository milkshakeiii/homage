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
- `server` — headless dedicated server. Hand-rolled Valve-style lag
  compensation (see comments in `server/src/main.rs` for why not
  lightyear_avian's plugin).
- `client` — windowed client, gizmo-only rendering. `cargo run -p
  homage_client -- <id>` (unique id per client), append `bot` for a
  self-driving client.
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
- Island sleeping is disabled in avian (incompatible with rollbacks); the
  IslandPlugin itself must stay (see `shared/src/lib.rs`).
- `app.set_error_handler(bevy::ecs::error::warn)` on the server: ECS command
  failures must not kill a dedicated server.

## Process (agreed with Henry)

- Autonomous work happens on the `claude/autonomous-dev` branch, committed in
  small, frequent commits; merges to `main` go through Henry.
- Rendering stays gizmo-based placeholder art for now.
- Tuning numbers in DESIGN.md are placeholders; put actual constants in
  `sim.rs` and expect them to change.
