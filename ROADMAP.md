# Battle Royale roadmap

The checked items below distinguish the composed authoritative match and real two-client transport from the still-pending rendered MMORPG mode entrance. The agreed first-mode direction is tracked in the [Decision Atlas](docs/decisions/atlas.md); only Battle Royale is in immediate scope.

## Foundation — authoritative match kernel

- [x] Establish Rust workspace, exact dependency pins and fail-closed validation.
- [x] Define deterministic match identity, player capacity and ordered state.
- [x] Replace the historical direct `physics-engine` integration with one MMORPG zone and its Rapier movement.
- [x] Define Waiting -> Drop -> Combat -> Finished lifecycle.
- [x] Add deterministic safe-zone shrink and storm elimination.
- [x] Keep canonical recovery snapshots separate from player-scoped projections.
- [x] Adapt the match kernel to `game-server::GameSimulation`.
- [x] Prove replay/recovery and reconnect continuity through `game-server`.
- [x] Prove multiple matches can be hosted without sharing gameplay authority.

## Runnable authoritative host

- [x] Add a Battle Royale host binary around `game-server::MatchHost`.
- [x] Expose health/readiness/status through the reusable host status contract.
- [x] Add graceful hosted recovery for a complete configured match set.
- [x] Keep operational status read-only and separate from gameplay mutation.
- [x] Prove two real WebTransport clients fight and receive the same match winner.
- [ ] Prove two simultaneous hosted matches remain isolated over real WebTransport.

## First playable MMORPG PvP mode

- [x] Reuse authoritative opt-in MMORPG player-combat rules, attacks, guards, projectiles and class abilities ([mmorpg #339](https://github.com/moritzbrantner/mmorpg/issues/339)).
- [x] Admit max-level MMORPG character builds with legal level-appropriate gear, without modifying durable MMO progression ([mmorpg #340](https://github.com/moritzbrantner/mmorpg/issues/340)).
- [x] Replace duplicate player health, movement and physics with one authoritative `mmorpg-core::ZoneSimulation` per match; retain only match-specific storm, phase, elimination and winner logic ([#13](https://github.com/moritzbrantner/battle-royale/issues/13)).
- [x] Preserve deterministic command ordering and complete match + zone recovery through the existing `game-server` host ([#13](https://github.com/moritzbrantner/battle-royale/issues/13)).
- [ ] Play as full-size MMORPG characters with a friend in one real hosted match using the MMORPG's presentation; change class/gear only between lives ([mmorpg #341](https://github.com/moritzbrantner/mmorpg/issues/341)).
- [ ] Revalidate scale/performance on real MMORPG character combat and animation before claiming large-player readiness.

Extra PvP modes and precise rules for loot, teams, respawn, prizes, balance and the drop phase are **not yet selected**. Existing match defaults are implementation scaffolding; do not build gun/fire/reload mechanics or a second inventory/combat system.

## Networking and scale

- [x] Bound canonical and player-scoped snapshot sizes at the full 100-player capacity.
- [ ] Add spatial indexing only when deterministic evidence shows the bounded O(n) projection is insufficient.
- [ ] Add snapshot delta/compression only behind versioned protocol evidence.
- [ ] Keep one match single-writer; scale by hosting more matches, not by splitting one physics step across machines.
- [ ] If a future mega-map requires intra-match partitioning, design explicit authority transfer rather than shared mutable simulation.

## Matchmaking and services

- [ ] Integrate match creation/placement with existing multiplayer setup/service foundations.
- [ ] Keep account, party, ranking and entitlement state outside the match hot loop.
- [ ] Fence delayed external side effects with match/session identity.
- [ ] Add durable match result publication as an idempotent post-match command.

## Client foundations

- [ ] Reuse the MMORPG's existing Three.js/browser and wgpu/native character presentation, animation and assets, without introducing a Battle Royale-specific renderer.
- [x] Reuse the shared `input-bindings` action model for Battle Royale client actions.
- [x] Add deterministic mobile thumbstick adaptation onto the existing authoritative movement command.
- [x] Keep look/menu mobile interactions client-local until game rules require an authoritative command.
- [ ] Compose the shared mobile overlay editor into the first rendered Battle Royale client once that client surface exists.
- [ ] Reuse `settings` and `asset-tooling`.
- [ ] Add interpolation over authoritative player-scoped snapshots.
- [ ] Add optional shared-physics prediction/reconciliation only after acknowledgement contracts are explicit.
- [ ] Keep browser/Pages demos non-authoritative.

## Evidence

- [x] Revalidate the deterministic 100-character capacity/movement workload on the composed MMORPG zone (headless correctness, not rendered/network scale).
- [x] Add performance receipts for tick cost and projection bytes separately from correctness.
- [ ] Add packet impairment/reconnect acceptance using the existing `game-server` transport harness patterns.
- [ ] Add long-run replay equivalence and recovery corruption tests.
