use std::hint::black_box;
use std::time::Instant;

use battle_royale_core::{
    BattleMatchId, BattleRoyaleMatch, CanonicalMatchSnapshot, MAX_PLAYERS_PER_MATCH, MatchCommand,
    MatchPhase,
};
use battle_royale_game_server::BattleRoyaleGameServerAdapter;
use battle_royale_protocol::encode_command;
use game_server::GameSimulation;

const MATCH_ID: u64 = 0x400;
/// Ticks of active storm before measurement starts, so every batch evaluates storm damage.
/// The warm-up itself follows the storm clock rather than a fixed tick count.
const STORM_ACTIVE_MARGIN_TICKS: u64 = 32;
const TICKS_PER_BATCH: usize = 32;
const WARMUP_BATCHES: usize = 5;
const MEASURED_BATCHES: usize = 31;

fn full_match() -> BattleRoyaleGameServerAdapter {
    let mut simulation = BattleRoyaleGameServerAdapter::new(BattleMatchId::new(MATCH_ID));

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

/// Plays the full match into combat until the storm has been damaging for
/// `STORM_ACTIVE_MARGIN_TICKS`, once, and returns its canonical snapshot.
fn prepared_snapshot() -> CanonicalMatchSnapshot {
    let mut simulation = full_match();
    while !simulation.simulation().storm_snapshot().damage_active {
        simulation.advance_tick().unwrap();
    }
    for _ in 0..STORM_ACTIVE_MARGIN_TICKS {
        simulation.advance_tick().unwrap();
    }

    let simulation = simulation.into_simulation();
    assert_eq!(simulation.player_count(), MAX_PLAYERS_PER_MATCH);
    assert_eq!(simulation.alive_count(), MAX_PLAYERS_PER_MATCH);
    assert_eq!(simulation.phase(), MatchPhase::Combat);
    assert!(simulation.storm_snapshot().damage_active);

    let snapshot = simulation.snapshot().unwrap();
    let restored = BattleRoyaleMatch::from_snapshot(snapshot.clone()).unwrap();
    assert_eq!(restored.snapshot().unwrap(), snapshot);
    snapshot
}

fn prepared_match(snapshot: &CanonicalMatchSnapshot) -> BattleRoyaleGameServerAdapter {
    BattleRoyaleGameServerAdapter::from_simulation(
        BattleRoyaleMatch::from_snapshot(snapshot.clone()).unwrap(),
    )
}

fn tick_batch_ns(snapshot: &CanonicalMatchSnapshot) -> u128 {
    let mut simulation = prepared_match(snapshot);
    let started = Instant::now();

    for _ in 0..TICKS_PER_BATCH {
        simulation.advance_tick().unwrap();
    }

    let elapsed = started.elapsed().as_nanos();

    assert_eq!(simulation.simulation().phase(), MatchPhase::Combat);
    assert_eq!(simulation.simulation().alive_count(), MAX_PLAYERS_PER_MATCH);
    assert!(simulation.simulation().storm_snapshot().damage_active);

    black_box(simulation.current_tick());
    elapsed
}

fn projection_sizes(snapshot: &CanonicalMatchSnapshot) -> (usize, usize, usize, usize) {
    let simulation = prepared_match(snapshot);
    let canonical_bytes = simulation.snapshot().unwrap().payload.len();

    let mut total_player_bytes = 0usize;
    let mut max_player_bytes = 0usize;
    let mut min_player_bytes = usize::MAX;

    for player_id in 1..=u32::try_from(MAX_PLAYERS_PER_MATCH).unwrap() {
        let bytes = simulation.snapshot_for(player_id).unwrap().payload.len();
        total_player_bytes = total_player_bytes.checked_add(bytes).unwrap();
        max_player_bytes = max_player_bytes.max(bytes);
        min_player_bytes = min_player_bytes.min(bytes);
    }

    (
        canonical_bytes,
        total_player_bytes,
        max_player_bytes,
        min_player_bytes,
    )
}

fn main() {
    let snapshot = prepared_snapshot();
    let pre_measure_ticks = snapshot.tick;

    for _ in 0..WARMUP_BATCHES {
        black_box(tick_batch_ns(&snapshot));
    }

    let tick_batch_ns = (0..MEASURED_BATCHES)
        .map(|_| tick_batch_ns(&snapshot))
        .collect::<Vec<_>>();
    let (
        canonical_bytes,
        player_projection_bytes_total,
        player_projection_bytes_max,
        player_projection_bytes_min,
    ) = projection_sizes(&snapshot);

    let samples = tick_batch_ns
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join(",");

    println!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"players\":{},",
            "\"match_id\":{},",
            "\"pre_measure_ticks\":{},",
            "\"ticks_per_batch\":{},",
            "\"warmup_batches\":{},",
            "\"measured_batches\":{},",
            "\"tick_batch_ns\":[{}],",
            "\"canonical_bytes\":{},",
            "\"player_projection_bytes_total\":{},",
            "\"player_projection_bytes_max\":{},",
            "\"player_projection_bytes_min\":{}",
            "}}"
        ),
        MAX_PLAYERS_PER_MATCH,
        MATCH_ID,
        pre_measure_ticks,
        TICKS_PER_BATCH,
        WARMUP_BATCHES,
        MEASURED_BATCHES,
        samples,
        canonical_bytes,
        player_projection_bytes_total,
        player_projection_bytes_max,
        player_projection_bytes_min
    );
}
