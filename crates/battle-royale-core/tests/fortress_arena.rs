//! Fortress arena fixture (#31): collision layout, symmetry, spawn clearance and the
//! intended routes between elevations, walked by a real character in the shared zone.
use battle_royale_core::arena::{self, ArenaPart, TOWER_TOP_Y, WALL_WALK_Y};
use mmorpg_core::{
    ClassChoice, EphemeralCharacterBuild, MAX_PLAYERS_PER_ZONE, PLAYER_HALF_EXTENTS_UNITS,
    Platform, PlatformPhase, PlayerClass, Ramp, RampRise, Sex, ZoneCommand, ZoneId, ZoneSimulation,
};
use std::collections::BTreeSet;

type Bounds = ([i32; 3], [i32; 3]);

fn bounds(position: [i32; 3], half: [i32; 3]) -> Bounds {
    (
        [0, 1, 2].map(|axis| position[axis] - half[axis]),
        [0, 1, 2].map(|axis| position[axis] + half[axis]),
    )
}

fn turned((min, max): Bounds) -> Bounds {
    ([-max[2], min[1], min[0]], [-min[2], max[1], max[0]])
}

fn turned_rise(rise: RampRise) -> RampRise {
    match rise {
        RampRise::PosX => RampRise::PosZ,
        RampRise::PosZ => RampRise::NegX,
        RampRise::NegX => RampRise::NegZ,
        RampRise::NegZ => RampRise::PosX,
    }
}

fn overlaps((a_min, a_max): Bounds, (b_min, b_max): Bounds) -> bool {
    (0..3).all(|axis| a_min[axis] < b_max[axis] && b_min[axis] < a_max[axis])
}

/// Everything a lift ever occupies: its pillar from the low stop to the high stop.
fn swept(platform: &Platform) -> Bounds {
    let (low_min, low_max) = bounds(platform.low, platform.half_extents);
    let (high_min, high_max) = bounds(platform.high, platform.half_extents);
    (
        [0, 1, 2].map(|axis| low_min[axis].min(high_min[axis])),
        [0, 1, 2].map(|axis| low_max[axis].max(high_max[axis])),
    )
}

#[test]
fn the_fortress_is_symmetric_identified_and_keeps_every_spawn_slot_clear() {
    let content = arena::content().unwrap();
    assert_eq!(
        content.revision(),
        arena::content().unwrap().revision(),
        "one shared content identity"
    );

    // Every body is listed exactly once with its presentation part.
    let mut bodies: Vec<u32> = content
        .definition()
        .colliders()
        .iter()
        .map(|c| c.id)
        .chain(content.ramps().iter().map(|r| r.body))
        .chain(content.platforms().iter().map(|p| p.body))
        .collect();
    bodies.sort_unstable();
    let listed: Vec<u32> = arena::parts().iter().map(|(id, _)| *id).collect();
    assert_eq!(bodies, listed);
    let part = |id: u32| {
        arena::parts()
            .iter()
            .find(|(body, _)| *body == id)
            .unwrap()
            .1
    };
    assert_eq!(content.platforms().len(), 8);
    assert!(
        content
            .platforms()
            .iter()
            .all(|p| part(p.body) == ArenaPart::Lift)
    );
    assert_eq!(content.ramps().len(), 20);

    // A quarter turn maps the fortress onto itself.
    let solids: BTreeSet<(Bounds, ArenaPart)> = content
        .definition()
        .colliders()
        .iter()
        .map(|c| (bounds(c.position, c.half_extents), part(c.id)))
        .collect();
    let turned_solids: BTreeSet<(Bounds, ArenaPart)> =
        solids.iter().map(|(b, p)| (turned(*b), *p)).collect();
    // The roof slabs are cut along x, so compare everything but the floor slabs exactly.
    let without_floor = |set: &BTreeSet<(Bounds, ArenaPart)>| -> BTreeSet<(Bounds, ArenaPart)> {
        set.iter()
            .filter(|(_, p)| *p != ArenaPart::Floor)
            .copied()
            .collect()
    };
    assert_eq!(without_floor(&solids), without_floor(&turned_solids));
    let ramps: BTreeSet<(Bounds, RampRise)> = content
        .ramps()
        .iter()
        .map(|r: &Ramp| (bounds(r.position, r.half_extents), r.rises))
        .collect();
    let turned_ramps: BTreeSet<(Bounds, RampRise)> = ramps
        .iter()
        .map(|(b, rise)| (turned(*b), turned_rise(*rise)))
        .collect();
    assert_eq!(ramps, turned_ramps);
    let lifts: BTreeSet<Bounds> = content.platforms().iter().map(swept).collect();
    let turned_lifts: BTreeSet<Bounds> = lifts.iter().map(|b| turned(*b)).collect();
    assert_eq!(lifts, turned_lifts);

    // Every shared spawn slot stands clear of ramps and of all space a lift sweeps; the
    // shared zone already refuses content whose static boxes overlap a slot.
    let grid = content.definition().spawn_grid();
    for slot in 0..MAX_PLAYERS_PER_ZONE {
        let feet = grid.feet(u16::try_from(slot).unwrap()).unwrap();
        let body = bounds(
            [feet[0], PLAYER_HALF_EXTENTS_UNITS[1], feet[2]],
            PLAYER_HALF_EXTENTS_UNITS,
        );
        for ramp in content.ramps() {
            assert!(
                !overlaps(body, bounds(ramp.position, ramp.half_extents)),
                "slot {slot} under ramp {}",
                ramp.body
            );
        }
        for platform in content.platforms() {
            assert!(
                !overlaps(body, swept(platform)),
                "slot {slot} in lift {}",
                platform.body
            );
        }
    }
}

