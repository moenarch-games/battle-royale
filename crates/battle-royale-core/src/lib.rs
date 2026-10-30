#![forbid(unsafe_code)]

pub mod arena;

use mmorpg_core::{CanonicalZoneSnapshot, ZoneId, ZoneSimulation, ZoneSnapshot};
pub use mmorpg_core::{
    ClassChoice, EntityRef, EphemeralCharacterBuild, PlayerClass, PlayerId, Sex, ZoneCommand,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

pub const TICK_HZ: u16 = mmorpg_core::TICK_HZ;
pub const MAX_PLAYERS_PER_MATCH: usize = 100;
pub const MIN_PLAYERS_TO_START: usize = 2;
pub const LOBBY_COUNTDOWN_TICKS: u64 = 60;
pub const DROP_DURATION_TICKS: u64 = 90;
pub const STORM_DELAY_TICKS: u64 = 90;
pub const STORM_SHRINK_DURATION_TICKS: u64 = 900;
pub const INITIAL_STORM_RADIUS_UNITS: i32 = 50_000;
pub const FINAL_STORM_RADIUS_UNITS: i32 = 2_000;
pub const STORM_DAMAGE_PER_TICK: u32 = 2;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 2;
pub const RULESET_REVISION: u64 = 2;
const ARENA_ZONE: ZoneId = ZoneId::new(100);

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
    Combat,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchCommand {
    /// Compatibility adapter for the existing semantic mobile movement action.
    SetMovement { x: i8, z: i8 },
    /// Lobby setup only. Once the lobby closes, the current life is immutable.
    SelectBuild(EphemeralCharacterBuild),
    /// The shared MMORPG command; the mode admits only combat/movement/social intents.
    Character(ZoneCommand),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StormSnapshot {
    pub center: [i32; 2],
    pub radius: i32,
    pub damage_active: bool,
}

/// Mode-only roster information. Physical/combat state lives exclusively in `zone`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub player_id: PlayerId,
    pub alive: bool,
    pub elimination_tick: Option<u64>,
    pub elimination_cause: Option<EliminationCause>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalPlayerSnapshot {
    pub player_id: PlayerId,
    pub last_sequence: u32,
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
    /// The unmodified shared MMORPG player projection, ready for its existing renderer.
    pub zone: ZoneSnapshot,
}
#[derive(Clone, Debug, PartialEq)]
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
    pub zone: CanonicalZoneSnapshot,
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for MatchError {}
impl From<mmorpg_core::ZoneError> for MatchError {
    fn from(error: mmorpg_core::ZoneError) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Clone, Copy)]
struct PlayerState {
    last_sequence: u32,
    elimination_tick: Option<u64>,
    elimination_cause: Option<EliminationCause>,
}
impl PlayerState {
    fn alive(self) -> bool {
        self.elimination_tick.is_none()
    }
    fn visible(self, player_id: PlayerId) -> PlayerSnapshot {
        PlayerSnapshot {
            player_id,
            alive: self.alive(),
            elimination_tick: self.elimination_tick,
            elimination_cause: self.elimination_cause,
        }
    }
}

pub struct BattleRoyaleMatch {
    match_id: BattleMatchId,
    tick: u64,
    phase: MatchPhase,
    phase_started_tick: u64,
    combat_started_tick: Option<u64>,
    lobby_ready_tick: Option<u64>,
    zone: ZoneSimulation,
    players: BTreeMap<PlayerId, PlayerState>,
    winner: Option<PlayerId>,
}
impl BattleRoyaleMatch {
    #[must_use]
    pub fn new(match_id: BattleMatchId) -> Self {
        // Fixed authored content is validated once; failure is a program initialization defect.
        let zone = ZoneSimulation::with_content(
            ARENA_ZONE,
            arena::content().expect("valid authored arena"),
        )
        .expect("valid authored arena simulation");
        Self {
            match_id,
            tick: 0,
            phase: MatchPhase::Waiting,
            phase_started_tick: 0,
            combat_started_tick: None,
            lobby_ready_tick: None,
            zone,
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
        self.players.values().filter(|p| p.alive()).count()
    }
    #[must_use]
    pub const fn winner(&self) -> Option<PlayerId> {
        self.winner
    }

    pub fn add_player(&mut self, player_id: PlayerId) -> Result<(), MatchError> {
        let build = EphemeralCharacterBuild::standard(ClassChoice {
            class: PlayerClass::Warden,
            sex: Sex::Male,
        })
        .map_err(|e| MatchError::new(e.to_string()))?;
        self.add_player_with_build(player_id, &build)
    }
    /// Host admission can supply a selected build before spawning the character.
    /// The transport's default guest may instead select it during lobby setup.
    pub fn add_player_with_build(
        &mut self,
        player_id: PlayerId,
        build: &EphemeralCharacterBuild,
    ) -> Result<(), MatchError> {
        if self.phase != MatchPhase::Waiting {
            return Err(MatchError::new("match admission is closed"));
        }
        if self.players.contains_key(&player_id) {
            return Err(MatchError::new("player already exists in match"));
        }
        if self.players.len() >= MAX_PLAYERS_PER_MATCH {
            return Err(MatchError::new("match player capacity reached"));
        }
        self.zone.admit_ephemeral(player_id, build)?;
        self.players.insert(
            player_id,
            PlayerState {
                last_sequence: 0,
                elimination_tick: None,
                elimination_cause: None,
            },
        );
        self.refresh_lobby_ready_tick();
        Ok(())
    }
    pub fn remove_player(&mut self, player_id: PlayerId) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        if self.phase == MatchPhase::Finished {
            return true;
        }
        if self.phase == MatchPhase::Waiting {
            self.players.remove(&player_id);
            self.zone.remove_player(player_id);
            self.refresh_lobby_ready_tick();
        } else if player.alive() {
            self.eliminate_player(player_id, EliminationCause::SessionExpired);
            self.zone.remove_player(player_id);
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
            .get(&player_id)
            .ok_or_else(|| MatchError::new("unknown player"))?;
        if !player.alive() {
            return Err(MatchError::new("player is eliminated"));
        }
        if sequence == 0 || sequence <= player.last_sequence {
            return Err(MatchError::new("command sequence is stale"));
        }
        match command {
            MatchCommand::SelectBuild(build) => {
                if self.phase != MatchPhase::Waiting {
                    return Err(MatchError::new("loadouts are locked for this life"));
                }
                // Prepare replacement before committing: a failed setup never retires the guest.
                let mut prepared =
                    ZoneSimulation::from_snapshot(self.zone.snapshot()?, arena::content()?)?;
                prepared.remove_player(player_id);
                prepared.admit_ephemeral(player_id, &build)?;
                self.zone = prepared;
            }
            MatchCommand::SetMovement { x, z } => {
                if !(-1..=1).contains(&x) || !(-1..=1).contains(&z) {
                    return Err(MatchError::new(
                        "movement components must be between -1 and 1",
                    ));
                }
                self.zone.apply_command(
                    player_id,
                    sequence,
                    ZoneCommand::Move {
                        forward: z,
                        strafe: -x,
                        facing: 0,
                    },
                )?;
            }
            MatchCommand::Character(command) => {
                let movement_or_social = matches!(
                    command,
                    ZoneCommand::Move { .. }
                        | ZoneCommand::Jump
                        | ZoneCommand::Roll
                        | ZoneCommand::SelectTarget(_)
                        | ZoneCommand::Chat { .. }
                        | ZoneCommand::Emote(_)
                );
                let combat = matches!(
                    command,
                    ZoneCommand::Swing { .. }
                        | ZoneCommand::UseAbility { .. }
                        | ZoneCommand::ReleaseAbility { .. }
                        | ZoneCommand::DirectedStrike { .. }
                        | ZoneCommand::Guard { .. }
                        | ZoneCommand::CancelCast
                );
                if !movement_or_social && !(combat && self.phase == MatchPhase::Combat) {
                    return Err(MatchError::new(
                        "character command is unavailable in this match phase",
                    ));
                }
                self.zone.apply_command(player_id, sequence, command)?;
            }
        }
        self.players
            .get_mut(&player_id)
            .expect("validated player")
            .last_sequence = sequence;
        Ok(())
    }
    pub fn advance_tick(&mut self) -> Result<(), MatchError> {
        let next = self
            .tick
            .checked_add(1)
            .ok_or_else(|| MatchError::new("match tick overflow"))?;
        if self.phase != MatchPhase::Finished {
            self.zone.advance_tick()?;
        }
        self.tick = next;
        if self.phase == MatchPhase::Combat {
            // Observe actual shared combat deaths before applying this tick's storm.
            for player in self.zone.snapshot()?.players {
                if player.combat.health == 0 {
                    self.eliminate_player(player.player_id, EliminationCause::Combat);
                }
            }
        }
        match self.phase {
            MatchPhase::Waiting => {
                self.refresh_lobby_ready_tick();
                if self.players.len() == MAX_PLAYERS_PER_MATCH
                    || self
                        .lobby_ready_tick
                        .is_some_and(|t| self.tick.saturating_sub(t) >= LOBBY_COUNTDOWN_TICKS)
                {
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
                self.apply_storm_damage()?;
                self.finish_if_decided();
            }
            MatchPhase::Finished => {}
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<CanonicalMatchSnapshot, MatchError> {
        Ok(CanonicalMatchSnapshot {
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
                .map(|(&id, p)| CanonicalPlayerSnapshot {
                    player_id: id,
                    last_sequence: p.last_sequence,
                    elimination_tick: p.elimination_tick,
                    elimination_cause: p.elimination_cause,
                })
                .collect(),
            zone: self.zone.snapshot()?,
        })
    }
    pub fn snapshot_for_player(&self, player_id: PlayerId) -> Result<MatchSnapshot, MatchError> {
        let player = self
            .players
            .get(&player_id)
            .ok_or_else(|| MatchError::new("unknown player"))?;
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
            acknowledged_sequence: player.last_sequence,
            players: self.players.iter().map(|(&id, p)| p.visible(id)).collect(),
            zone: self.zone.snapshot_for_player(player_id)?,
        })
    }
    pub fn from_snapshot(snapshot: CanonicalMatchSnapshot) -> Result<Self, MatchError> {
        if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION
            || snapshot.ruleset_revision != RULESET_REVISION
        {
            return Err(MatchError::new("unsupported match snapshot version"));
        }
        if snapshot.players.len() > MAX_PLAYERS_PER_MATCH
            || snapshot
                .players
                .windows(2)
                .any(|p| p[0].player_id >= p[1].player_id)
        {
            return Err(MatchError::new("invalid match roster"));
        }
        if snapshot.phase_started_tick > snapshot.tick
            || snapshot.zone.zone_id != ARENA_ZONE
            || snapshot.zone.tick > snapshot.tick
            || (snapshot.phase != MatchPhase::Finished && snapshot.zone.tick != snapshot.tick)
        {
            return Err(MatchError::new("invalid match/zone clock or identity"));
        }
        let active = matches!(snapshot.phase, MatchPhase::Combat | MatchPhase::Finished);
        if active != snapshot.combat_started_tick.is_some()
            || snapshot
                .combat_started_tick
                .is_some_and(|t| t > snapshot.phase_started_tick)
            || snapshot
                .lobby_ready_tick
                .is_some_and(|t| snapshot.phase != MatchPhase::Waiting || t > snapshot.tick)
        {
            return Err(MatchError::new("invalid match phase clocks"));
        }
        let mut players = BTreeMap::new();
        for p in &snapshot.players {
            if p.elimination_tick.is_some() != p.elimination_cause.is_some()
                || p.elimination_tick
                    .is_some_and(|t| t > snapshot.tick || snapshot.phase == MatchPhase::Waiting)
            {
                return Err(MatchError::new("invalid match elimination"));
            }
            let character = snapshot
                .zone
                .players
                .iter()
                .find(|z| z.player_id == p.player_id);
            if p.elimination_cause == Some(EliminationCause::SessionExpired) {
                if character.is_some() {
                    return Err(MatchError::new("retired character remains in zone"));
                }
            } else {
                let z = character
                    .ok_or_else(|| MatchError::new("match character missing from zone"))?;
                if (z.combat.health > 0) != p.elimination_tick.is_none()
                    || z.last_sequence > p.last_sequence
                    || z.combat.level != mmorpg_core::MAX_PLAYER_LEVEL
                    || z.combat.abilities.class.is_none()
                {
                    return Err(MatchError::new(
                        "match roster disagrees with shared character state",
                    ));
                }
            }
            players.insert(
                p.player_id,
                PlayerState {
                    last_sequence: p.last_sequence,
                    elimination_tick: p.elimination_tick,
                    elimination_cause: p.elimination_cause,
                },
            );
        }
        if snapshot
            .zone
            .players
            .iter()
            .any(|p| !players.contains_key(&p.player_id))
        {
            return Err(MatchError::new(
                "zone character is absent from match roster",
            ));
        }
        let expected_storm = snapshot.storm;
        let zone = ZoneSimulation::from_snapshot(snapshot.zone, arena::content()?)?;
        let result = Self {
            match_id: snapshot.match_id,
            tick: snapshot.tick,
            phase: snapshot.phase,
            phase_started_tick: snapshot.phase_started_tick,
            combat_started_tick: snapshot.combat_started_tick,
            lobby_ready_tick: snapshot.lobby_ready_tick,
            zone,
            players,
            winner: snapshot.winner,
        };
        if result.storm_snapshot() != expected_storm {
            return Err(MatchError::new("match storm disagrees with its clock"));
        }
        let survivor = result
            .players
            .iter()
            .find_map(|(&id, p)| p.alive().then_some(id));
        if (result.phase == MatchPhase::Finished
            && (result.alive_count() > 1 || result.winner != survivor))
            || (result.phase != MatchPhase::Finished && result.winner.is_some())
        {
            return Err(MatchError::new("invalid match winner"));
        }
        Ok(result)
    }
    fn apply_storm_damage(&mut self) -> Result<(), MatchError> {
        let storm = self.storm_snapshot();
        if !storm.damage_active {
            return Ok(());
        }
        let radius = i128::from(storm.radius);
        for p in self.zone.snapshot()?.players {
            if !self.players.get(&p.player_id).is_some_and(|p| p.alive()) {
                continue;
            }
            let position = mmorpg_core::physics::to_units(p.position);
            let dx = i128::from(position[0]) - i128::from(storm.center[0]);
            let dz = i128::from(position[2]) - i128::from(storm.center[1]);
            if dx * dx + dz * dz > radius * radius {
                let remaining = p.combat.health;
                let dealt = self
                    .zone
                    .apply_environmental_damage(p.player_id, STORM_DAMAGE_PER_TICK)?;
                if dealt == remaining {
                    self.eliminate_player(p.player_id, EliminationCause::Storm);
                }
            }
        }
        Ok(())
    }
    fn eliminate_player(&mut self, id: PlayerId, cause: EliminationCause) {
        if let Some(p) = self.players.get_mut(&id)
            && p.alive()
        {
            p.elimination_tick = Some(self.tick);
            p.elimination_cause = Some(cause);
        }
    }
    fn finish_if_decided(&mut self) {
        if self.phase != MatchPhase::Combat || self.alive_count() > 1 {
            return;
        }
        self.winner = self
            .players
            .iter()
            .find_map(|(&id, p)| p.alive().then_some(id));
        self.start_phase(MatchPhase::Finished);
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
}
