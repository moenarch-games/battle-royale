#![forbid(unsafe_code)]

mod arena;

use physics_engine::{BodyId, RigidBody, Vec3i, World, WorldConfig};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

pub type PlayerId = u32;

pub const TICK_HZ: u16 = 30;
pub const MAX_PLAYERS_PER_MATCH: usize = 100;
pub const MIN_PLAYERS_TO_START: usize = 2;
pub const LOBBY_COUNTDOWN_TICKS: u64 = 60;
pub const DROP_DURATION_TICKS: u64 = 90;
pub const STORM_DELAY_TICKS: u64 = 90;
pub const STORM_SHRINK_DURATION_TICKS: u64 = 900;
pub const INITIAL_STORM_RADIUS_UNITS: i32 = 50_000;
pub const FINAL_STORM_RADIUS_UNITS: i32 = 2_000;
pub const STORM_DAMAGE_PER_TICK: u16 = 2;
pub const MAX_HEALTH: u16 = 100;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const RULESET_REVISION: u64 = 1;
pub const INTEREST_RADIUS_UNITS: i32 = 20_000;

const PLAYER_BODY_BASE: u64 = 1_000_000;
const PLAYER_HALF_EXTENTS: Vec3i = Vec3i::new(30, 50, 30);
const PLAYER_SPEED: i32 = 18;
const PLAYER_DIAGONAL_SPEED: i32 = 13;
const SPAWN_GRID_WIDTH: u16 = 10;
const SPAWN_SPACING: i32 = 800;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BattleMatchId(u64);

