# Agent guidance

## Approved direction

Read [the Battle Royale Decision Atlas](docs/decisions/atlas.md). This repository is the **first PvP mode of the MMORPG**, not an independent shooter. The implementation uses one `mmorpg-core::ZoneSimulation` per match for actual max-level MMORPG characters, PvP combat, health and Rapier physics; `battle-royale-core` supplies match phases, storm, elimination and victory only. Match-specific loadouts are selected before spawn or after elimination/between lives, never mid-life. One mode first; defer other modes and detailed rules. Do not reintroduce a standalone player/physics implementation. The rendered MMORPG mode entrance remains #341. See [battle-royale #13](https://github.com/moritzbrantner/battle-royale/issues/13) and [mmorpg #339](https://github.com/moritzbrantner/mmorpg/issues/339).

## Authority

- `battle-royale-core` owns deterministic match rules: lifecycle phases, storm/safe-zone state, elimination semantics, arena content and mode information visibility. The shared zone owns character intent, physical spawn allocation and character interest.
- `battle-royale-client` owns Battle Royale-specific client adaptation only: semantic action declarations and normalized client input -> authoritative command mapping. It must not become a second source of match truth or fork reusable device/input semantics.
- The composed MMORPG zone owns health, combat, equipment and Rapier movement. Battle Royale must not mirror character state or implement a second physics/combat engine.
- `game-server` owns session identity, sequencing, tick scheduling, reconnects, replay/recovery, WebTransport and multi-match process hosting. Keep Battle Royale adapters thin.
- Matchmaking, accounts, parties, rankings and fleet placement remain outside match simulation authority. Integrate existing services at explicit boundaries instead of importing their state into the hot loop.

## Match invariants

- A match has exactly one authoritative simulation writer.
- Player commands are monotonically sequenced and stale or duplicate sequences fail closed.
- Temporary transport disconnects do not eliminate a player; only session retirement invokes game-level removal.
- Once the lobby closes, new player admission fails closed.
- The storm schedule is integer-only and deterministic from match identity plus authoritative tick.
- Elimination is terminal for the match. Finished matches cannot resume gameplay mutation.
- Canonical snapshots contain complete replay/recovery truth. Player-scoped snapshots are projections and must never become recovery authority.

## Determinism and performance

- Stable IDs and ordered collections are required in authoritative state.
- No wall-clock time, ambient randomness, unordered iteration or frontend state may influence match truth.
- Capacity changes require deterministic workload evidence; correctness tests must not depend on timing thresholds.
- Keep durable persistence and external service calls outside the simulation tick.

## Dependencies

- External reusable foundations are pinned to exact revisions.
- Update pins intentionally with compatibility evidence.
- `input-bindings` owns device/binding semantics and the reusable mobile overlay editor; Battle Royale may only supply consumer actions and adapters.
- Do not fork `game-server`, `physics-engine`, input, settings, rendering or asset foundations into this repository.

## Validation

Run formatting, Clippy, tests and build for the complete workspace with the committed lockfile. Do not weaken failures to make a slice green.
