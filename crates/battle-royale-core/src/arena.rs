//! The mode's one arena: a symmetric multilevel fortress (graybox, #31).
//!
//! This module is the single definition of the arena's physical truth. Every body is a
//! shared `mmorpg-core` static box, ramp or moving platform inside the shared
//! [`ZoneContent`], so the zone's Rapier physics owns all traversal and all character
//! rules belong to the shared zone. Presentation consumers read the same content through
//! [`content`] and [`parts`] (see `docs/ARENA.md`) instead of copying the geometry.
//!
//! Layout, in units (100 per metre, y up, ground top at y = 0), on an 8 m module:
//!
//! - Spawn slots form a 23 × 23 lattice 8 m apart covering ±88 m, so all 512 shared slots
//!   stand on open ground at a lattice point. Everything that reaches below 1.8 m keeps
//!   clear of those points: walls stand on the module lines halfway between them.
//! - A square curtain wall (±84 m, 8 m high, 4 m wall walk with an outer parapet) with one
//!   gate in the middle of each side, crossed by a lintel at wall-walk height.
//! - Four solid corner towers (16 m), each reached from both adjacent wall walks by a ramp.
//! - Two ramps per side from the courtyard up to the wall walk.
//! - A central keep (24 m square): a ring corridor around a central hall on the ground
//!   floor, one door per side, an interior ramp per side up to the roof (8 m), a central
//!   tower on the roof (16 m) and a bridge per side from the roof to the gate lintel.
//! - Eight traversal-only lifts: one per side from the courtyard up to the keep roof and one
//!   per side from the keep roof up to the central tower. Each lift is a pillar that fills
//!   its own shaft below its top, so it never sweeps space a character can stand in.
//!
//! The whole layout is four copies of one side rotated by quarter turns about the centre.
use mmorpg_core::{
    CharacterPersistence, Platform, PlatformId, PlayerCombatPolicy, Ramp, RampRise, SpawnGrid,
    StaticCollider, ZoneContent, ZoneDefinition, ZoneError,
};
use std::sync::{Arc, OnceLock};

/// Module size: spawn slots sit at its multiples, walls on the lines halfway between.
pub const MODULE_UNITS: i32 = 800;
/// Spawn slots per lattice row; 23 rows of 23 hold all 512 shared slots.
pub const SPAWN_COLUMNS: u16 = 23;
/// Top of the curtain wall walk, the keep roof and the bridges.
pub const WALL_WALK_Y: i32 = 800;
/// Top of the corner towers and the central tower.
pub const TOWER_TOP_Y: i32 = 1_600;
/// Lift speed in units per second (1.5 m/s).
pub const LIFT_SPEED_UNITS_PER_SECOND: i32 = 150;
/// Lift rest at each stop (3 s).
pub const LIFT_DWELL_MS: u32 = 3_000;

const HALF_SPAN: i32 = (SPAWN_COLUMNS as i32 - 1) / 2 * MODULE_UNITS;

/// What a body is, for presentation materials; physics treats them all alike.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArenaPart {
    Ground,
    Wall,
    Parapet,
    Tower,
    Floor,
    Bridge,
    Column,
    Ramp,
    Lift,
}

/// The validated arena content shared by every match.
pub fn content() -> Result<Arc<ZoneContent>, ZoneError> {
    static CONTENT: OnceLock<Result<Arc<ZoneContent>, ZoneError>> = OnceLock::new();
    CONTENT.get_or_init(build_content).clone()
}

/// Every body of [`content`] (colliders, ramps and lift platforms) with its part, by body id.
#[must_use]
pub fn parts() -> &'static [(u32, ArenaPart)] {
    static PARTS: OnceLock<Vec<(u32, ArenaPart)>> = OnceLock::new();
    PARTS.get_or_init(|| layout().parts)
}

/// The shared spawn lattice: slot `s` stands at column `s % 23`, row `s / 23`.
#[must_use]
pub const fn spawn_grid() -> SpawnGrid {
    SpawnGrid {
        origin: [-HALF_SPAN, -HALF_SPAN],
        columns: SPAWN_COLUMNS,
        spacing: MODULE_UNITS,
    }
}