impl BattleMatchId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchPhase {
    Waiting,
    Drop,
    Combat,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EliminationCause {
    Storm,
    SessionExpired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchCommand {
    SetMovement { x: i8, z: i8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StormSnapshot {
    pub center: [i32; 2],
    pub radius: i32,
    pub damage_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub player_id: PlayerId,
    pub position: [i32; 3],
    pub velocity: [i32; 3],
    pub health: u16,
    pub alive: bool,
    pub elimination_tick: Option<u64>,
    pub elimination_cause: Option<EliminationCause>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalPlayerSnapshot {
    pub player_id: PlayerId,
    pub position: [i32; 3],
    pub velocity: [i32; 3],
    pub movement_x: i8,
    pub movement_z: i8,
    pub last_sequence: u32,
    pub spawn_slot: u16,
    pub health: u16,
    pub alive: bool,
    pub elimination_tick: Option<u64>,
    pub elimination_cause: Option<EliminationCause>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchSnapshot {
    pub schema_version: u16,
    pub ruleset_revision: u64,
    pub match_id: BattleMatchId,
    pub tick: u64,
    pub phase: MatchPhase,
    pub phase_started_tick: u64,
    pub combat_started_tick: Option<u64>,
    pub lobby_ready_tick: Option<u64>,
    pub storm: StormSnapshot,
    pub winner: Option<PlayerId>,
    pub acknowledged_sequence: u32,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalMatchSnapshot {
    pub schema_version: u16,
    pub ruleset_revision: u64,
    pub match_id: BattleMatchId,
    pub tick: u64,
    pub phase: MatchPhase,
    pub phase_started_tick: u64,
    pub combat_started_tick: Option<u64>,
    pub lobby_ready_tick: Option<u64>,
    pub storm: StormSnapshot,
    pub winner: Option<PlayerId>,
    pub players: Vec<CanonicalPlayerSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchError {
    message: String,
}

impl MatchError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for MatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MatchError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlayerState {
    movement_x: i8,
    movement_z: i8,
    last_sequence: u32,
    spawn_slot: u16,
    health: u16,
    alive: bool,
    position: [i32; 3],
    velocity: [i32; 3],
    elimination_tick: Option<u64>,
    elimination_cause: Option<EliminationCause>,
}

pub struct BattleRoyaleMatch {
    match_id: BattleMatchId,
    tick: u64,
    phase: MatchPhase,
    phase_started_tick: u64,
    combat_started_tick: Option<u64>,
    lobby_ready_tick: Option<u64>,
    world: World,
    players: BTreeMap<PlayerId, PlayerState>,
    winner: Option<PlayerId>,
}

impl BattleRoyaleMatch {
    #[must_use]
    pub fn new(match_id: BattleMatchId) -> Self {
        Self {
            match_id,
            tick: 0,
            phase: MatchPhase::Waiting,
            phase_started_tick: 0,
            combat_started_tick: None,
            lobby_ready_tick: None,
            world: World::new(WorldConfig {
                gravity: Vec3i::ZERO,
                ..WorldConfig::default()
            }),
            players: BTreeMap::new(),
            winner: None,
        }
    }

    #[must_use]
    pub const fn match_id(&self) -> BattleMatchId {
        self.match_id
    }

    #[must_use]
    pub const fn current_tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub const fn phase(&self) -> MatchPhase {
        self.phase
    }

    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    #[must_use]
    pub fn alive_count(&self) -> usize {
        self.players.values().filter(|player| player.alive).count()
    }

    #[must_use]
    pub const fn winner(&self) -> Option<PlayerId> {
        self.winner
    }

    pub fn add_player(&mut self, player_id: PlayerId) -> Result<(), MatchError> {
        if self.phase != MatchPhase::Waiting {
            return Err(MatchError::new("match admission is closed"));
        }
        if self.players.contains_key(&player_id) {
            return Err(MatchError::new("player already exists in match"));
        }
        if self.players.len() >= MAX_PLAYERS_PER_MATCH {
            return Err(MatchError::new("match player capacity reached"));
        }

        let spawn_slot = self
            .available_spawn_slot()
            .ok_or_else(|| MatchError::new("match spawn capacity reached"))?;
        let position = Self::spawn_position(spawn_slot);
        self.world
            .add_body(RigidBody::dynamic(
                Self::body_id(player_id),
                Vec3i::new(position[0], position[1], position[2]),
                Vec3i::ZERO,
                PLAYER_HALF_EXTENTS,
            ))
            .map_err(physics_error)?;

        self.players.insert(
            player_id,
            PlayerState {
                movement_x: 0,
                movement_z: 0,
                last_sequence: 0,
                spawn_slot,
                health: MAX_HEALTH,
                alive: true,
                position,
                velocity: [0, 0, 0],
                elimination_tick: None,
                elimination_cause: None,
            },
        );
        self.refresh_lobby_ready_tick();
        Ok(())
    }

    pub fn remove_player(&mut self, player_id: PlayerId) -> bool {
        if !self.players.contains_key(&player_id) {
            return false;
        }

        if self.phase == MatchPhase::Waiting {
            self.players.remove(&player_id);
            self.world.remove_body(Self::body_id(player_id));
            self.refresh_lobby_ready_tick();
            return true;
        }

        if self.players[&player_id].alive {
            self.eliminate_player(player_id, EliminationCause::SessionExpired);
            self.finish_if_decided();
        }
        true
    }

    pub fn apply_command(
        &mut self,
        player_id: PlayerId,
        sequence: u32,
        command: MatchCommand,
    ) -> Result<(), MatchError> {
        if self.phase == MatchPhase::Finished {
            return Err(MatchError::new("match is finished"));
        }

        let player = self
            .players
            .get_mut(&player_id)
            .ok_or_else(|| MatchError::new("unknown player"))?;
        if !player.alive {
            return Err(MatchError::new("player is eliminated"));
        }
        if sequence == 0 || sequence <= player.last_sequence {
            return Err(MatchError::new("command sequence is stale"));
        }

        match command {
            MatchCommand::SetMovement { x, z } => {
                if !(-1..=1).contains(&x) || !(-1..=1).contains(&z) {
                    return Err(MatchError::new(
                        "movement components must be between -1 and 1",
                    ));
                }
                player.movement_x = x;
                player.movement_z = z;
            }
        }

        player.last_sequence = sequence;
        Ok(())
    }

    pub fn advance_tick(&mut self) -> Result<(), MatchError> {
        let next_tick = self
            .tick
            .checked_add(1)
            .ok_or_else(|| MatchError::new("match tick overflow"))?;

        if self.phase != MatchPhase::Finished {
            self.advance_movement()?;
        }
        self.tick = next_tick;

        match self.phase {
            MatchPhase::Waiting => {
                self.refresh_lobby_ready_tick();
                let countdown_complete = self.lobby_ready_tick.is_some_and(|ready_tick| {
                    self.tick.saturating_sub(ready_tick) >= LOBBY_COUNTDOWN_TICKS
                });
                if self.players.len() == MAX_PLAYERS_PER_MATCH || countdown_complete {
                    self.start_phase(MatchPhase::Drop);
                }
            }
            MatchPhase::Drop => {
                if self.tick.saturating_sub(self.phase_started_tick) >= DROP_DURATION_TICKS {
                    self.start_phase(MatchPhase::Combat);
                    self.finish_if_decided();
                }
            }
            MatchPhase::Combat => {
                self.apply_storm_damage();
                self.finish_if_decided();
            }
            MatchPhase::Finished => {}
        }
        Ok(())
    }

    pub fn snapshot(&self) -> CanonicalMatchSnapshot {
        CanonicalMatchSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            ruleset_revision: RULESET_REVISION,
            match_id: self.match_id,
            tick: self.tick,
            phase: self.phase,
            phase_started_tick: self.phase_started_tick,
            combat_started_tick: self.combat_started_tick,
            lobby_ready_tick: self.lobby_ready_tick,
            storm: self.storm_snapshot(),
            winner: self.winner,
            players: self
                .players
                .iter()
                .map(|(&player_id, state)| Self::canonical_player_snapshot(player_id, *state))
                .collect(),
        }
    }

    pub fn snapshot_for_player(&self, player_id: PlayerId) -> Result<MatchSnapshot, MatchError> {
        let viewer = self
            .players
            .get(&player_id)
            .ok_or_else(|| MatchError::new("unknown player"))?;
        let radius = i128::from(INTEREST_RADIUS_UNITS);
        let radius_squared = radius * radius;

        let players = self
            .players
            .iter()
            .filter_map(|(&candidate_id, state)| {
                if candidate_id == player_id {
                    return Some(Self::player_snapshot(candidate_id, *state));
                }
                let dx = i128::from(state.position[0]) - i128::from(viewer.position[0]);
                let dz = i128::from(state.position[2]) - i128::from(viewer.position[2]);
                ((dx * dx) + (dz * dz) <= radius_squared)
                    .then(|| Self::player_snapshot(candidate_id, *state))
            })
            .collect();

        Ok(MatchSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            ruleset_revision: RULESET_REVISION,
            match_id: self.match_id,
            tick: self.tick,
            phase: self.phase,
            phase_started_tick: self.phase_started_tick,
            combat_started_tick: self.combat_started_tick,
            lobby_ready_tick: self.lobby_ready_tick,
            storm: self.storm_snapshot(),
            winner: self.winner,
            acknowledged_sequence: viewer.last_sequence,
            players,
        })
    }

    #[must_use]
    pub fn storm_snapshot(&self) -> StormSnapshot {
        let center = Self::storm_center(self.match_id);
        if self.phase != MatchPhase::Combat && self.phase != MatchPhase::Finished {
            return StormSnapshot {
                center,
                radius: INITIAL_STORM_RADIUS_UNITS,
                damage_active: false,
            };
        }

        let combat_started_tick = self
            .combat_started_tick
            .expect("combat and finished phases retain their combat start tick");
        let elapsed = self.tick.saturating_sub(combat_started_tick);
        if elapsed <= STORM_DELAY_TICKS {
            return StormSnapshot {
                center,
                radius: INITIAL_STORM_RADIUS_UNITS,
                damage_active: false,
            };
        }

        let shrink_elapsed = elapsed
            .saturating_sub(STORM_DELAY_TICKS)
            .min(STORM_SHRINK_DURATION_TICKS);
        let radius_delta = i64::from(INITIAL_STORM_RADIUS_UNITS - FINAL_STORM_RADIUS_UNITS);
        let shrink = radius_delta.saturating_mul(i64::try_from(shrink_elapsed).unwrap_or(i64::MAX))
            / i64::try_from(STORM_SHRINK_DURATION_TICKS).unwrap_or(1);
        let radius = i64::from(INITIAL_STORM_RADIUS_UNITS).saturating_sub(shrink);

        StormSnapshot {
            center,
            radius: i32::try_from(radius).unwrap_or(FINAL_STORM_RADIUS_UNITS),
            damage_active: true,
        }
    }

    fn advance_movement(&mut self) -> Result<(), MatchError> {
        let velocities = self
            .players
            .iter()
            .filter(|(_, state)| state.alive)
            .map(|(&player_id, state)| (player_id, Self::movement_velocity(*state)))
            .collect::<Vec<_>>();

        for (player_id, velocity) in velocities {
            self.world
                .set_velocity(Self::body_id(player_id), velocity)
                .map_err(physics_error)?;
        }

        self.world.step(1).map_err(physics_error)?;

        let body_states = self
            .players
            .iter()
            .filter(|(_, state)| state.alive)
            .map(|(&player_id, _)| {
                let body = self
                    .world
                    .body(Self::body_id(player_id))
                    .ok_or_else(|| MatchError::new("player physics body is missing"))?;
                Ok((player_id, body.position(), body.velocity()))
            })
            .collect::<Result<Vec<_>, MatchError>>()?;

        for (player_id, position, velocity) in body_states {
            let player = self
                .players
                .get_mut(&player_id)
                .ok_or_else(|| MatchError::new("player disappeared during physics update"))?;
            player.position = [position.x, position.y, position.z];
            player.velocity = [velocity.x, velocity.y, velocity.z];
        }
        Ok(())
    }

    fn apply_storm_damage(&mut self) {
        let storm = self.storm_snapshot();
        if !storm.damage_active {
            return;
        }

        let radius = i128::from(storm.radius);
        let radius_squared = radius * radius;
        let outside = self
            .players
            .iter()
            .filter_map(|(&player_id, state)| {
                if !state.alive {
                    return None;
                }
                let dx = i128::from(state.position[0]) - i128::from(storm.center[0]);
                let dz = i128::from(state.position[2]) - i128::from(storm.center[1]);
                ((dx * dx) + (dz * dz) > radius_squared).then_some(player_id)
            })
            .collect::<Vec<_>>();

        for player_id in outside {
            let eliminated = if let Some(player) = self.players.get_mut(&player_id) {
                player.health = player.health.saturating_sub(STORM_DAMAGE_PER_TICK);
                player.health == 0
            } else {
                false
            };
            if eliminated {
                self.eliminate_player(player_id, EliminationCause::Storm);
            }
        }
    }

    fn eliminate_player(&mut self, player_id: PlayerId, cause: EliminationCause) {
        let was_alive = if let Some(player) = self.players.get_mut(&player_id) {
            if !player.alive {
                return;
            }
            player.alive = false;
            player.health = 0;
            player.movement_x = 0;
            player.movement_z = 0;
            player.velocity = [0, 0, 0];
            player.elimination_tick = Some(self.tick);
            player.elimination_cause = Some(cause);
            true
        } else {
            false
        };

        if was_alive {
            self.world.remove_body(Self::body_id(player_id));
        }
    }

    fn finish_if_decided(&mut self) {
        if self.phase != MatchPhase::Combat {
            return;
        }
        let mut alive = self
            .players
            .iter()
            .filter_map(|(&player_id, state)| state.alive.then_some(player_id));
        let first = alive.next();
        if first.is_some() && alive.next().is_none() {
            self.winner = first;
            self.start_phase(MatchPhase::Finished);
        } else if first.is_none() {
            self.winner = None;
            self.start_phase(MatchPhase::Finished);
        }
    }

    fn refresh_lobby_ready_tick(&mut self) {
        if self.phase != MatchPhase::Waiting {
            return;
        }
        if self.players.len() >= MIN_PLAYERS_TO_START {
            if self.lobby_ready_tick.is_none() {
                self.lobby_ready_tick = Some(self.tick);
            }
        } else {
            self.lobby_ready_tick = None;
        }
    }

    fn start_phase(&mut self, phase: MatchPhase) {
        self.phase = phase;
        self.phase_started_tick = self.tick;
        if phase == MatchPhase::Combat {
            self.combat_started_tick = Some(self.tick);
        }
        if phase != MatchPhase::Waiting {
            self.lobby_ready_tick = None;
        }
    }

    fn available_spawn_slot(&self) -> Option<u16> {
        (0..u16::try_from(MAX_PLAYERS_PER_MATCH).ok()?).find(|slot| {
            self.players
                .values()
                .all(|player| player.spawn_slot != *slot)
        })
    }

    fn spawn_position(slot: u16) -> [i32; 3] {
        let row = slot / SPAWN_GRID_WIDTH;
        let column = slot % SPAWN_GRID_WIDTH;
        let half = i32::from(SPAWN_GRID_WIDTH - 1) * SPAWN_SPACING / 2;
        [
            i32::from(column) * SPAWN_SPACING - half,
            0,
            i32::from(row) * SPAWN_SPACING - half,
        ]
    }

    fn movement_velocity(state: PlayerState) -> Vec3i {
        let speed = if state.movement_x != 0 && state.movement_z != 0 {
            PLAYER_DIAGONAL_SPEED
        } else {
            PLAYER_SPEED
        };
        Vec3i::new(
            i32::from(state.movement_x) * speed,
            0,
            i32::from(state.movement_z) * speed,
        )
    }

    fn body_id(player_id: PlayerId) -> BodyId {
        BodyId(PLAYER_BODY_BASE + u64::from(player_id))
    }

    fn storm_center(match_id: BattleMatchId) -> [i32; 2] {
        let first = Self::mix(match_id.get() ^ 0x9e37_79b9_7f4a_7c15);
        let second = Self::mix(match_id.get() ^ 0xc2b2_ae3d_27d4_eb4f);
        [Self::axis(first), Self::axis(second)]
    }

    fn mix(mut value: u64) -> u64 {
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn axis(value: u64) -> i32 {
        i32::try_from(value % 10_001).unwrap_or(0) - 5_000
    }

    fn player_snapshot(player_id: PlayerId, state: PlayerState) -> PlayerSnapshot {
        PlayerSnapshot {
            player_id,
            position: state.position,
            velocity: state.velocity,
            health: state.health,
            alive: state.alive,
            elimination_tick: state.elimination_tick,
            elimination_cause: state.elimination_cause,
        }
    }

    fn canonical_player_snapshot(
        player_id: PlayerId,
        state: PlayerState,
    ) -> CanonicalPlayerSnapshot {
        CanonicalPlayerSnapshot {
            player_id,
            position: state.position,
            velocity: state.velocity,
            movement_x: state.movement_x,
            movement_z: state.movement_z,
            last_sequence: state.last_sequence,
            spawn_slot: state.spawn_slot,
            health: state.health,
            alive: state.alive,
            elimination_tick: state.elimination_tick,
            elimination_cause: state.elimination_cause,
        }
    }
}

fn physics_error(error: impl fmt::Display) -> MatchError {
    MatchError::new(format!("physics error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_combat(simulation: &mut BattleRoyaleMatch) {
        simulation.add_player(1).unwrap();
        simulation.add_player(2).unwrap();
        for _ in 0..LOBBY_COUNTDOWN_TICKS {
            simulation.advance_tick().unwrap();
        }
        assert_eq!(simulation.phase(), MatchPhase::Drop);
        for _ in 0..DROP_DURATION_TICKS {
            simulation.advance_tick().unwrap();
        }
        assert_eq!(simulation.phase(), MatchPhase::Combat);
    }

    #[test]
    fn lobby_countdown_closes_admission_deterministically() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(7));
        simulation.add_player(1).unwrap();
        for _ in 0..(LOBBY_COUNTDOWN_TICKS + 5) {
            simulation.advance_tick().unwrap();
        }
        assert_eq!(simulation.phase(), MatchPhase::Waiting);

        simulation.add_player(2).unwrap();
        for _ in 0..(LOBBY_COUNTDOWN_TICKS - 1) {
            simulation.advance_tick().unwrap();
        }
        assert_eq!(simulation.phase(), MatchPhase::Waiting);
        simulation.advance_tick().unwrap();

        assert_eq!(simulation.phase(), MatchPhase::Drop);
        assert_eq!(
            simulation.add_player(3).unwrap_err().message(),
            "match admission is closed"
        );
    }

    #[test]
    fn stale_commands_fail_closed_and_valid_movement_advances() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(9));
        simulation.add_player(1).unwrap();
        simulation
            .apply_command(1, 1, MatchCommand::SetMovement { x: 1, z: 0 })
            .unwrap();
        simulation.advance_tick().unwrap();

        let snapshot = simulation.snapshot();
        assert!(snapshot.players[0].position[0] > -3_600);
        assert_eq!(
            simulation
                .apply_command(1, 1, MatchCommand::SetMovement { x: 0, z: 1 })
                .unwrap_err()
                .message(),
            "command sequence is stale"
        );
    }

    #[test]
    fn safe_zone_schedule_is_deterministic_and_monotonic() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(11));
        start_combat(&mut simulation);

        let before = simulation.storm_snapshot();
        for _ in 0..(STORM_DELAY_TICKS + 1) {
            simulation.advance_tick().unwrap();
        }
        let shrinking = simulation.storm_snapshot();
        for _ in 0..STORM_SHRINK_DURATION_TICKS {
            if simulation.phase() == MatchPhase::Finished {
                break;
            }
            simulation.advance_tick().unwrap();
        }
        let after = simulation.storm_snapshot();

        assert_eq!(before.radius, INITIAL_STORM_RADIUS_UNITS);
        assert!(shrinking.damage_active);
        assert!(shrinking.radius < before.radius);
        assert!(after.radius <= shrinking.radius);
        assert!(after.radius >= FINAL_STORM_RADIUS_UNITS);
        assert_eq!(after.center, shrinking.center);
        assert_eq!(
            BattleRoyaleMatch::storm_center(BattleMatchId::new(11)),
            BattleRoyaleMatch::storm_center(BattleMatchId::new(11))
        );
    }

    #[test]
    fn session_retirement_is_terminal_only_after_match_start() {
        let mut lobby = BattleRoyaleMatch::new(BattleMatchId::new(13));
        lobby.add_player(1).unwrap();
        assert!(lobby.remove_player(1));
        assert_eq!(lobby.player_count(), 0);

        let mut active = BattleRoyaleMatch::new(BattleMatchId::new(14));
        start_combat(&mut active);
        assert!(active.remove_player(1));
        let snapshot = active.snapshot();
        let eliminated = snapshot
            .players
            .iter()
            .find(|player| player.player_id == 1)
            .unwrap();
        assert!(!eliminated.alive);
        assert_eq!(
            eliminated.elimination_cause,
            Some(EliminationCause::SessionExpired)
        );
        assert_eq!(active.phase(), MatchPhase::Finished);
        assert_eq!(active.winner(), Some(2));
    }

    #[test]
    fn player_projection_keeps_sequence_private_to_the_addressed_player() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(15));
        simulation.add_player(1).unwrap();
        simulation.add_player(2).unwrap();
        simulation
            .apply_command(1, 7, MatchCommand::SetMovement { x: 1, z: 0 })
            .unwrap();

        let canonical = simulation.snapshot();
        let projection = simulation.snapshot_for_player(1).unwrap();

        assert_eq!(canonical.players[0].last_sequence, 7);
        assert_eq!(projection.acknowledged_sequence, 7);
        assert_eq!(projection.players.len(), 2);
    }
}
