# Composed match protocol

Battle Royale protocol **2**, canonical schema **2**, ruleset revision **3** replace the
standalone integer player simulator. Version 1 commands and snapshots fail closed.
Ruleset 3 (#33) gives the storm a 60 s delay and a 120 s close, so it reaches no spawn
slot in the first two minutes of combat. The embedded MMORPG codecs are the pinned
revision's, identified by its `mmorpg-protocol::PROTOCOL_ID`; this repository does not
fork or extend them. `game-server` still supplies player
identity, connection epochs, command sequences and its session framing.

## Commands

A little-endian u16 version and u8 kind precede one payload:

| Kind | Payload |
| --- | --- |
| 1 | Two signed bytes, world X/Z movement components in -1..=1; adapted to shared movement |
| 2 | Class u8, sex u8, six little-endian u16 equipped item IDs (zero is an empty slot) |
| 3 | A complete `mmorpg-protocol` command: plain, observed, or wrapped by `encode_acknowledged_command` with the sheet revisions the client holds |

The host records the sheet revisions of each applied kind 3 command and packs that
player's projection with `pack_snapshot_acknowledged`, as MMORPG's zone host does
(mmorpg #544): a bag, quest, loot or ability sheet travels only while the client lacks its
current revision. The record is projection-only; recovery rebuilds it by replay or
starts empty, which resends every sheet. An observed command's tick is not used.

Kind 2 validates against the shared class/equipment catalogs and is accepted only in
the waiting lobby, before the active life. Hosts may instead supply the chosen build
to `add_player_with_build` before admission. The transport's default guest is a
maximum-level Warden with the shared standard build. Lobby reconfiguration prepares a
fresh ephemeral character before committing it; no durable character API is used.

The mode accepts shared movement/jump, target selection and social commands; attacks,
guard, directed strikes and abilities require Combat. Inventory, class-choice,
progression and spirit-release commands are refused. Selection closes at Drop; an
eliminated life cannot be edited, revived or reused in the same match. Another match
provides the next life and another build selection.

## Snapshots

`BRCA` identifies canonical authority; `BRPL` identifies a player projection. They have
separate decoders and cannot be substituted. Common mode fields retain little-endian
encoding: versions, match ID, authoritative tick, phase clocks, storm and winner.
Ordered roster records contain only identity, sequencing (canonical only) and terminal
elimination metadata. They contain no copied player health, position or velocity.

A u32 byte length then precedes a complete payload from the pinned MMORPG codec:
canonical zone state in BRCA, the existing player-scoped zone projection in BRPL.
The embedded bytes retain their own MMORPG encoding and content identity. Clients
pass the decoded `MatchSnapshot.zone` to existing MMORPG presentation code; they do
not reconstruct gameplay from mode metadata. `acknowledged_sequence` covers every
accepted match command, including lobby setup; the shared zone's own sequence can
therefore lag it until the next character command.

Canonical restore validates mode versions, roster/zone membership, character life,
maximum level, phase clocks, storm derivation and winner, then delegates the complete
zone checkpoint to `ZoneSimulation::from_snapshot` with identical authored content.
`game-server` recovery also replays the composed adapter and fences reconnected
sessions. Recovery reconnects both formerly connected and disconnected sessions;
transport loss itself does not eliminate a character.

## Evidence and limits

`composed_match` covers a Firebolt cast/hit, duplicate refusal, mid-cast checkpoint,
recovery/reconnect, exact continuation, elimination and winner. `hosted_pvp` is an
explicit real UDP/TLS WebTransport check with two independent clients against the
normal match host. It uses a temporary OpenSSL certificate and the actual welcome,
command and snapshot contracts; it creates no alternative transport or renderer.

The 100-character headless workload is correctness evidence, not a claim that 100
rendered or networked combatants are ready. The MMORPG browser/native mode entrance
and presentation integration remain MMORPG #341.