fn build_content() -> Result<Arc<ZoneContent>, ZoneError> {
    let Layout {
        colliders,
        ramps,
        platforms,
        ..
    } = layout();
    let definition =
        ZoneDefinition::with_spawn_grid(mmorpg_core::STANDARD_GRAVITY, spawn_grid(), colliders)?;
    let content = ZoneContent::from_definition(definition)
        .with_creature_abilities(Vec::new())?
        .with_platforms(platforms)?
        .with_ramps(ramps)?
        .with_character_persistence(CharacterPersistence::Ephemeral)
        .with_player_combat(PlayerCombatPolicy::FreeForAll);
    Ok(Arc::new(content))
}

/// An axis-aligned box by its corners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Cuboid {
    min: [i32; 3],
    max: [i32; 3],
}

const fn cuboid(x: [i32; 2], y: [i32; 2], z: [i32; 2]) -> Cuboid {
    Cuboid {
        min: [x[0], y[0], z[0]],
        max: [x[1], y[1], z[1]],
    }
}

impl Cuboid {
    /// A quarter turn about the vertical axis through the centre: (x, z) -> (-z, x).
    const fn turned(self) -> Self {
        Self {
            min: [-self.max[2], self.min[1], self.min[0]],
            max: [-self.min[2], self.max[1], self.max[0]],
        }
    }

    fn turned_by(self, quarters: u8) -> Self {
        (0..quarters).fold(self, |cuboid, _| cuboid.turned())
    }

    fn centre(self) -> [i32; 3] {
        [0, 1, 2].map(|axis| (self.min[axis] + self.max[axis]) / 2)
    }

    fn half_extents(self) -> [i32; 3] {
        [0, 1, 2].map(|axis| (self.max[axis] - self.min[axis]) / 2)
    }
}

/// The rise direction after a quarter turn (x, z) -> (-z, x).
const fn turned_rise(rise: RampRise) -> RampRise {
    match rise {
        RampRise::PosX => RampRise::PosZ,
        RampRise::PosZ => RampRise::NegX,
        RampRise::NegX => RampRise::NegZ,
        RampRise::NegZ => RampRise::PosX,
    }
}

// The south side (z < 0); the other three sides are its quarter turns.
const SIDE_SOLIDS: [(ArenaPart, Cuboid); 13] = [
    // Curtain wall either side of the gate, its lintel and the outer parapet.
    (
        ArenaPart::Wall,
        cuboid([-8_600, -300], [0, 800], [-8_600, -8_200]),
    ),
    (
        ArenaPart::Wall,
        cuboid([300, 8_600], [0, 800], [-8_600, -8_200]),
    ),
    (
        ArenaPart::Wall,
        cuboid([-300, 300], [600, 800], [-8_600, -8_200]),
    ),
    (
        ArenaPart::Parapet,
        cuboid([-8_600, 8_600], [800, 900], [-8_600, -8_540]),
    ),
    // South-west corner tower.
    (
        ArenaPart::Tower,
        cuboid([-8_760, -8_040], [0, 1_600], [-8_760, -8_040]),
    ),
    // Keep outer wall with its door (x 250..650) and lintel.
    (
        ArenaPart::Wall,
        cuboid([-1_260, 250], [0, 750], [-1_260, -1_140]),
    ),
    (
        ArenaPart::Wall,
        cuboid([650, 1_260], [0, 750], [-1_260, -1_140]),
    ),
    (
        ArenaPart::Wall,
        cuboid([250, 650], [600, 750], [-1_260, -1_140]),
    ),
    // Central hall wall with its door (x -200..200) and lintel.
    (
        ArenaPart::Wall,
        cuboid([-460, -200], [0, 750], [-460, -340]),
    ),
    (ArenaPart::Wall, cuboid([200, 460], [0, 750], [-460, -340])),
    (
        ArenaPart::Wall,
        cuboid([-200, 200], [600, 750], [-460, -340]),
    ),
    // Bridge from the keep roof to the gate lintel.
    (
        ArenaPart::Bridge,
        cuboid([-150, 150], [750, 800], [-8_200, -1_260]),
    ),
    // Solid shaft under the central tower lift.
    (
        ArenaPart::Column,
        cuboid([-535, -265], [0, 750], [-840, -570]),
    ),
];

