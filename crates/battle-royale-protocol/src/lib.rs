#![forbid(unsafe_code)]

use battle_royale_core::{
    BattleMatchId, CanonicalMatchSnapshot, CanonicalPlayerSnapshot, EliminationCause,
    MAX_PLAYERS_PER_MATCH, MatchCommand, MatchPhase, MatchSnapshot, PlayerSnapshot, StormSnapshot,
};
use mmorpg_core::ZoneCommand;
use mmorpg_core::{
    ClassChoice, EQUIPMENT_SLOTS, EphemeralCharacterBuild, ItemId, PlayerClass, Sex,
};
pub use mmorpg_protocol::SheetRevisions;
use std::{error::Error, fmt};

pub const PROTOCOL_VERSION: u16 = 2;
const CANONICAL_MAGIC: [u8; 4] = *b"BRCA";
const PLAYER_MAGIC: [u8; 4] = *b"BRPL";
const MAX_COMPOSED_ZONE_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    message: String,
}
impl ProtocolError {
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
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for ProtocolError {}
fn shared_error(error: impl fmt::Display) -> ProtocolError {
    ProtocolError::new(error.to_string())
}

#[must_use]
pub fn encode_command(command: MatchCommand) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_u16(&mut bytes, PROTOCOL_VERSION);
    match command {
        MatchCommand::SetMovement { x, z } => {
            bytes.extend_from_slice(&[1, x as u8, z as u8]);
        }
        MatchCommand::SelectBuild(build) => {
            bytes.extend_from_slice(&[2, build.class().class.code(), build.class().sex.code()]);
            for slot in build.equipment().slots() {
                write_u16(&mut bytes, slot.map_or(0, ItemId::get));
            }
        }
        MatchCommand::Character(command) => {
            bytes.push(3);
            bytes.extend_from_slice(&mmorpg_protocol::encode_command(command));
        }
    }
    bytes
}
/// A command datagram as the host reads it: the command and, for a character command the
/// client wrapped with [`mmorpg_protocol::encode_acknowledged_command`], the sheet revisions
/// it holds (mmorpg #544).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandEnvelope {
    pub command: MatchCommand,
    pub sheets: Option<SheetRevisions>,
}

/// A character command wrapped with the sheet revisions the client holds.
#[must_use]
pub fn encode_acknowledged_command(sheets: SheetRevisions, command: ZoneCommand) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_u16(&mut bytes, PROTOCOL_VERSION);
    bytes.push(3);
    bytes.extend_from_slice(&mmorpg_protocol::encode_acknowledged_command(
        sheets,
        &mmorpg_protocol::encode_command(command),
    ));
    bytes
}

pub fn decode_command(payload: &[u8]) -> Result<CommandEnvelope, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    if cursor.read_u16()? != PROTOCOL_VERSION {
        return Err(ProtocolError::new("unsupported command protocol version"));
    }
    let mut sheets = None;
    let command = match cursor.read_u8()? {
        1 => {
            let x = cursor.read_u8()? as i8;
            let z = cursor.read_u8()? as i8;
            if !(-1..=1).contains(&x) || !(-1..=1).contains(&z) {
                return Err(ProtocolError::new(
                    "movement components must be between -1 and 1",
                ));
            }
            MatchCommand::SetMovement { x, z }
        }
        2 => {
            let class = PlayerClass::from_code(cursor.read_u8()?)
                .ok_or_else(|| ProtocolError::new("unknown class"))?;
            let sex = Sex::from_code(cursor.read_u8()?)
                .ok_or_else(|| ProtocolError::new("unknown sex"))?;
            let mut slots = [None; EQUIPMENT_SLOTS];
            for slot in &mut slots {
                let id = cursor.read_u16()?;
                *slot = (id != 0).then_some(ItemId::new(id));
            }
            MatchCommand::SelectBuild(
                EphemeralCharacterBuild::new(ClassChoice { class, sex }, slots)
                    .map_err(shared_error)?,
            )
        }
        3 => {
            let remaining = cursor.bytes.len() - cursor.offset;
            let envelope = mmorpg_protocol::decode_command_envelope(cursor.take(remaining)?)
                .map_err(shared_error)?;
            // The mode judges every intent on arrival; an observed tick (#415) is not used.
            sheets = envelope.sheets;
            MatchCommand::Character(envelope.command)
        }
        _ => return Err(ProtocolError::new("unsupported command kind")),
    };
    cursor.finish()?;
    Ok(CommandEnvelope { command, sheets })
}