/// One character walking the fortress by held movement toward waypoints.
struct Walker {
    zone: ZoneSimulation,
    sequence: u32,
}

const PLAYER: u32 = 1;
/// Body centre height standing on a surface at `top`.
fn standing_on(top: i32) -> f32 {
    (top + PLAYER_HALF_EXTENTS_UNITS[1]) as f32
}

impl Walker {
    /// Admits one character at spawn slot 0, outside the south-west corner.
    fn new() -> Self {
        let mut zone =
            ZoneSimulation::with_content(ZoneId::new(31), arena::content().unwrap()).unwrap();
        let build = EphemeralCharacterBuild::standard(ClassChoice {
            class: PlayerClass::Ranger,
            sex: Sex::Female,
        })
        .unwrap();
        zone.admit_ephemeral(PLAYER, &build).unwrap();
        let mut walker = Self { zone, sequence: 0 };
        walker.idle(5);
        let start = walker.position();
        assert_eq!([start[0], start[2]], [-8_800.0, -8_800.0]);
        walker
    }

    fn position(&self) -> [f32; 3] {
        let snapshot = self.zone.snapshot().unwrap();
        snapshot
            .players
            .iter()
            .find(|p| p.player_id == PLAYER)
            .unwrap()
            .position
    }

    fn command(&mut self, forward: i8, facing: u16) {
        self.sequence += 1;
        self.zone
            .apply_command(
                PLAYER,
                self.sequence,
                ZoneCommand::Move {
                    forward,
                    strafe: 0,
                    facing,
                },
            )
            .unwrap();
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.zone.advance_tick().unwrap();
        }
    }

    /// Runs straight toward `(x, z)` and stops there; panics when it never arrives.
    fn walk_to(&mut self, x: f32, z: f32) -> [f32; 3] {
        for _ in 0..3_000 {
            let at = self.position();
            let (dx, dz) = (x - at[0], z - at[2]);
            if dx.hypot(dz) < 20.0 {
                self.command(0, 0);
                self.idle(30);
                return self.position();
            }
            let yaw = dx.atan2(dz) / std::f32::consts::TAU * 65_536.0;
            self.command(1, (yaw.round() as i32).rem_euclid(65_536) as u16);
            self.zone.advance_tick().unwrap();
        }
        panic!("stuck at {:?} on the way to ({x}, {z})", self.position());
    }

    fn lift(&self, near: [i32; 2]) -> Platform {
        *self
            .zone
            .content()
            .platforms()
            .iter()
            .find(|p| p.low[0] == near[0] && p.low[2] == near[1])
            .expect("a lift stands there")
    }

    /// Waits until `lift` has just entered `phase`.
    fn wait_for(&mut self, lift: &Platform, phase: PlatformPhase) {
        for _ in 0..2 * lift.period() {
            let (now, elapsed) = lift.phase_at(self.zone.current_tick());
            if now == phase && elapsed < 10 {
                return;
            }
            self.zone.advance_tick().unwrap();
        }
        panic!("lift never reached {phase:?}");
    }
}