const SIDE_RAMPS: [(Cuboid, RampRise); 5] = [
    // Courtyard up to the wall walk.
    (
        cuboid([-4_600, -4_200], [0, 800], [-8_200, -7_000]),
        RampRise::NegZ,
    ),
    (
        cuboid([4_200, 4_600], [0, 800], [-8_200, -7_000]),
        RampRise::NegZ,
    ),
    // Wall walk up to the corner towers at both ends.
    (
        cuboid([-8_040, -6_840], [800, 1_600], [-8_540, -8_200]),
        RampRise::NegX,
    ),
    (
        cuboid([6_840, 8_040], [800, 1_600], [-8_540, -8_200]),
        RampRise::PosX,
    ),
    // Keep ground floor up to the roof, along the outer wall.
    (
        cuboid([-1_140, 60], [0, 800], [-1_140, -840]),
        RampRise::NegX,
    ),
];

/// The roof opening above the keep ramp.
const SIDE_ROOF_HOLE: Cuboid = cuboid([-1_140, 60], [750, 800], [-1_140, -840]);

/// Lift pillars at their low stop: courtyard -> keep roof, keep roof -> central tower.
const SIDE_LIFTS: [Cuboid; 2] = [
    cuboid([-940, -660], [-800, 0], [-1_550, -1_270]),
    cuboid([-535, -265], [0, 800], [-840, -570]),
];
const LIFT_RISE: i32 = 800;

struct Layout {
    colliders: Vec<StaticCollider>,
    ramps: Vec<Ramp>,
    platforms: Vec<Platform>,
    parts: Vec<(u32, ArenaPart)>,
}

impl Layout {
    fn body(&mut self, part: ArenaPart) -> u32 {
        let id = u32::try_from(self.parts.len() + 1).expect("arena body count fits u32");
        self.parts.push((id, part));
        id
    }

    fn solid(&mut self, part: ArenaPart, cuboid: Cuboid) {
        let id = self.body(part);
        self.colliders.push(StaticCollider {
            id,
            position: cuboid.centre(),
            half_extents: cuboid.half_extents(),
        });
    }
}

fn layout() -> Layout {
    let mut layout = Layout {
        colliders: Vec::new(),
        ramps: Vec::new(),
        platforms: Vec::new(),
        parts: Vec::new(),
    };
    layout.solid(
        ArenaPart::Ground,
        cuboid([-32_000, 32_000], [-100, 0], [-32_000, 32_000]),
    );
    for quarter in 0..4 {
        for (part, cuboid) in SIDE_SOLIDS {
            layout.solid(part, cuboid.turned_by(quarter));
        }
    }
    // Keep roof: the keep's square less the four ramp openings, as disjoint slabs.
    let holes: Vec<Cuboid> = (0..4).map(|q| SIDE_ROOF_HOLE.turned_by(q)).collect();
    for slab in slabs_around(cuboid([-1_260, 1_260], [750, 800], [-1_260, 1_260]), &holes) {
        layout.solid(ArenaPart::Floor, slab);
    }
    layout.solid(
        ArenaPart::Tower,
        cuboid([-560, 560], [800, 1_600], [-560, 560]),
    );
    for quarter in 0..4 {
        for (cuboid, rises) in SIDE_RAMPS {
            let cuboid = cuboid.turned_by(quarter);
            let body = layout.body(ArenaPart::Ramp);
            layout.ramps.push(Ramp {
                body,
                position: cuboid.centre(),
                half_extents: cuboid.half_extents(),
                rises: (0..quarter).fold(rises, |rise, _| turned_rise(rise)),
            });
        }
    }
    for quarter in 0..4 {
        for pillar in SIDE_LIFTS {
            let pillar = pillar.turned_by(quarter);
            let body = layout.body(ArenaPart::Lift);
            let low = pillar.centre();
            let id = u8::try_from(layout.platforms.len() + 1).expect("lift count fits u8");
            layout.platforms.push(Platform {
                id: PlatformId::new(id),
                body,
                half_extents: pillar.half_extents(),
                low,
                high: [low[0], low[1] + LIFT_RISE, low[2]],
                speed: mmorpg_core::subunits_per_tick(LIFT_SPEED_UNITS_PER_SECOND),
                dwell: mmorpg_core::long_ticks_from_ms(LIFT_DWELL_MS),
            });
        }
    }
    layout
}

