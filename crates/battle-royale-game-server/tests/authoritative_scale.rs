use battle_royale_core::{BattleMatchId, MAX_PLAYERS_PER_MATCH, MatchCommand};
use battle_royale_game_server::BattleRoyaleGameServerAdapter;
use battle_royale_protocol::{decode_canonical_snapshot, decode_snapshot, encode_command};
use game_server::GameSimulation;

// Format 2 includes the complete shared MMORPG zone (24,040 bytes at fresh capacity),
// replacing the old 8 KiB baseline for the retired integer-only player records.
const CANONICAL_SNAPSHOT_BUDGET_BYTES: usize = 32 * 1024;
const PLAYER_SNAPSHOT_BUDGET_BYTES: usize = 4 * 1024;
const DETERMINISM_TICKS: usize = 512;

fn full_match(match_id: BattleMatchId) -> BattleRoyaleGameServerAdapter {
    let mut simulation = BattleRoyaleGameServerAdapter::new(match_id);

    for player_id in 1..=u32::try_from(MAX_PLAYERS_PER_MATCH).unwrap() {
        simulation.add_player(player_id).unwrap();
        let x = match player_id % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        };
        let z = match (player_id / 3) % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        };
        simulation
            .apply_command(
                player_id,
                1,
                &encode_command(MatchCommand::SetMovement { x, z }),
            )
            .unwrap();
    }

    simulation
}

#[test]
fn full_capacity_is_real_and_fails_closed_above_one_hundred_players() {
    let mut simulation = full_match(BattleMatchId::new(0x100));

    assert_eq!(simulation.max_players(), MAX_PLAYERS_PER_MATCH);
    assert_eq!(
        decode_canonical_snapshot(&simulation.snapshot().unwrap().payload)
            .unwrap()
            .players
            .len(),
        MAX_PLAYERS_PER_MATCH
    );

    let overflow_player = u32::try_from(MAX_PLAYERS_PER_MATCH).unwrap() + 1;
    assert!(simulation.add_player(overflow_player).is_err());
}

#[test]
fn full_match_snapshot_contracts_stay_bounded_without_hiding_players() {
    let simulation = full_match(BattleMatchId::new(0x200));

    let canonical = simulation.snapshot().unwrap();
    let addressed = simulation.snapshot_for(1).unwrap();
    let decoded = decode_snapshot(&addressed.payload).unwrap();

    assert_eq!(decoded.players.len(), MAX_PLAYERS_PER_MATCH);
    assert!(
        canonical.payload.len() <= CANONICAL_SNAPSHOT_BUDGET_BYTES,
        "canonical snapshot grew to {} bytes",
        canonical.payload.len()
    );
    assert!(
        addressed.payload.len() <= PLAYER_SNAPSHOT_BUDGET_BYTES,
        "player snapshot grew to {} bytes",
        addressed.payload.len()
    );
}

#[test]
fn full_capacity_workload_is_byte_deterministic_across_independent_simulations() {
    let match_id = BattleMatchId::new(0x300);
    let mut left = full_match(match_id);
    let mut right = full_match(match_id);

    for _ in 0..DETERMINISM_TICKS {
        left.advance_tick().unwrap();
        right.advance_tick().unwrap();
    }

    let left_snapshot = left.snapshot().unwrap();
    let right_snapshot = right.snapshot().unwrap();

    assert_eq!(left.current_tick(), DETERMINISM_TICKS as u64);
    assert_eq!(left_snapshot.payload, right_snapshot.payload);
    assert_eq!(left_snapshot.state_hash, right_snapshot.state_hash);

    let decoded = decode_canonical_snapshot(&left_snapshot.payload).unwrap();
    assert_eq!(decoded.players.len(), MAX_PLAYERS_PER_MATCH);
}
