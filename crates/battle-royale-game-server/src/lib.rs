#![forbid(unsafe_code)]

use battle_royale_core::{BattleMatchId, BattleRoyaleMatch, MAX_PLAYERS_PER_MATCH, TICK_HZ};
use battle_royale_protocol::{decode_command, encode_canonical_snapshot, encode_snapshot};
use game_server::{
    GameSimulation, HostError, MatchHost, MatchId, MatchIdError, MatchRuntime, SimulationError,
    SimulationSnapshot, SnapshotScope,
};

pub const PINNED_GAME_SERVER_REVISION: &str = "769de47005cc37891011fc76ae183c18b7c5e0ae";
pub const PINNED_MMORPG_REVISION: &str = "81306faec8a6a7ea3b4279d75f763c1135ca4a19";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchHostBuildError {
    Empty,
    DuplicateMatch(BattleMatchId),
    MatchId(MatchIdError),
    Host(HostError),
}

impl std::fmt::Display for MatchHostBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("match host must contain at least one match"),
            Self::DuplicateMatch(match_id) => {
                write!(
                    formatter,
                    "match {} is configured more than once",
                    match_id.get()
                )
            }
            Self::MatchId(error) => error.fmt(formatter),
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MatchHostBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MatchId(error) => Some(error),
            Self::Host(error) => Some(error),
            Self::Empty | Self::DuplicateMatch(_) => None,
        }
    }
}

pub struct BattleRoyaleGameServerAdapter {
    simulation: BattleRoyaleMatch,
}

impl BattleRoyaleGameServerAdapter {
    #[must_use]
    pub fn new(match_id: BattleMatchId) -> Self {
        Self {
            simulation: BattleRoyaleMatch::new(match_id),
        }
    }

    #[must_use]
    pub fn simulation(&self) -> &BattleRoyaleMatch {
        &self.simulation
    }

    pub fn simulation_mut(&mut self) -> &mut BattleRoyaleMatch {
        &mut self.simulation
    }

    #[must_use]
    pub fn into_simulation(self) -> BattleRoyaleMatch {
        self.simulation
    }
}

pub fn route_match_id(match_id: BattleMatchId) -> Result<MatchId, MatchIdError> {
    MatchId::new(format!("br-{:016x}", match_id.get()))
}

pub fn build_match_entries(
    match_ids: impl IntoIterator<Item = BattleMatchId>,
) -> Result<Vec<(MatchId, BattleRoyaleGameServerAdapter)>, MatchHostBuildError> {
    let match_ids = match_ids.into_iter().collect::<Vec<_>>();
    validate_match_ids(&match_ids)?;

    match_ids
        .into_iter()
        .map(|match_id| {
            let route = route_match_id(match_id).map_err(MatchHostBuildError::MatchId)?;
            Ok((route, BattleRoyaleGameServerAdapter::new(match_id)))
        })
        .collect()
}

pub fn build_match_host(
    match_ids: impl IntoIterator<Item = BattleMatchId>,
    reconnect_grace_ticks: u64,
) -> Result<MatchHost<BattleRoyaleGameServerAdapter>, MatchHostBuildError> {
    let matches = build_match_entries(match_ids)?;
    let mut host = MatchHost::new(matches.len()).map_err(MatchHostBuildError::Host)?;
    for (route, simulation) in matches {
        let runtime = MatchRuntime::new(simulation, reconnect_grace_ticks);
        host.insert(route, runtime).map_err(|failure| {
            let (error, _, _) = failure.into_parts();
            MatchHostBuildError::Host(error)
        })?;
    }
    Ok(host)
}

fn validate_match_ids(match_ids: &[BattleMatchId]) -> Result<(), MatchHostBuildError> {
    if match_ids.is_empty() {
        return Err(MatchHostBuildError::Empty);
    }

    let mut ordered = match_ids.to_vec();
    ordered.sort_unstable();
    if let Some(duplicate) = ordered.windows(2).find_map(|pair| {
        let [left, right] = pair else {
            return None;
        };
        (left == right).then_some(*left)
    }) {
        return Err(MatchHostBuildError::DuplicateMatch(duplicate));
    }
    Ok(())
}

impl GameSimulation for BattleRoyaleGameServerAdapter {
    fn tick_hz(&self) -> u16 {
        TICK_HZ
    }

    fn max_players(&self) -> usize {
        MAX_PLAYERS_PER_MATCH
    }

    fn current_tick(&self) -> u64 {
        self.simulation.current_tick()
    }

    fn add_player(&mut self, player_id: game_server::PlayerId) -> Result<(), SimulationError> {
        self.simulation
            .add_player(player_id)
            .map_err(map_match_error)
    }

    fn remove_player(&mut self, player_id: game_server::PlayerId) -> bool {
        self.simulation.remove_player(player_id)
    }

    fn apply_command(
        &mut self,
        player_id: game_server::PlayerId,
        sequence: u32,
        payload: &[u8],
    ) -> Result<(), SimulationError> {
        let command = decode_command(payload).map_err(map_protocol_error)?;
        self.simulation
            .apply_command(player_id, sequence, command)
            .map_err(map_match_error)
    }

    fn advance_tick(&mut self) -> Result<(), SimulationError> {
        self.simulation.advance_tick().map_err(map_match_error)
    }

    fn snapshot_scope(&self) -> SnapshotScope {
        SnapshotScope::PlayerScoped
    }

