# Architecture

## Scope

A Battle Royale match is a bounded, single-writer authoritative simulation. Horizontal scale comes from hosting more independent matches through `game-server::MatchHost`; the first design does not distribute one match's physics step across machines.

```text
client input
     |
     v
battle-royale-client
     |
     v
authoritative commands
     |
     v
 game-server MatchHost <---- matchmaking / placement
   |          |
 match A    match B
   |          |
 BattleRoyaleGameServerAdapter
        |
        v
 battle-royale-core
        |
        v
  mmorpg-core::ZoneSimulation (Rapier)
```

## Authority boundaries

### battle-royale-client

Owns consumer-specific client adaptation, not input-device semantics or match truth:

- declares Battle Royale semantic actions through the pinned `input-bindings` model;
- converts normalized mobile thumbstick samples into the existing discrete movement intent;
- suppresses duplicate movement commands when high-frequency pointer samples quantize to the same intent;
- treats look/camera and menu interaction as client-local until a game rule requires a corresponding authoritative command.

The reusable `input-bindings` repository owns the mobile overlay editor and binding semantics. Battle Royale must not fork that UI or introduce a second touch resolver.

### battle-royale-core

Owns only game-specific match truth:

- phase progression and lobby closure;
- lobby build selection and admission;
- mode arena content;
- safe-zone/storm schedule;
- terminal match elimination and winner state;
- which match state is visible to a player.

It does not own transport connection epochs, reconnect capabilities, TLS, host draining, replay storage, matchmaking or rendering.

### game-server

Owns reusable runtime authority:

- player/session identity and connection epochs;
- command sequence admission before game-specific decode;
- authoritative tick scheduling;
- player-scoped snapshot delivery;
- replay and graceful recovery;
- multi-match process hosting and operational lifecycle.

The adapter converts bytes and delegates; it does not create a second scheduler or session registry.

### mmorpg-core

One pinned `ZoneSimulation` per match owns real maximum-level characters, equipment,
abilities, health, attacks, guards, projectiles, status and Rapier movement. Battle
Royale stores no second health or physical state. Its fortress arena
([ARENA.md](ARENA.md)) is shared zone content (boxes, ramps and lifts) and explicitly opts into
ephemeral characters and free-for-all eligibility; the shared content fingerprint
binds those choices, geometry and the ability catalog through recovery.

The mode evaluates its existing integer storm schedule against the shared rounded
physical positions and asks the zone to apply direct environmental damage. The zone
owns health changes and normal death cleanup, with no fabricated combat source. The
mode observes shared combat deaths before applying that tick's storm and records a
terminal match elimination. It refuses spirit release and all in-life equipment/class
changes. The zone stays frozen when the match finishes.

## Deterministic lifecycle

`Waiting` admits players until the deterministic lobby countdown expires. The match then closes admission and enters `Drop`, followed by `Combat`. During combat the safe-zone radius is computed from integer constants and authoritative tick only. No system clock or random number source participates.

The safe-zone center is deterministically derived from `BattleMatchId`, making a fresh replay simulation reconstruct the same world without hidden ambient seed state.

`Finished` is terminal. Runtime ticks may continue for transport/replay bookkeeping, but gameplay movement and damage no longer mutate.

## Disconnect semantics

Temporary network disconnect is a `game-server` concern and does not immediately change Battle Royale truth. The game simulation sees `remove_player` only when the reusable session runtime retires the slot. Before the match starts that removes the lobby participant; after the match starts it is a terminal `SessionExpired` elimination.

This prevents brief transport loss from becoming a second, accidental game-rule authority.

## Snapshot boundary

Canonical snapshots contain the complete shared zone plus ordered mode roster, phase, elimination and sequence state. There is one source of character truth. They are used for replay/recovery hashing and can be restored through `BattleRoyaleMatch::from_snapshot`.

Player-scoped snapshots contain only the addressed player's allowed projection. The shared zone owns physical interest filtering and always includes the addressed player; the mode supplies its public match roster and outcome separately. Future stealth, inventory or spectator policy belongs in this projection boundary, never in transport code.

## Capacity

The initial 100-player cap is a correctness bound, not a throughput claim. Raising it requires deterministic workload evidence for physics, tick work and per-player projection size. The mode roster is bounded at 100 entries. Physical interest queries use the shared zone's existing index; do not add a competing Battle Royale spatial index. Headless capacity checks do not establish 100-client transport or rendering readiness.

## Persistence and external services

Durable match results, accounts, parties, rankings and matchmaking use command/query boundaries outside the hot loop. External service retries must be idempotent and fenced with match/session identity. Event sourcing is not required: `game-server` replay/recovery already provides deterministic runtime evidence, while durable business state can use simpler persistence contracts.

See [the composed protocol](PROTOCOL.md) for the version boundary and client contract.
