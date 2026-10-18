# battle-royale

Server-authoritative Battle Royale foundation built on the same reusable multiplayer and physics boundaries as `mmorpg`.

This first implementation slice is deliberately about authority, determinism and recovery rather than presentation. It provides a deterministic 100-player match simulation, lobby/drop/combat/finished phases, safe-zone shrink, storm elimination, player-scoped projections and a thin `game-server` adapter. Rendering, weapons, loot, matchmaking and deployment orchestration remain separate follow-up boundaries.

## Authority map

| Concern | Authority |
| --- | --- |
| Match phases, storm, spawn slots, elimination and visibility policy | `battle-royale-core` |
| Collision and movement integration | pinned `physics-engine` |
| Battle Royale wire encoding | `battle-royale-protocol` |
| Session ticks, command sequencing, reconnects, replay/recovery and process hosting | pinned `game-server` |
| Matchmaking/accounts/parties/rankings | external services, not match simulation |
| Input action/binding semantics and mobile overlay editing | pinned `input-bindings`; Battle Royale owns only consumer-specific action mapping |
| Rendering/settings/assets | existing reusable client foundations in later slices |

The repository composes those foundations; it should not grow substitutes for them.

## Implemented foundation

- deterministic integer-only match state with ordered player identity;
- bounded 100-player admission while the lobby is open;
- a deterministic lobby countdown and explicit Drop -> Combat -> Finished lifecycle;
- movement delegated to the shared `physics-engine`;
- exact monotonic game-command sequencing in core in addition to runtime fencing;
- deterministic safe-zone center and integer shrink schedule derived from match identity;
- terminal storm/session-expiry elimination and deterministic winner selection;
- canonical complete snapshots for replay/recovery;
- player-scoped snapshots with authoritative interest filtering;
- a thin `game-server::GameSimulation` adapter;
- deterministic mapping from Battle Royale match IDs to route-safe `game-server::MatchId` values;
- bounded multi-match host construction without duplicating host/session policy;
- a runnable WebTransport multi-match host with read-only health/readiness/status, signal-driven draining and optional recovery bundles;
- recovery tests that restore match state and reconnect identity through `game-server`;
- a `battle-royale-client` adapter that exposes Battle Royale semantic actions through the shared `input-bindings` model;
- deterministic mobile thumbstick quantization with a radial dead zone, diagonal hysteresis and redundant-command suppression;
- exact dependency pins, committed lockfile and fail-closed workspace validation.

## Mobile input boundary

Mobile mode reuses the shared `input-bindings` mobile-controls workbench rather than adding a Battle Royale-specific touch resolver. The shared layer owns the editable overlay presentation; `battle-royale-client` declares the consumer actions and translates normalized thumbstick samples into the existing `MatchCommand::SetMovement` contract.

The movement adapter uses integer-only quantization, a 42% radial dead zone and diagonal enter/exit hysteresis. It emits a network command only when the quantized movement direction changes, including one neutral command on release. This keeps pointer sampling rate and frontend render frequency out of authoritative match truth.

`battle-royale.look` and `battle-royale.menu` are declared as local client actions. Look/camera motion is not sent as gameplay truth until a future combat or aiming command defines an authoritative need for it. The starter overlay's action buttons likewise remain unbound to server gameplay until the combat model owns fire/reload semantics.

## Running the authoritative host

The host defaults to one match on UDP port 4433 and read-only operational status on port 8080. TLS material is explicit; gameplay mutation is not exposed through the status/control surface.

```sh
BATTLE_ROYALE_CERT_PEM=cert.pem \
BATTLE_ROYALE_KEY_PEM=key.pem \
BATTLE_ROYALE_MATCH_IDS=1,2,3 \
cargo run --locked -p battle-royale-game-server --bin battle-royale-host
```

Set `BATTLE_ROYALE_RECOVERY_DIR` to a complete hosted recovery bundle to start through the reusable `game-server` recovery path. Other knobs are `BATTLE_ROYALE_PORT`, `BATTLE_ROYALE_STATUS_PORT`, `BATTLE_ROYALE_RECONNECT_GRACE_TICKS`, `BATTLE_ROYALE_DRAIN_GRACE_MS` and `BATTLE_ROYALE_ROUTE_PREFIX`.

## Pinned foundations

- `game-server`: `769de47005cc37891011fc76ae183c18b7c5e0ae`
- `physics-engine`: `c796ea382bdcb0276b9309e8a3cca34c8c28313b`
- `input-bindings`: `7df9ca0122fa18d50f764ee4a2f1b100982d2be4`
- reusable validation workflow: `728fffa13c451766d08f06e6c7d7950a4de57b3d`
- Performance Evidence contract: `a1b21d34f04e5b3b2324f6c0300459e70380edd2`

The pins intentionally match the already-proven MMORPG integration surface for the first slice. Upgrades should be explicit compatibility work, not floating dependency drift.

## Deterministic scale evidence

Ordinary validation includes a full-capacity 100-player workload. It proves that player 101 fails closed, two independent 512-tick simulations produce byte-identical canonical snapshots, and full-capacity canonical/player projections remain within explicit 8 KiB/4 KiB protocol budgets. These are deterministic state/byte contracts; no wall-clock threshold is used in correctness CI.

## Performance evidence

The opt-in full-capacity benchmark records release-mode authoritative tick cost separately from deterministic projection bytes and emits canonical Performance Evidence 1.0.0 documents.

```sh
python3 benchmarks/run.py --output-dir performance-results
```

`benchmark` is exposed through `.coding-tooling.json` but is intentionally absent from the fast/full correctness tiers. Hosted performance evidence runs on `main` and by manual dispatch, validates its artifacts against an exact pinned `performance-evidence` revision, and preserves the receipts as workflow artifacts. Wall-clock measurements are advisory rather than a correctness threshold. See [the benchmark contract](benchmarks/README.md).

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

See [the architecture](docs/ARCHITECTURE.md) and [roadmap](ROADMAP.md).
