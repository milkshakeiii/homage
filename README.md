32 vs. 32 multiplayer action RTS+asteroids game built using bevy and lightyear.

## Development

The workspace has three crates:

- `shared` — the network protocol (replicated components, inputs), the avian2d
  physics setup, and the ship/bullet simulation, which runs identically on
  client and server so client-side prediction stays in sync.
- `server` — headless dedicated server (`cargo run -p homage_server`), with
  lag-compensated hit detection: targets are rewound to the interpolated state
  the shooter saw when validating hits.
- `client` — windowed client (`cargo run -p homage_client -- <client_id>`).
  Add `bot` as a second argument for a self-driving client.

### Running locally

```sh
cargo run -p homage_server
# in separate terminals, with unique client ids:
cargo run -p homage_client -- 1
cargo run -p homage_client -- 2
```

Controls: `W`/`↑` thrust, `A`/`←` and `D`/`→` turn, `Space` fire.
