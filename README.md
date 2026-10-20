# battle-royale

Battle Royale composes one authoritative `mmorpg-core::ZoneSimulation` per match.
Players use the MMORPG's maximum-level classes, legal equipment, real attacks,
guards, spells, projectiles, health and Rapier movement. The mode owns lobby setup,
phases, storm, elimination and winner; `game-server` owns hosting and sessions.

The headless scenario and real two-client WebTransport path work. The rendered
browser/native mode entrance remains [MMORPG #341](https://github.com/moritzbrantner/mmorpg/issues/341).
There is no separate Battle Royale renderer, inventory or combat engine.

## Character setup and play

Hosts can pass a validated `EphemeralCharacterBuild` to `add_player_with_build`.
The network host admits a standard maximum-level Warden guest; a client can choose
any supported class, appearance and legal equipped catalog items in the waiting
lobby with `MatchCommand::SelectBuild`. Setup is frozen at Drop, and every admitted
life is single-elimination. A subsequent match offers another selection. Match
outcomes never export or overwrite durable MMORPG characters.

Use `MatchCommand::Character(ZoneCommand)` for the actual MMORPG movement, target,
attack, guard, directed-strike and ability intents. Combat requires the Combat
phase. Inventory/class mutations and spirit release are refused, so the shared
MMORPG's PvE resurrection command cannot revive a match elimination. The existing
mobile semantic movement adapter maps its world X/Z intent into shared movement.

The mode keeps its existing deterministic Waiting -> Drop -> Combat -> Finished
shape and integer safe-zone schedule. It asks the shared zone to apply storm damage,
observes actual combat deaths, and freezes gameplay after the winner is decided.
Exact placeholder timing, balance, loot, teams, rewards, drop variants and further
PvP modes remain deferred product choices.

## Ownership

| Concern | Authority |
| --- | --- |
| Character state, equipment, movement, health, combat, abilities and Rapier physics | pinned `mmorpg-core` |
| Lobby build selection, arena content, phases, storm, terminal elimination and winner | `battle-royale-core` |
| Mode envelope around existing MMORPG command/canonical/player codecs | `battle-royale-protocol` |
| Sessions, sequences, ticks, disconnect/reconnect, replay, recovery and multi-match WebTransport hosting | pinned `game-server` |
| Device/binding semantics and reusable mobile overlay editor | pinned `input-bindings` |
| Models, animation, effects, assets and renderer | existing MMORPG presentation owners; client entry pending #341 |
| Accounts, matchmaking, parties, rankings and durable rewards | external services outside the match hot loop |

Canonical recovery embeds the whole shared zone plus mode state. Player snapshots
embed its ordinary interest-filtered projection plus public mode metadata. Health
and physical state are never mirrored in a second mode simulation. See
[architecture](docs/ARCHITECTURE.md), [protocol](docs/PROTOCOL.md),
[Decision Atlas](docs/decisions/atlas.md) and [roadmap](ROADMAP.md).

## Run the authoritative host

```sh
BATTLE_ROYALE_CERT_PEM=cert.pem \
BATTLE_ROYALE_KEY_PEM=key.pem \
BATTLE_ROYALE_MATCH_IDS=1,2,3 \
cargo run --locked -p battle-royale-game-server --bin battle-royale-host
```

The host defaults to UDP 4433 and read-only operational status on 8080. Route
`/game/matches/br-0000000000000001` selects match 1. Existing `game-server` welcome,
command, reconnect and snapshot framing surrounds the mode bytes documented in
[the client contract](docs/PROTOCOL.md).

`BATTLE_ROYALE_RECOVERY_DIR` selects a complete hosted recovery bundle. Other knobs:
`BATTLE_ROYALE_PORT`, `BATTLE_ROYALE_STATUS_PORT`, `BATTLE_ROYALE_RECONNECT_GRACE_TICKS`,
`BATTLE_ROYALE_DRAIN_GRACE_MS` and `BATTLE_ROYALE_ROUTE_PREFIX`. Status cannot mutate
gameplay. Brief disconnects do not eliminate; session retirement does.

## Evidence

```sh
cargo test -p battle-royale-game-server --locked --test composed_match
cargo test -p battle-royale-game-server --locked --test hosted_pvp -- --ignored
```

The first exercises real PvP hit, mid-cast canonical continuation, duplicate commands,
host recovery, both players reconnecting, combat elimination and winner. The second
uses real TLS/UDP WebTransport clients against the normal host and requires OpenSSL;
it is explicit network acceptance, excluded from ordinary deterministic tests.

The ordinary 100-character workload checks capacity, bounded canonical/player bytes
and independent deterministic movement/zone continuation. It no longer runs a second
integer player simulator. This is headless correctness evidence: it does **not** prove
100 connected or fully animated combatants. The existing opt-in benchmark now measures
the composed character workload; no new performance threshold or release FPS claim is
introduced. See [benchmark scope](benchmarks/README.md).

## Mobile input boundary

`battle-royale-client` supplies semantic move/look/menu actions through shared
`input-bindings`. Its integer thumbstick quantizer uses a 42% radial dead zone,
diagonal hysteresis and duplicate suppression. Look/menu remain local; actual combat
uses the shared character commands. The MMORPG mode entrance will reuse the existing
mobile overlay and rendering rather than creating a competing UI stack.

## Exact foundations

- `mmorpg-core` / `mmorpg-protocol`: `81306faec8a6a7ea3b4279d75f763c1135ca4a19`
- `game-server`: `769de47005cc37891011fc76ae183c18b7c5e0ae`
- `input-bindings`: `9872c63b18b52842aba206b0ecef7a70dc8f1249`
- reusable validation workflow: `728fffa13c451766d08f06e6c7d7950a4de57b3d`
- Performance Evidence contract: `a1b21d34f04e5b3b2324f6c0300459e70380edd2`

The former direct `physics-engine` dependency is removed. Pins are intentional;
combat and temporary-build consumer contract tests use the same runtime MMORPG pin.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --all-features --locked
```

Convention source revision: `6cc05789dab39cee6758b288f83fb48b4ee6dbdb`.