pub fn encode_canonical_snapshot(
    snapshot: &CanonicalMatchSnapshot,
) -> Result<Vec<u8>, ProtocolError> {
    let zone = mmorpg_protocol::encode_canonical_snapshot(&snapshot.zone).map_err(shared_error)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&CANONICAL_MAGIC);
    write_common_header(&mut bytes, CommonHeader::from(snapshot));
    validate_player_count(snapshot.players.len())?;
    write_u16(&mut bytes, snapshot.players.len() as u16);
    for player in &snapshot.players {
        write_u32(&mut bytes, player.player_id);
        write_u32(&mut bytes, player.last_sequence);
        write_option_u64(&mut bytes, player.elimination_tick);
        bytes.push(encode_cause(player.elimination_cause));
    }
    write_zone(&mut bytes, &zone)?;
    Ok(bytes)
}
pub fn decode_canonical_snapshot(payload: &[u8]) -> Result<CanonicalMatchSnapshot, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    cursor.expect_magic(CANONICAL_MAGIC)?;
    let h = read_common_header(&mut cursor)?;
    let count = usize::from(cursor.read_u16()?);
    validate_player_count(count)?;
    let mut players = Vec::with_capacity(count);
    for _ in 0..count {
        players.push(CanonicalPlayerSnapshot {
            player_id: cursor.read_u32()?,
            last_sequence: cursor.read_u32()?,
            elimination_tick: cursor.read_option_u64()?,
            elimination_cause: decode_cause(cursor.read_u8()?)?,
        });
    }
    let zone = mmorpg_protocol::decode_canonical_snapshot(read_zone(&mut cursor)?)
        .map_err(shared_error)?;
    cursor.finish()?;
    Ok(CanonicalMatchSnapshot {
        schema_version: h.schema_version,
        ruleset_revision: h.ruleset_revision,
        match_id: h.match_id,
        tick: h.tick,
        phase: h.phase,
        phase_started_tick: h.phase_started_tick,
        combat_started_tick: h.combat_started_tick,
        lobby_ready_tick: h.lobby_ready_tick,
        storm: h.storm,
        winner: h.winner,
        players,
        zone,
    })
}
/// The player projection, carrying the zone sheets whose revision differs from the
/// client's `acknowledged` ones (mmorpg #544). The zone section is budget-packed: entities
/// beyond one datagram are omitted by relevance priority.
pub fn encode_snapshot(
    snapshot: &MatchSnapshot,
    acknowledged: SheetRevisions,
) -> Result<Vec<u8>, ProtocolError> {
    let zone = mmorpg_protocol::pack_snapshot_acknowledged(&snapshot.zone, acknowledged)
        .map_err(shared_error)?
        .payload;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&PLAYER_MAGIC);
    write_common_header(&mut bytes, CommonHeader::from(snapshot));
    write_u32(&mut bytes, snapshot.acknowledged_sequence);
    validate_player_count(snapshot.players.len())?;
    write_u16(&mut bytes, snapshot.players.len() as u16);
    for player in &snapshot.players {
        write_u32(&mut bytes, player.player_id);
        bytes.push(u8::from(player.alive));
        write_option_u64(&mut bytes, player.elimination_tick);
        bytes.push(encode_cause(player.elimination_cause));
    }
    write_zone(&mut bytes, &zone)?;
    Ok(bytes)
}
pub fn decode_snapshot(payload: &[u8]) -> Result<MatchSnapshot, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    cursor.expect_magic(PLAYER_MAGIC)?;
    let h = read_common_header(&mut cursor)?;
    let acknowledged_sequence = cursor.read_u32()?;
    let count = usize::from(cursor.read_u16()?);
    validate_player_count(count)?;
    let mut players = Vec::with_capacity(count);
    for _ in 0..count {
        players.push(PlayerSnapshot {
            player_id: cursor.read_u32()?,
            alive: cursor.read_bool()?,
            elimination_tick: cursor.read_option_u64()?,
            elimination_cause: decode_cause(cursor.read_u8()?)?,
        });
    }
    let zone = mmorpg_protocol::decode_snapshot(read_zone(&mut cursor)?).map_err(shared_error)?;
    cursor.finish()?;
    Ok(MatchSnapshot {
        schema_version: h.schema_version,
        ruleset_revision: h.ruleset_revision,
        match_id: h.match_id,
        tick: h.tick,
        phase: h.phase,
        phase_started_tick: h.phase_started_tick,
        combat_started_tick: h.combat_started_tick,
        lobby_ready_tick: h.lobby_ready_tick,
        storm: h.storm,
        winner: h.winner,
        acknowledged_sequence,
        players,
        zone,
    })
}
fn write_zone(bytes: &mut Vec<u8>, zone: &[u8]) -> Result<(), ProtocolError> {
    if zone.len() > MAX_COMPOSED_ZONE_BYTES {
        return Err(ProtocolError::new("composed zone payload is too large"));
    }
    write_u32(bytes, zone.len() as u32);
    bytes.extend_from_slice(zone);
    Ok(())
}
fn read_zone<'a>(cursor: &mut Cursor<'a>) -> Result<&'a [u8], ProtocolError> {
    let len = cursor.read_u32()? as usize;
    if len > MAX_COMPOSED_ZONE_BYTES {
        return Err(ProtocolError::new("composed zone payload is too large"));
    }
    cursor.take(len)
}
#[derive(Clone, Copy)]
struct CommonHeader {
    schema_version: u16,
    ruleset_revision: u64,
    match_id: BattleMatchId,
    tick: u64,
    phase: MatchPhase,
    phase_started_tick: u64,
    combat_started_tick: Option<u64>,
    lobby_ready_tick: Option<u64>,
    storm: StormSnapshot,
    winner: Option<u32>,
}