    fn snapshot(&self) -> Result<SimulationSnapshot, SimulationError> {
        let snapshot = self.simulation.snapshot().map_err(map_match_error)?;
        Ok(SimulationSnapshot::new(
            snapshot.tick,
            encode_canonical_snapshot(&snapshot).map_err(map_protocol_error)?,
        ))
    }

    fn snapshot_for(
        &self,
        player_id: game_server::PlayerId,
    ) -> Result<SimulationSnapshot, SimulationError> {
        let snapshot = self
            .simulation
            .snapshot_for_player(player_id)
            .map_err(map_match_error)?;
        Ok(SimulationSnapshot::new(
            snapshot.tick,
            encode_snapshot(&snapshot).map_err(map_protocol_error)?,
        ))
    }
}

fn map_match_error(error: battle_royale_core::MatchError) -> SimulationError {
    SimulationError::new(error.to_string())
}

fn map_protocol_error(error: battle_royale_protocol::ProtocolError) -> SimulationError {
    SimulationError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use battle_royale_core::{
        DROP_DURATION_TICKS, LOBBY_COUNTDOWN_TICKS, MatchCommand, MatchPhase,
    };
    use battle_royale_protocol::{decode_canonical_snapshot, decode_snapshot, encode_command};
    use game_server::{RECONNECT_TOKEN_BYTES, ReconnectToken};

    fn token(value: u8) -> ReconnectToken {
        ReconnectToken([value; RECONNECT_TOKEN_BYTES])
    }

    #[test]
    fn route_ids_are_stable_and_url_safe() {
        assert_eq!(
            route_match_id(BattleMatchId::new(0x2a)).unwrap().as_str(),
            "br-000000000000002a"
        );
    }

    #[test]
    fn adapter_keeps_canonical_and_player_snapshots_separate() {
        let mut adapter = BattleRoyaleGameServerAdapter::new(BattleMatchId::new(7));
        adapter.add_player(1).unwrap();
        adapter.add_player(2).unwrap();
        let payload = encode_command(MatchCommand::SetMovement { x: 1, z: 0 });
        adapter.apply_command(1, 9, &payload).unwrap();

        let canonical = decode_canonical_snapshot(&adapter.snapshot().unwrap().payload).unwrap();
        let visible = decode_snapshot(&adapter.snapshot_for(1).unwrap().payload).unwrap();

        assert_eq!(adapter.snapshot_scope(), SnapshotScope::PlayerScoped);
        assert_eq!(canonical.players.len(), 2);
        assert_eq!(canonical.players[0].last_sequence, 9);
        assert_eq!(visible.acknowledged_sequence, 9);
        assert_eq!(visible.players.len(), 2);
    }

    #[test]
    fn shared_runtime_drives_match_authority_and_recovery() {
        let match_id = BattleMatchId::new(9);
        let old_token = token(7);
        let replacement_token = token(8);
        let mut runtime = MatchRuntime::new_with_replay_capture(
            BattleRoyaleGameServerAdapter::new(match_id),
            300,
        );
        let first = runtime.admit(old_token).unwrap();
        runtime.admit(token(2)).unwrap();

        let payload = encode_command(MatchCommand::SetMovement { x: 1, z: 0 });
        runtime
            .submit_command(first.player_id, first.connection_epoch, 5, &payload)
            .unwrap();

        for _ in 0..(LOBBY_COUNTDOWN_TICKS + DROP_DURATION_TICKS + 5) {
            runtime.advance_tick().unwrap();
        }
        assert!(runtime.disconnect(first.player_id, first.connection_epoch));

        runtime.freeze_for_recovery();
        let image = runtime.recovery_image().unwrap();
        let mut restored = MatchRuntime::restore_from_recovery(
            BattleRoyaleGameServerAdapter::new(match_id),
            image,
        )
        .unwrap();

        let canonical = decode_canonical_snapshot(&restored.snapshot().unwrap().payload).unwrap();
        let reconnected = restored.reconnect(old_token, replacement_token).unwrap();

        assert_eq!(canonical.phase, MatchPhase::Combat);
        assert_eq!(canonical.players[0].last_sequence, 5);
        assert!(canonical.zone.players[0].position[0] > -3_600.0);
        assert_eq!(reconnected.player_id, first.player_id);
        assert_eq!(reconnected.connection_epoch, first.connection_epoch + 1);
    }

    #[test]
    fn host_rejects_empty_and_duplicate_sets_and_keeps_matches_isolated() {
        assert!(matches!(
            build_match_entries([]),
            Err(MatchHostBuildError::Empty)
        ));
        assert!(matches!(
            build_match_host([], 120),
            Err(MatchHostBuildError::Empty)
        ));
        assert_eq!(
            build_match_host([BattleMatchId::new(4), BattleMatchId::new(4)], 120).err(),
            Some(MatchHostBuildError::DuplicateMatch(BattleMatchId::new(4)))
        );

        let first = BattleMatchId::new(10);
        let second = BattleMatchId::new(11);
        let entries = build_match_entries([first, second]).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, route_match_id(first).unwrap());
        assert_eq!(entries[1].0, route_match_id(second).unwrap());
        let first_route = route_match_id(first).unwrap();
        let second_route = route_match_id(second).unwrap();
        let mut host = build_match_host([first, second], 120).unwrap();

        host.with_runtime_mut(&first_route, |runtime| runtime.advance_tick())
            .unwrap()
            .unwrap();

        assert_eq!(host.runtime(&first_route).unwrap().current_tick(), 1);
        assert_eq!(host.runtime(&second_route).unwrap().current_tick(), 0);
    }
}