/// Splits `area` into disjoint boxes covering it except `holes` (same height), by cutting
/// it into x bands at every hole edge and each band into z runs between the holes.
fn slabs_around(area: Cuboid, holes: &[Cuboid]) -> Vec<Cuboid> {
    let mut xs: Vec<i32> = holes
        .iter()
        .flat_map(|hole| [hole.min[0], hole.max[0]])
        .chain([area.min[0], area.max[0]])
        .filter(|x| (area.min[0]..=area.max[0]).contains(x))
        .collect();
    xs.sort_unstable();
    xs.dedup();
    let mut slabs = Vec::new();
    for band in xs.windows(2) {
        let [x0, x1] = [band[0], band[1]];
        let mut cuts: Vec<[i32; 2]> = holes
            .iter()
            .filter(|hole| hole.min[0] <= x0 && x1 <= hole.max[0])
            .map(|hole| [hole.min[2], hole.max[2]])
            .collect();
        cuts.sort_unstable();
        let mut z = area.min[2];
        for [cut_min, cut_max] in cuts.into_iter().chain([[area.max[2], area.max[2]]]) {
            if z < cut_min {
                slabs.push(cuboid([x0, x1], [area.min[1], area.max[1]], [z, cut_min]));
            }
            z = z.max(cut_max);
        }
    }
    slabs
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_core::{
        ClassChoice, EphemeralCharacterBuild, PlayerClass, Sex, ZoneId, ZoneSimulation,
    };
    use strum::VariantArray;

    #[test]
    fn the_mode_arena_admits_full_capacity_through_shared_character_rules() {
        let content = content().unwrap();
        let mut zone =
            ZoneSimulation::with_content(ZoneId::new(100), Arc::clone(&content)).unwrap();
        for player in 1..=crate::MAX_PLAYERS_PER_MATCH as u32 {
            let build = EphemeralCharacterBuild::standard(ClassChoice {
                class: PlayerClass::VARIANTS[(player as usize - 1) % PlayerClass::VARIANTS.len()],
                sex: Sex::Male,
            })
            .unwrap();
            zone.admit_ephemeral(player, &build).unwrap();
        }
        assert!(zone.players_are_hostile(1, 2));
        let initial = zone.snapshot().unwrap();
        assert_eq!(initial.players.len(), crate::MAX_PLAYERS_PER_MATCH);
        assert!(initial.creatures.is_empty());
        let mut restored = ZoneSimulation::from_snapshot(initial, content).unwrap();
        zone.advance_tick().unwrap();
        restored.advance_tick().unwrap();
        assert_eq!(zone.snapshot().unwrap(), restored.snapshot().unwrap());
    }

    #[test]
    fn roof_slabs_cover_the_keep_except_the_ramp_openings() {
        let area = cuboid([-1_260, 1_260], [750, 800], [-1_260, 1_260]);
        let holes: Vec<Cuboid> = (0..4).map(|q| SIDE_ROOF_HOLE.turned_by(q)).collect();
        let slabs = slabs_around(area, &holes);
        let area_of = |c: &Cuboid| i64::from(c.max[0] - c.min[0]) * i64::from(c.max[2] - c.min[2]);
        let covered: i64 = slabs.iter().map(area_of).sum();
        let open: i64 = holes.iter().map(area_of).sum();
        assert_eq!(covered + open, 2_520 * 2_520);
        let overlaps = |a: &Cuboid, b: &Cuboid| {
            [0, 2]
                .iter()
                .all(|&axis| a.min[axis] < b.max[axis] && b.min[axis] < a.max[axis])
        };
        for slab in &slabs {
            assert!(!holes.iter().any(|hole| overlaps(slab, hole)));
        }
        for (index, slab) in slabs.iter().enumerate() {
            assert!(!slabs[index + 1..].iter().any(|other| overlaps(slab, other)));
        }
    }

    #[test]
    fn every_side_is_a_quarter_turn_of_the_next() {
        let turn = |c: Cuboid| c.turned();
        assert_eq!(
            (0..4).fold(cuboid([1, 2], [3, 4], [5, 7]), |c, _| turn(c)),
            cuboid([1, 2], [3, 4], [5, 7])
        );
        let rise = (0..4).fold(RampRise::NegZ, |rise, _| turned_rise(rise));
        assert_eq!(rise, RampRise::NegZ);
    }
}