impl From<&CanonicalMatchSnapshot> for CommonHeader {
    fn from(snapshot: &CanonicalMatchSnapshot) -> Self {
        Self {
            schema_version: snapshot.schema_version,
            ruleset_revision: snapshot.ruleset_revision,
            match_id: snapshot.match_id,
            tick: snapshot.tick,
            phase: snapshot.phase,
            phase_started_tick: snapshot.phase_started_tick,
            combat_started_tick: snapshot.combat_started_tick,
            lobby_ready_tick: snapshot.lobby_ready_tick,
            storm: snapshot.storm,
            winner: snapshot.winner,
        }
    }
}

impl From<&MatchSnapshot> for CommonHeader {
    fn from(snapshot: &MatchSnapshot) -> Self {
        Self {
            schema_version: snapshot.schema_version,
            ruleset_revision: snapshot.ruleset_revision,
            match_id: snapshot.match_id,
            tick: snapshot.tick,
            phase: snapshot.phase,
            phase_started_tick: snapshot.phase_started_tick,
            combat_started_tick: snapshot.combat_started_tick,
            lobby_ready_tick: snapshot.lobby_ready_tick,
            storm: snapshot.storm,
            winner: snapshot.winner,
        }
    }
}

fn write_common_header(bytes: &mut Vec<u8>, header: CommonHeader) {
    write_u16(bytes, PROTOCOL_VERSION);
    write_u16(bytes, header.schema_version);
    write_u64(bytes, header.ruleset_revision);
    write_u64(bytes, header.match_id.get());
    write_u64(bytes, header.tick);
    bytes.push(encode_phase(header.phase));
    write_u64(bytes, header.phase_started_tick);
    write_option_u64(bytes, header.combat_started_tick);
    write_option_u64(bytes, header.lobby_ready_tick);
    write_i32(bytes, header.storm.center[0]);
    write_i32(bytes, header.storm.center[1]);
    write_i32(bytes, header.storm.radius);
    bytes.push(u8::from(header.storm.damage_active));
    write_option_u32(bytes, header.winner);
}