fn assert_height(at: [f32; 3], top: i32) {
    assert!(
        (at[1] - standing_on(top)).abs() < 5.0,
        "expected to stand on y = {top}, at {at:?}"
    );
}

#[test]
fn courtyard_ramps_lead_to_the_wall_walk_and_corner_towers() {
    let mut walker = Walker::new();
    assert_height(walker.walk_to(0.0, -8_800.0), 0);
    // Through the south gate under the lintel into the courtyard.
    assert_height(walker.walk_to(0.0, -6_800.0), 0);
    assert_height(walker.walk_to(-4_400.0, -6_800.0), 0);
    // Up the courtyard ramp onto the wall walk.
    assert_height(walker.walk_to(-4_400.0, -8_370.0), WALL_WALK_Y);
    assert_height(walker.walk_to(-6_700.0, -8_370.0), WALL_WALK_Y);
    // Up to the south-west tower top, then down onto the west wall walk.
    assert_height(walker.walk_to(-8_400.0, -8_370.0), TOWER_TOP_Y);
    assert_height(walker.walk_to(-8_370.0, -8_100.0), TOWER_TOP_Y);
    assert_height(walker.walk_to(-8_370.0, -6_500.0), WALL_WALK_Y);
}

#[test]
fn the_keep_ramp_and_lift_reach_the_roof_and_the_central_tower() {
    let mut walker = Walker::new();
    walker.walk_to(0.0, -8_800.0);
    walker.walk_to(0.0, -6_800.0);
    // Through the keep's south door into the ring corridor.
    walker.walk_to(450.0, -2_000.0);
    assert_height(walker.walk_to(450.0, -1_000.0), 0);
    assert_height(walker.walk_to(150.0, -990.0), 0);
    // Up the interior ramp onto the roof.
    assert_height(walker.walk_to(-1_200.0, -990.0), WALL_WALK_Y);
    assert_height(walker.walk_to(-1_200.0, -705.0), WALL_WALK_Y);
    assert_height(walker.walk_to(-700.0, -705.0), WALL_WALK_Y);
    // Board the lift at rest, ride it up and step off onto the central tower.
    let lift = walker.lift([-400, -705]);
    walker.wait_for(&lift, PlatformPhase::AtLow);
    assert_height(walker.walk_to(-400.0, -705.0), WALL_WALK_Y);
    walker.wait_for(&lift, PlatformPhase::AtHigh);
    assert_height(walker.position(), TOWER_TOP_Y);
    assert_height(walker.walk_to(-400.0, -300.0), TOWER_TOP_Y);
}

#[test]
fn the_courtyard_lift_and_bridge_join_the_keep_roof_to_the_wall_walk() {
    let mut walker = Walker::new();
    walker.walk_to(0.0, -8_800.0);
    walker.walk_to(0.0, -6_800.0);
    assert_height(walker.walk_to(-800.0, -2_000.0), 0);
    // Board the courtyard lift flush with the ground and ride it to the keep roof.
    let lift = walker.lift([-800, -1_410]);
    walker.wait_for(&lift, PlatformPhase::AtLow);
    assert_height(walker.walk_to(-800.0, -1_410.0), 0);
    walker.wait_for(&lift, PlatformPhase::AtHigh);
    assert_height(walker.position(), WALL_WALK_Y);
    assert_height(walker.walk_to(-800.0, -1_200.0), WALL_WALK_Y);
    assert_height(walker.walk_to(0.0, -1_200.0), WALL_WALK_Y);
    // Along the bridge to the south gate lintel on the wall walk.
    assert_height(walker.walk_to(0.0, -4_000.0), WALL_WALK_Y);
    assert_height(walker.walk_to(0.0, -8_400.0), WALL_WALK_Y);
}
