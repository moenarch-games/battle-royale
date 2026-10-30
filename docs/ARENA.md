# Fortress arena

Battle Royale has one arena: a symmetric multilevel fantasy fortress ([#31](https://github.com/moenarch-games/battle-royale/issues/31)). This milestone is **graybox**: exact authoritative collision with plain boxes, wedges and lifts and no authored art. Visual art comes later through the existing Blender → asset-tooling → MMORPG renderer path and must keep this collision.

## Single definition

`battle_royale_core::arena` is the only definition of the arena. It builds one shared `mmorpg_core::ZoneContent`; the match's `ZoneSimulation` and its Rapier physics own every collision, slope and lift ride. Nothing else (client, renderer, host) may carry its own copy of the geometry.

| API | Meaning |
| --- | --- |
| `arena::content()` | The validated `Arc<ZoneContent>`: static boxes (`definition().colliders()`), walkable wedges (`ramps()`), lifts (`platforms()`), spawn lattice (`definition().spawn_grid()`) and the free-for-all, ephemeral-character policies. |
| `content.revision()` | The content identity: `mmorpg-core`'s hash of every table. Snapshots and recovery carry it and refuse any other content. A presentation built from the same content has the same revision. |
| `arena::parts()` | `(body id, ArenaPart)` for every collider, ramp and lift body, for choosing materials: `Ground`, `Wall`, `Parapet`, `Tower`, `Floor`, `Bridge`, `Column`, `Ramp`, `Lift`. |
| `arena::spawn_grid()` and constants | The spawn lattice, module size, wall-walk and tower heights and lift timing. |

Units are `mmorpg-core` units: 100 per metre, y up, the ground's top at y = 0. A collider is a box (`position` centre, `half_extents`). A ramp is a right wedge filling the lower half of its box, rising toward `rises`. A lift is a box moving between `low` and `high` centres; its centre at any zone tick is `Platform::centre_at(tick)`, so a renderer derives lift motion from the projection's tick alone, exactly as the zone does.

## How the MMORPG renderer consumes it

The browser and native clients ([MMORPG #552](https://github.com/moenarch-games/mmorpg/issues/552)) depend on `battle-royale-core` at a pinned revision and read `arena::content()` and `arena::parts()` to build their scene and local practice zone. Because `battle-royale-core` itself pins `mmorpg-core` by git revision, the MMORPG workspace patches that dependency to its own workspace crate:

```toml
[dependencies]
battle-royale-core = { git = "https://github.com/moenarch-games/battle-royale.git", rev = "<pinned>" }

[patch."https://github.com/moritzbrantner/mmorpg.git"]
mmorpg-core = { path = "crates/mmorpg-core" }
```

The renderer checks that its content's `revision()` equals the revision a hosted match projects; a mismatch means the client and host disagree about the arena and must not play together.

## Layout

All dimensions on an 8 m module. The south side is authored once; the other three are its quarter turns about the centre, so the arena is identical from every gate.

- **Spawn lattice**: 23 × 23 slots 8 m apart covering ±88 m; the shared zone fills them in row order from the south-west. Every slot is on open ground, at least 1.8 m below any ceiling, and clear of walls, ramps and every lift's swept space: walls stand on the module lines halfway between slots.
- **Curtain wall** (±84 m): 8 m high, 4 m wall walk, 1 m outer parapet, a 6 m gate in the middle of each side under a lintel at wall-walk height. Two ramps per side lead from the courtyard to the wall walk.
- **Corner towers**: four solid 7.2 m towers, 16 m high. Each is reached from both adjacent wall walks by a ramp, so the wall walk is one continuous ring over the tower tops.
- **Courtyard** between the wall and the keep.
- **Keep** (24 m square, roof at 8 m): ground floor ring corridor around a central hall; one door per side in the outer wall and in the hall wall; one interior ramp per side up through a roof opening. A 11.2 m central tower (16 m high) stands on the roof. A bridge per side links the roof to the gate lintel.
- **Lifts** (traversal only, no hazards): one per side from the courtyard to the keep roof and one per side from the roof to the central tower top. A lift rests 3 s at each stop and moves at 1.5 m/s. Each lift pillar fills its own shaft below its top (sunk into the ground, or into a solid column inside the keep), so it never sweeps space a character can stand in.

Elevations: ground 0 m, wall walk / keep roof / bridges 8 m, tower tops 16 m. All ramps are 34°, under the character controller's 45° limit.

## Evidence

`crates/battle-royale-core/tests/fortress_arena.rs` validates the collision layout, its quarter-turn symmetry, the stable content identity, spawn clearance against ramps and lifts, and walks a real shared character along the intended routes: gate → courtyard → ramp → wall walk → corner tower → next wall walk; keep door → interior ramp → roof → lift → central tower; courtyard lift → roof → bridge → gate lintel (boarding, riding and leaving both kinds of lift).

## Limitations

- Graybox only: no textures, props or authored meshes yet.
- The shared zone allocates spawn slots in row order, so a 100-player match fills the southern rows first; spawn distribution is a later match-rule decision, not an arena change.
- Integrated manual movement review in the MMORPG renderer waits for [MMORPG #552](https://github.com/moenarch-games/mmorpg/issues/552).