fn read_common_header(cursor: &mut Cursor<'_>) -> Result<CommonHeader, ProtocolError> {
    let protocol_version = cursor.read_u16()?;
    if protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::new("unsupported snapshot protocol version"));
    }
    Ok(CommonHeader {
        schema_version: cursor.read_u16()?,
        ruleset_revision: cursor.read_u64()?,
        match_id: BattleMatchId::new(cursor.read_u64()?),
        tick: cursor.read_u64()?,
        phase: decode_phase(cursor.read_u8()?)?,
        phase_started_tick: cursor.read_u64()?,
        combat_started_tick: cursor.read_option_u64()?,
        lobby_ready_tick: cursor.read_option_u64()?,
        storm: StormSnapshot {
            center: [cursor.read_i32()?, cursor.read_i32()?],
            radius: cursor.read_i32()?,
            damage_active: cursor.read_bool()?,
        },
        winner: cursor.read_option_u32()?,
    })
}

fn validate_player_count(count: usize) -> Result<(), ProtocolError> {
    if count > MAX_PLAYERS_PER_MATCH {
        return Err(ProtocolError::new(
            "snapshot player count exceeds match capacity",
        ));
    }
    Ok(())
}

fn encode_phase(phase: MatchPhase) -> u8 {
    match phase {
        MatchPhase::Waiting => 0,
        MatchPhase::Drop => 1,
        MatchPhase::Combat => 2,
        MatchPhase::Finished => 3,
    }
}

fn decode_phase(value: u8) -> Result<MatchPhase, ProtocolError> {
    match value {
        0 => Ok(MatchPhase::Waiting),
        1 => Ok(MatchPhase::Drop),
        2 => Ok(MatchPhase::Combat),
        3 => Ok(MatchPhase::Finished),
        _ => Err(ProtocolError::new("invalid match phase")),
    }
}

fn encode_cause(cause: Option<EliminationCause>) -> u8 {
    match cause {
        None => 0,
        Some(EliminationCause::Storm) => 1,
        Some(EliminationCause::SessionExpired) => 2,
        Some(EliminationCause::Combat) => 3,
    }
}

fn decode_cause(value: u8) -> Result<Option<EliminationCause>, ProtocolError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(EliminationCause::Storm)),
        2 => Ok(Some(EliminationCause::SessionExpired)),
        3 => Ok(Some(EliminationCause::Combat)),
        _ => Err(ProtocolError::new("invalid elimination cause")),
    }
}

fn write_option_u64(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            bytes.push(1);
            write_u64(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn write_option_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            bytes.push(1);
            write_u32(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn expect_magic(&mut self, expected: [u8; 4]) -> Result<(), ProtocolError> {
        if self.take(4)? != expected {
            return Err(ProtocolError::new("invalid snapshot magic"));
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn read_bool(&mut self) -> Result<bool, ProtocolError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::new("invalid boolean value")),
        }
    }

    fn read_u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, ProtocolError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_i32(&mut self) -> Result<i32, ProtocolError> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_option_u64(&mut self) -> Result<Option<u64>, ProtocolError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_u64()?)),
            _ => Err(ProtocolError::new("invalid optional u64 tag")),
        }
    }

    fn read_option_u32(&mut self) -> Result<Option<u32>, ProtocolError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_u32()?)),
            _ => Err(ProtocolError::new("invalid optional u32 tag")),
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| ProtocolError::new("snapshot offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| ProtocolError::new("truncated snapshot"))?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), ProtocolError> {
        if self.offset != self.bytes.len() {
            return Err(ProtocolError::new("snapshot has trailing bytes"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use battle_royale_core::BattleRoyaleMatch;
    #[test]
    fn command_envelopes_preserve_shared_intents_and_validate_builds() {
        let build = EphemeralCharacterBuild::standard(ClassChoice {
            class: PlayerClass::Arcanist,
            sex: Sex::Female,
        })
        .unwrap();
        for command in [
            MatchCommand::SetMovement { x: -1, z: 1 },
            MatchCommand::SelectBuild(build),
            MatchCommand::Character(ZoneCommand::UseAbility {
                ability: 9,
                target: Some(mmorpg_core::EntityRef::Player(2)),
            }),
        ] {
            let encoded = encode_command(command);
            assert_eq!(
                decode_command(&encoded).unwrap(),
                CommandEnvelope {
                    command,
                    sheets: None
                }
            );
            let mut trailing = encoded.clone();
            trailing.push(0);
            assert!(decode_command(&trailing).is_err());
        }
        let mut invalid = encode_command(MatchCommand::SelectBuild(build));
        invalid[5..7].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(decode_command(&invalid).is_err());
        assert!(
            decode_command(&[1, 0, 1, 0, 0]).is_err(),
            "old commands fail closed"
        );
    }
    #[test]
    fn character_commands_carry_the_sheet_revisions_the_client_holds() {
        let sheets = SheetRevisions {
            inventory: 3,
            quests: 0,
            loot: 1,
            abilities: 7,
        };
        let command = ZoneCommand::UseAbility {
            ability: 9,
            target: Some(mmorpg_core::EntityRef::Player(2)),
        };
        let encoded = encode_acknowledged_command(sheets, command);
        assert_eq!(
            decode_command(&encoded).unwrap(),
            CommandEnvelope {
                command: MatchCommand::Character(command),
                sheets: Some(sheets)
            }
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_command(&trailing).is_err());
    }
    #[test]
    fn projections_leave_out_the_sheets_a_client_acknowledged() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(33));
        simulation.add_player(1).unwrap();
        simulation.add_player(2).unwrap();
        let visible = simulation.snapshot_for_player(1).unwrap();
        let mut decoder = mmorpg_protocol::SnapshotDecoder::new();
        decoder
            .decode(
                &mmorpg_protocol::pack_snapshot(&visible.zone)
                    .unwrap()
                    .payload,
            )
            .unwrap();
        let held = decoder.revisions();
        assert_ne!(held, SheetRevisions::default());
        let full = encode_snapshot(&visible, SheetRevisions::default()).unwrap();
        let quiet = encode_snapshot(&visible, held).unwrap();
        assert!(quiet.len() < full.len());
        assert_eq!(decode_snapshot(&quiet).unwrap().players, visible.players);
    }
    #[test]
    fn complete_zone_and_player_projection_roundtrip_separately() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(31));
        simulation.add_player(1).unwrap();
        simulation.add_player(2).unwrap();
        simulation
            .apply_command(1, 4, MatchCommand::SetMovement { x: 1, z: 0 })
            .unwrap();
        simulation.advance_tick().unwrap();
        let canonical = simulation.snapshot().unwrap();
        let visible = simulation.snapshot_for_player(1).unwrap();
        assert_eq!(
            decode_canonical_snapshot(&encode_canonical_snapshot(&canonical).unwrap()).unwrap(),
            canonical
        );
        let encoded = encode_snapshot(&visible, SheetRevisions::default()).unwrap();
        assert_eq!(decode_snapshot(&encoded).unwrap(), visible);
        assert!(decode_canonical_snapshot(&encoded).is_err());
        assert!(decode_snapshot(&encode_canonical_snapshot(&canonical).unwrap()).is_err());
    }
    #[test]
    fn malformed_snapshot_fails_closed() {
        let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(32));
        simulation.add_player(1).unwrap();
        let mut encoded = encode_canonical_snapshot(&simulation.snapshot().unwrap()).unwrap();
        encoded.push(9);
        assert_eq!(
            decode_canonical_snapshot(&encoded).unwrap_err().message(),
            "snapshot has trailing bytes"
        );
        encoded.truncate(12);
        assert!(decode_canonical_snapshot(&encoded).is_err());
    }
}
